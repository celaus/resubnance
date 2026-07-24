use crate::{
    svc::{
        AudioServiceCommand,
        AudioServiceCommandTx,
        CommandWithMeta,
        QueueManagerServiceCommand,
        QueueManagerServiceCommandTx,
        ServiceEvent,
        ServiceFactory,
        error::SvcError,
        tracks::TracklistSourceProvider,
    },
    types::{
        MetaData,
        QueueStateInput,
    },
    web::WebSocketState,
};
use axum::{
    extract::{
        State,
        ws::{
            Message,
            WebSocketUpgrade,
        },
    },
    response::IntoResponse,
};
use futures::{
    SinkExt,
    StreamExt,
};

use std::{
    fmt,
    sync::Arc,
};
use tokio::sync::mpsc;
mod types;

use types::*;
pub use types::{
    print_all_possible_messages,
    ws_schema_requests,
    ws_schema_responses,
};

#[derive(Clone, Debug)]
pub struct WebSocketMessageHandler<S>
where
    S: ServiceFactory<Service: TracklistSourceProvider> + fmt::Debug,
{
    queue: QueueManagerServiceCommandTx,
    sink: AudioServiceCommandTx,
    sources: Arc<S>,
}

impl<S> WebSocketMessageHandler<S>
where
    S: ServiceFactory<Service: TracklistSourceProvider> + fmt::Debug,
{
    pub fn new(
        sources: Arc<S>,
        queue: QueueManagerServiceCommandTx,
        sink: AudioServiceCommandTx,
    ) -> Self {
        Self {
            sources,
            queue,
            sink,
        }
    }

    async fn pong(&self, tx: mpsc::Sender<WsResponseInternal>) {
        let _ = tx.send(WsResponseInternal::Pong).await;
    }

    async fn close(&self, tx: mpsc::Sender<WsResponseInternal>) {
        let _ = tx.send(WsResponseInternal::Close).await;
    }

    async fn send_to_queue(&self, cmd: QueueManagerServiceCommand) -> WsResponse {
        let (tx, mut rx) = mpsc::channel(1);
        let _ = self.queue.send(CommandWithMeta::new(tx, cmd)).await;
        match rx.recv().await {
            Some(response) => WsResponse::from(response),
            None => {
                tracing::error!("Sender dropped?");
                WsResponse::error(SvcError::Internal())
            }
        }
    }

    async fn send_to_backend(&self, cmd: AudioServiceCommand) -> WsResponse {
        let (tx, mut rx) = mpsc::channel(1);

        let _ = self.sink.send(CommandWithMeta::new(tx, cmd)).await;
        match rx.recv().await {
            Some(response) => WsResponse::from(response),
            None => {
                tracing::error!("Sender dropped?");
                WsResponse::error(SvcError::Internal())
            }
        }
    }

    async fn handle(self, msg: WsRequest, tx: mpsc::Sender<WsResponseInternal>) {
        let src_svc = self.sources.get_instance().await;
        let response = match msg.rq {
            WsControlRq::Player(control) => {
                tracing::info!(control=?control);
                match control {
                    Player::Set(player_state_input) => {
                        self.send_to_backend(AudioServiceCommand::Set(player_state_input))
                            .await
                    }
                    Player::Get => self.send_to_backend(AudioServiceCommand::Get).await,
                }
            }
            WsControlRq::Queue(queue_control) => match queue_control {
                Queue::Set(q) => {
                    // the input is only an id, the metadata is fetched here (not the frontend)
                    let lookup_data: Result<Vec<MetaData>, _> = self
                        .sources
                        .get_instance()
                        .await
                        .lookup(q.meta)
                        .await
                        .into_iter()
                        .collect();

                    match lookup_data {
                        Ok(queue) => {
                            self.send_to_queue(QueueManagerServiceCommand::ReplaceQueueState(
                                QueueStateInput {
                                    meta: queue,
                                    currently_playing_position: q.currently_playing_position,
                                },
                            ))
                            .await
                        }
                        Err(e) => WsResponse::error(e),
                    }
                }
                Queue::Get => {
                    self.send_to_queue(QueueManagerServiceCommand::GetQueueState)
                        .await
                }
            },
            WsControlRq::Search(query) => match src_svc.search(query).await {
                Ok(result) => {
                    let tracks = result.into_iter().collect();
                    WsResponse::from(WsControlRp::SearchResults(tracks))
                }
                Err(e) => WsResponse::error(e),
            },
            WsControlRq::Playlists => match src_svc.playlists().await {
                Ok(playlists) => WsResponse::from(WsControlRp::Playlists(
                    playlists
                        .into_iter()
                        .map(|p| Playlist {
                            name: p.name,
                            tracks: p.tracks,
                        })
                        .collect::<Vec<_>>(),
                )),
                Err(e) => WsResponse::error(e),
            },
            WsControlRq::Unknown => WsResponse::with_error_msg("Unknown request"),
        };
        let _ = tx.send(WsResponseInternal::Data(response)).await;
    }
}

