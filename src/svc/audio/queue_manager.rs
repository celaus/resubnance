use std::fmt;
use std::{
    collections::BTreeMap,
    sync::Arc,
};

use arc_swap::ArcSwap;
use tokio::sync::{
    RwLock,
    mpsc,
};

use crate::svc::tracks::{
    Playable,
    PlayableSourceProvider,
};
use crate::svc::{
    CommandWithMeta,
    QueueManagerServiceCommand,
    QueueManagerServiceCommandRx,
    QueueManagerServiceResponse,
    SingletonService,
    audio::{
        QueueManagerCommand,
        QueueManagerCommandTx,
        SinkEvent,
    },
};
use crate::svc::{
    ServiceEvent,
    ServiceEventsTx,
};
use crate::types::QueueState;

#[derive(Debug)]
struct QueueStateManager {
    events: ServiceEventsTx<Arc<QueueState>>,
    inner: ArcSwap<QueueState>,
}

impl QueueStateManager {
    pub fn new(events: ServiceEventsTx<Arc<QueueState>>, inner: ArcSwap<QueueState>) -> Self {
        Self { events, inner }
    }

    pub fn update(&self, new_state: QueueState) {
        let s = Arc::new(new_state);
        self.inner.swap(s.clone());
        let _ = self.events.send(ServiceEvent::StateChange(s));
    }

    pub fn get(&self) -> Arc<QueueState> {
        self.inner.load_full()
    }

    pub fn add_to_downloads(&self, ids: &[String]) {
        let old_queue = self.get();
        let mut q = Arc::unwrap_or_clone(old_queue);
        q.downloading.extend_from_slice(ids);
        self.update(q);
    }

    pub fn remove_from_downloads(&self, ids: &[String]) {
        let old_queue = self.get();
        let mut q = Arc::unwrap_or_clone(old_queue);
        q.downloading.retain(|d| !ids.contains(d));
        self.update(q);
    }
}

/// Service for managing the queue.
pub struct QueueManagerService<S>
where
    S: fmt::Debug + PlayableSourceProvider + Send + 'static,
{
    queue: QueueStateManager,
    sink_events: mpsc::Receiver<CommandWithMeta<SinkEvent, QueueManagerCommandTx>>,
    queue_events: QueueManagerServiceCommandRx,
    playable_track_source: S,
    data_cache: Arc<RwLock<BTreeMap<String, S::PlayableType>>>,
}

