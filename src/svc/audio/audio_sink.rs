use std::fmt;
use std::num::NonZero;
use std::sync::Arc;
use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
use std::time::Duration;

use arc_swap::{
    ArcSwap,
    ArcSwapOption,
};
use rodio::DeviceSinkError;
use rodio::{
    MixerDeviceSink,
    Player,
    cpal::{
        BufferSize,
        SampleFormat,
        traits::HostTrait,
    },
    decoder,
    source,
};
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::svc::AudioServiceResponse;
use crate::svc::error::SvcError;
use crate::types::{
    self,
    PlayerState,
};
use crate::{
    config,
    svc::{
        AudioServiceCommand,
        AudioServiceCommandRx,
        CommandWithMeta,
        SingletonService,
        audio::{
            QueueManagerCommand,
            QueueManagerCommandTx,
            SinkEvent,
        },
    },
};

struct DeviceSinkManager {
    conf: config::AudioSink,
    current_mixer: ArcSwapOption<MixerDeviceSink>,
    player_state: ArcSwap<PlayerState>,
    player: ArcSwapOption<Player>,
}

impl fmt::Debug for DeviceSinkManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceSinkManager")
            .field("conf", &self.conf)
            .field("current_mixer", &self.current_mixer)
            .field("player_state", &self.player_state)
            .field(
                "player",
                &format!("The player (None? {})", self.player.load().is_some()),
            )
            .finish()
    }
}

impl DeviceSinkManager {
    pub fn new(conf: config::AudioSink) -> Self {
        Self {
            conf,
            current_mixer: ArcSwapOption::empty(),
            player_state: ArcSwap::new(Arc::new(PlayerState {
                last_command: types::PlayerCommands::Stop,
                is_paused: true,
                volume: 1.0,
            })),
            player: ArcSwapOption::empty(),
        }
    }

    pub fn open_mixer(&self) -> Result<(), SvcError> {
        if self.current_mixer.load().is_none() {
            let mixer = Arc::new(open_audio_device(&self.conf).unwrap());
            self.current_mixer.store(Some(mixer));
        }
        Ok(())
    }

    fn connect_player(&self) {
        let mixer_ = self.current_mixer.load_full();
        if let Some(mixer) = mixer_ {
            let player = Player::connect_new(mixer.mixer());
            // new players spawn with 100% volume
            player.set_volume(self.get_state().volume);
            self.player.swap(Some(Arc::new(player)));
        }
    }

    pub fn drop_player(&self) {
        let old_ = self.player.swap(None);
        if let Some(old) = old_.and_then(|o| Arc::try_unwrap(o).ok()) {
            old.detach();
        } else {
            tracing::debug!(
                "Detaching player failed, will be dropped when all Arcs go out of scope."
            )
        }
    }

    pub fn set_state(&self, player_state: Arc<PlayerState>) {
        self.player_state.store(player_state);
    }

    pub fn get_state(&self) -> Arc<PlayerState> {
        self.player_state.load_full().clone()
    }

    pub fn mixer_is_open(&self) -> bool {
        self.current_mixer.load_full().is_some()
    }

    pub fn has_connected_player(&self) -> bool {
        self.player.load_full().is_some()
    }

    pub fn player(&self) -> Option<Arc<Player>> {
        if !self.has_connected_player() && self.mixer_is_open() {
            self.connect_player();
        }
        self.player.load_full()
    }
}

#[tracing::instrument(level = "info")]
fn open_audio_device(conf: &config::AudioSink) -> Result<MixerDeviceSink, DeviceSinkError> {
    let default_device = rodio::cpal::default_host().default_output_device().unwrap();
    let handle = rodio::DeviceSinkBuilder::from_device(default_device)
        .unwrap()
        .with_buffer_size(BufferSize::Fixed(conf.buffer_size))
        .with_sample_rate(NonZero::new(conf.sampling_rate).unwrap())
        .with_sample_format(SampleFormat::F32)
        // Note that the function below still tries alternative configs if the specified one fails.
        // If you need to only use the exact specified configuration,
        // then use DeviceSinkBuilder::open_sink() instead.
        .open_stream()?;
    tracing::debug!(device=?handle);
    Ok(handle)
}

#[derive(Debug)]
pub struct DefaultAudioSink {
    audio_events_tx: mpsc::Sender<CommandWithMeta<SinkEvent, QueueManagerCommandTx>>,
    command_channel: AudioServiceCommandRx,
    config: config::AudioSink,
}

impl DefaultAudioSink {
    #[tracing::instrument]
    pub fn new(
        conf: config::AudioSink,
        cmds: AudioServiceCommandRx,
        audio_events_tx: mpsc::Sender<CommandWithMeta<SinkEvent, QueueManagerCommandTx>>,
    ) -> Self {
        DefaultAudioSink {
            audio_events_tx,
            command_channel: cmds,
            config: conf,
        }
    }