pub async fn ws_control(
    State(state): State<WebSocketState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let controls = state.music_source_factory;
    let mut queue_state_change_events = state.events_rx.resubscribe();
    ws.on_upgrade(move |socket| async move {

        let (mut response_stream, mut rx) = socket.split();
        let (tx, mut outgoing_rx) = mpsc::channel::<WsResponseInternal>(10);
        let tx_ = tx.clone();

        // Send responses to clients
        tokio::spawn(async move {
            loop {
                if let Some(response) = outgoing_rx.recv().await {
                    match response {
                        WsResponseInternal::Data(response) =>{
                            if let Ok(ws_response) = serde_json::to_string(&response) {
                        let _ = response_stream.send(Message::Text(ws_response.into())).await;
                    } else {
                        tracing::warn!(msg="Couldn't serialize response", response=?response);
                    }
                        },
                        WsResponseInternal::Pong => {let _ = response_stream.send(Message::Pong(vec![].into())).await;},
                        WsResponseInternal::Close => {let _ = response_stream.send(Message::Close(None)).await;}
                    }
                } else {
                    tracing::warn!(msg="Response channel closed");
                    break;
                }
            }
        });

        // Message handler
        tokio::spawn(async move {
            // "Welcome" messages
            Arc::unwrap_or_clone(controls.clone()).handle(WsRequest { rq: WsControlRq::Player(Player::Get) }, tx.clone()).await;
            Arc::unwrap_or_clone(controls.clone()).handle(WsRequest { rq: WsControlRq::Queue(Queue::Get) }, tx.clone()).await;
            Arc::unwrap_or_clone(controls.clone()).handle(WsRequest { rq: WsControlRq::Playlists }, tx.clone()).await;

            while let Some(Ok(msg)) = rx.next().await {
                    let handler = Arc::unwrap_or_clone(controls.clone());
                    match msg {
                        Message::Text(text) => match serde_json::from_str::<WsRequest>(&text) {
                            Ok(cmd) => {
                                tokio::spawn(handler.handle(cmd, tx.clone()));
                            }
                            Err(e) => {
                                tracing::warn!(msg="Incoming message invalid", raw=?text, error=?e);
                                // don't tell the user?
                                handler.close(tx.clone()).await;
                            }
                        },
                        Message::Close(_) => {
                            handler.close(tx.clone()).await;
                        }
                        Message::Ping(_) => {
                            handler.pong(tx.clone()).await;
                        }
                        _ => (),
                    }
            }
        });

        // State events
        tokio::spawn(async move {
            while let Ok(event) = queue_state_change_events.recv().await {
                match event {
                    ServiceEvent::StateChange(new_state) => {
                        let _ = tx_.send(WsResponseInternal::Data(WsResponse::from(new_state))).await;
                }
            }
            }
        });
    })
}

#[cfg(test)]
mod tests;