impl<S> QueueManagerService<S>
where
    S: fmt::Debug + PlayableSourceProvider + Send + 'static,
{
    #[tracing::instrument]
    pub fn new(
        sink_events: mpsc::Receiver<CommandWithMeta<SinkEvent, QueueManagerCommandTx>>,
        queue_events: QueueManagerServiceCommandRx,
        playable_track_source: S,
        events: ServiceEventsTx<Arc<QueueState>>,
    ) -> Self {
        QueueManagerService {
            sink_events,
            queue_events,
            queue: QueueStateManager::new(events, Default::default()),
            playable_track_source,
            data_cache: Default::default(),
        }
    }

    async fn command_task(
        mut queue_events: QueueManagerServiceCommandRx,
        tx: mpsc::Sender<Vec<String>>,
        q_guard: Arc<QueueStateManager>,
        cache: Arc<RwLock<BTreeMap<String, S::PlayableType>>>,
    ) {
        while let Some(cmd_and_meta) = queue_events.recv().await {
            let cmd = cmd_and_meta.cmd;
            let response = match cmd {
                QueueManagerServiceCommand::ReplaceQueueState(q) => {
                    let old_queue = q_guard.get();
                    if !q.meta.is_empty() {
                        let c = cache.read().await;
                        let ids = q
                            .meta
                            .iter()
                            .map(|m| m.id.clone())
                            .filter(|id| !c.contains_key(id))
                            .collect();
                        drop(c); // make sure the download's rwlock can do write asap.
                        let _ = tx.send(ids).await;
                    }

                    tracing::debug!(msg="Replacing queue", current=?old_queue, new=?q);
                    q_guard.update(old_queue.transition(q));
                    QueueManagerServiceResponse::QueueState(q_guard.get().clone())
                }
                QueueManagerServiceCommand::GetQueueState => {
                    QueueManagerServiceResponse::QueueState(q_guard.get().clone())
                }
            };
            let _ = cmd_and_meta.response_channel.send(response).await;
        }
    }

    async fn sink_service_task(
        mut sink_events: mpsc::Receiver<CommandWithMeta<SinkEvent, QueueManagerCommandTx>>,
        q_guard: Arc<QueueStateManager>,
        cache: Arc<RwLock<BTreeMap<String, S::PlayableType>>>,
    ) {
        while let Some(cmd_and_meta) = sink_events.recv().await {
            let cmd = cmd_and_meta.cmd;
            let response = match cmd {
                SinkEvent::NoMoreSounds => {
                    let state_with_next = q_guard.get().next_track();
                    tracing::debug!(action = "loading music", next_state=?state_with_next);
                    let next_song = state_with_next.currently_playing();
                    q_guard.update(state_with_next);

                    if let Some(song) = next_song {
                        if let Some(song) = cache.read().await.get(&song.id) {
                            let song = song.data().expect("Disk cache fail");
                            QueueManagerCommand::LoadSound(Box::new(song))
                        } else {
                            QueueManagerCommand::Wait
                        }
                    } else {
                        QueueManagerCommand::Done
                    }
                }
                SinkEvent::Stopped => {
                    let state = q_guard.get();
                    q_guard.update(state.stop());
                    QueueManagerCommand::Done
                }
            };
            tracing::debug!(action="loading music", outcome=?response);

            // ignore the result.
            let _ = cmd_and_meta.response_channel.send(response).await;
        }
    }

    async fn downloader_task(
        dl: S,
        cache: Arc<RwLock<BTreeMap<String, S::PlayableType>>>,
        mut rx: mpsc::Receiver<Vec<String>>,
        q_guard: Arc<QueueStateManager>,
    ) {
        while let Some(ids) = rx.recv().await {
            q_guard.add_to_downloads(&ids);
            tracing::debug!(action="downloading", ids=?ids);
            match dl.playable_data(ids.clone()).await {
                Ok(d) => {
                    let mut c = cache.write().await;
                    q_guard.remove_from_downloads(&ids);
                    for (id, playable) in ids.into_iter().zip(d) {
                        c.insert(id, playable);
                    }
                }
                Err(e) => {
                    tracing::warn!(e=?e);
                }
            }
        }
    }
}

impl<S> SingletonService for QueueManagerService<S>
where
    S: fmt::Debug + PlayableSourceProvider + Send + Clone + Sync + 'static,
{
    async fn run(self) {
        let queue_guard = Arc::new(self.queue);

        // download channel for caching
        let (tx, rx) = mpsc::channel::<Vec<String>>(10);
        let ws_command_task = Self::command_task(
            self.queue_events,
            tx,
            queue_guard.clone(),
            self.data_cache.clone(),
        );

        // Task for the connection to/from audio sink service
        let q_guard = queue_guard.clone();

        let sink_service_task =
            Self::sink_service_task(self.sink_events, q_guard, self.data_cache.clone());

        let q_guard = queue_guard.clone();
        let downloader_task = Self::downloader_task(
            self.playable_track_source,
            self.data_cache.clone(),
            rx,
            q_guard,
        );

        tokio::select! {
            r = ws_command_task => {
                tracing::error!(e="ws command connection task ended", payload=?r)
            }
            r = sink_service_task => {
                tracing::error!(e="sink events task ended", payload=?r)

            }
            r = downloader_task => {
                tracing::error!(e="downloader task ended", payload=?r)
            }
        }
    }
}

#[cfg(test)]
mod tests {}
