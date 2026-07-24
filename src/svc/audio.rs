use tokio::sync::mpsc;

use crate::svc::tracks::PlayableAudioSourceType;

pub mod audio_sink;
pub mod queue_manager;

/// Sender for queue manager commands between the audio sink and the queue manager.
pub type QueueManagerCommandTx = mpsc::Sender<QueueManagerCommand>;

/// Receiver for queue manager commands between the audio sink and the queue manager.
pub type QueueManagerCommandRx = mpsc::Receiver<QueueManagerCommand>;

/// Events emitted by the audio sink service.
#[derive(Debug, Clone)]
pub enum SinkEvent {
    /// No more audio data available to play. When nothing is available, it's a good idea to emit this event periodically.
    NoMoreSounds,
    // Stopped the playback
    Stopped,
}

/// Commands sent to the audio sink for audio playback control.
#[derive(Debug)]
pub enum QueueManagerCommand {
    /// Load the provided audio data.
    LoadSound(Box<dyn PlayableAudioSourceType>),
    /// Audio source data not ready yet.
    Wait,
    /// No more audio sources.
    Done,
}