    #[tracing::instrument(skip_all)]
    async fn command_task(
        mut command_channel: AudioServiceCommandRx,
        q_events_tx: mpsc::Sender<QueueManagerCommand>,
        audio_events_tx: mpsc::Sender<CommandWithMeta<SinkEvent, QueueManagerCommandTx>>,
        device_mixer: Arc<DeviceSinkManager>,
    ) {
        if let Err(e) = device_mixer.open_mixer() {
            tracing::error!(
                msg = "Couldn't open audio device",
                device_mixer = ?device_mixer,
                error=?e
            );
            return;
        }
        while let Some(cmd_and_meta) = command_channel.recv().await {
            tracing::debug!(msg="New command received", cmd=?cmd_and_meta);
            let cmd = cmd_and_meta.cmd;
            let device_mixer_ = device_mixer.clone();
            match cmd {
                AudioServiceCommand::Set(player_state_input) => {
                    let new_command = player_state_input.command;
                    let mut new_volume = device_mixer_.get_state().volume;
                    let player = device_mixer_.player().unwrap();
                    if !device_mixer_.has_connected_player() {
                        let _ = audio_events_tx
                            .send(CommandWithMeta::new(
                                q_events_tx.clone(),
                                SinkEvent::NoMoreSounds,
                            ))
                            .await;
                    }

                    match player_state_input.command {
                        types::PlayerCommands::Play => {
                            if !player.is_paused() {
                                let _ = audio_events_tx
                                    .send(CommandWithMeta::new(
                                        q_events_tx.clone(),
                                        SinkEvent::NoMoreSounds,
                                    ))
                                    .await;
                            }
                            player.play()
                        }
                        types::PlayerCommands::Pause => player.pause(),
                        types::PlayerCommands::Next => {
                            player.clear();
                            let _ = audio_events_tx
                                .send(CommandWithMeta::new(
                                    q_events_tx.clone(),
                                    SinkEvent::NoMoreSounds,
                                ))
                                .await;
                            player.play();
                        }
                        types::PlayerCommands::Stop => {
                            player.clear();
                            let _ = audio_events_tx
                                .send(CommandWithMeta::new(
                                    q_events_tx.clone(),
                                    SinkEvent::Stopped,
                                ))
                                .await;
                        }
                        types::PlayerCommands::Previous => {}
                        types::PlayerCommands::Volume => {
                            new_volume = player_state_input.volume;
                            player.set_volume(new_volume);
                        }
                    }
                    device_mixer_.set_state(Arc::new(PlayerState {
                        last_command: new_command,
                        is_paused: new_command == types::PlayerCommands::Pause,
                        volume: new_volume,
                    }));
                }
                AudioServiceCommand::Get => {}
            };
            // ignore the result.
            let _ = cmd_and_meta
                .response_channel
                .send(AudioServiceResponse::PlayerState(
                    device_mixer_.get_state().clone(),
                ))
                .await;
        }
    }

    #[tracing::instrument(skip_all)]
    async fn queue_events_task(
        mut q_events_rx: mpsc::Receiver<QueueManagerCommand>,
        q_events_tx: mpsc::Sender<QueueManagerCommand>,
        audio_events_tx: mpsc::Sender<CommandWithMeta<SinkEvent, QueueManagerCommandTx>>,
        device_mixer: Arc<DeviceSinkManager>,
    ) {
        let mut interval = sleep(Duration::from_millis(0));
        let device_mixer_ = device_mixer.clone();
        while let Some(cmd) = q_events_rx.recv().await {
            interval.await;
            let device_mixer_ = device_mixer_.clone();
            match cmd {
                QueueManagerCommand::LoadSound(src) => {
                    tracing::debug!(source=?src);
                    if let Some(p) = device_mixer.player() {
                        // add the callback
                        let audio_events_tx = audio_events_tx.clone();
                        let q_events_tx = q_events_tx.clone();

                        let _ = tokio::task::spawn_blocking(move || {
                            tracing::debug!(msg="Player thread started. Appending data", data=?src);
                            match decoder::Decoder::new(src) {
                                Ok(data) => {
                                    let paused = p.is_paused();
                                    p.append(data);
                                    if paused {
                                        p.pause();
                                    }
                                    let has_been_triggered = AtomicBool::new(false);
                                    p.append(source::EmptyCallback::new(Box::new(move || {
                                        if !has_been_triggered.load(Ordering::Relaxed) {
                                            tracing::debug!(
                                                "Done playing! Asking for more sounds."
                                            );
                                            let _ =
                                                audio_events_tx.blocking_send(CommandWithMeta {
                                                    response_channel: q_events_tx.clone(),
                                                    cmd: SinkEvent::NoMoreSounds,
                                                });
                                            has_been_triggered.store(true, Ordering::Relaxed);
                                        }
                                    })));
                                }
                                Err(e) => {
                                    tracing::error!(error = ?e);
                                }
                            }
                        })
                        .await;
                    } else {
                        tracing::error!(
                            msg = "Player couldn't connect to the mixer",
                            device_mixer = ?device_mixer_
                        );
                    };
                }
                QueueManagerCommand::Done => {
                    tracing::debug!("Nothing more in the queue");
                    device_mixer_.drop_player();
                }
                QueueManagerCommand::Wait => {
                    interval = sleep(Duration::from_millis(100));
                    continue;
                }
            };
            interval = sleep(Duration::from_millis(0));
        }
    }
}

impl SingletonService for DefaultAudioSink {
    #[tracing::instrument(skip(self))]
    async fn run(self) {
        let audio_events_tx_ = self.audio_events_tx.clone();
        let (q_events_tx_, q_events_rx) = tokio::sync::mpsc::channel(30);

        let device_manager = Arc::new(DeviceSinkManager::new(self.config));
        let queue_events_task = Self::queue_events_task(
            q_events_rx,
            q_events_tx_.clone(),
            audio_events_tx_.clone(),
            device_manager.clone(),
        );

        let ws_command_task = Self::command_task(
            self.command_channel,
            q_events_tx_.clone(),
            audio_events_tx_.clone(),
            device_manager.clone(),
        );

        tokio::select! {
            r = ws_command_task => {
                tracing::error!(e="ws command connection task ended", payload=?r)
            }
            r = queue_events_task => {
                tracing::error!(e="queue events task ended", payload=?r)

            }
        }
    }
}
