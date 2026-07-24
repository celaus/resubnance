use std::sync::Arc;

use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};
use serde_with::skip_serializing_none;

/// Track id wrapper.
pub type TrackId = String;

/// Data for changing the queue state.
///
/// # Example
/// ```
/// use resubnance::types::{QueueStateInput, MetaData};
///
/// let input = QueueStateInput {
///     meta: vec![MetaData {
///         id: "track-1".into(),
///         title: "Song".into(),
///         artist: "Artist".into(),
///         duration: 180,
///         elapsed: 0,
///         mime: "audio/mpeg".into(),
///     }],
///     currently_playing_position: Some(0),
/// };
/// ```
#[skip_serializing_none]
#[derive(Clone, Default, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct QueueStateInput {
    pub meta: Vec<MetaData>,
    /// Desired position of the currently playing track _should_ be in.
    #[serde(default)]
    pub currently_playing_position: Option<usize>,
}

/// Input commands for changinge the audio player state.
///
/// # Example
/// ```
/// use resubnance::types::{PlayerStateInput, PlayerCommands};
///
/// let input = PlayerStateInput {
///     command: PlayerCommands::Play,
///     volume: 0.75,
/// };
/// ```
#[derive(Clone, Default, Debug, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub struct PlayerStateInput {
    /// The player command to execute.
    pub command: PlayerCommands,
    /// Volume level from 0.0 (mute) to 1.0 (max).
    pub volume: f32,
}

/// State of the queue.
///
/// # Example
/// ```
/// use resubnance::types::{QueueState, MetaData};
/// use std::sync::Arc;
///
/// let state = QueueState {
///     version: 1,
///     meta: vec![],
///     currently_playing_position: None,
///     next_playing_position: None,
///     downloading: vec![],
/// };
/// ```
#[skip_serializing_none]
#[derive(Clone, Default, Debug, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub struct QueueState {
    /// Monotonically increasing version counter for the queue state. Changes when the meta changes.
    pub version: u64,
    /// Metadata for all tracks in the queue.
    pub meta: Vec<Arc<MetaData>>,
    /// Index of the currently playing track.
    pub currently_playing_position: Option<usize>,
    /// Index of the next track to play.
    pub next_playing_position: Option<usize>,
    /// Track IDs currently being downloaded/not in cache.
    pub downloading: Vec<TrackId>,
}

impl QueueState {
    /// Move the state from one to the next. Clones.
    pub fn transition(&self, value: QueueStateInput) -> Self {
        let next = if value.currently_playing_position != self.currently_playing_position {
            value.currently_playing_position
        } else {
            self.currently_playing_position
                .map(|p| p + 1)
                .or(Some(value.meta.len() - 1))
        };
        QueueState {
            version: self.version + 1,
            meta: value.meta.into_iter().map(Arc::new).collect(),
            currently_playing_position: self.currently_playing_position,
            next_playing_position: next,
            downloading: self.downloading.clone(),
        }
    }

    /// Safely figure out the next track. Returns None if the queue is empty/done playing.
    #[tracing::instrument(level = "debug")]
    pub fn next_track(&self) -> Self {
        match self.next_playing_position {
            Some(pos) if pos < self.meta.len() => QueueState {
                version: self.version,
                meta: self.meta.clone(),
                currently_playing_position: Some(pos),
                next_playing_position: Some(pos + 1),
                downloading: self.downloading.clone(),
            },
            // Restart from the beginning if play is pressed and the queue is played through
            None if self.currently_playing_position.is_none() => QueueState {
                version: self.version,
                meta: self.meta.clone(),
                currently_playing_position: Some(0),
                next_playing_position: Some(1),
                downloading: self.downloading.clone(),
            },
            _ => QueueState {
                version: self.version,
                meta: self.meta.clone(),
                currently_playing_position: None,
                next_playing_position: None,
                downloading: self.downloading.clone(),
            },
        }
    }

    /// Returns the state with the next playing position as the current position and nothing playing right now.
    pub fn stop(&self) -> Self {
        QueueState {
            version: self.version,
            meta: self.meta.clone(),
            currently_playing_position: None,
            next_playing_position: self.currently_playing_position,
            downloading: self.downloading.clone(),
        }
    }

    /// Returns the meta data of the currently playing track (if any).
    pub fn currently_playing(&self) -> Option<Arc<MetaData>> {
        self.currently_playing_position
            .and_then(|pos| self.meta.get(pos))
            .cloned()
    }
}

/// Commands sent to the audio player.
///
/// # Example
/// ```
/// use resubnance::types::PlayerCommands;
///
/// let cmd = PlayerCommands::Play;
/// ```
#[derive(
    Clone, Copy, Default, Debug, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum PlayerCommands {
    Play,
    Pause,
    Next,
    Previous,
    #[default]
    Stop,
    Volume,
}

/// Current state of the audio player.
///
/// # Example
/// ```
/// use resubnance::types::{PlayerState, PlayerCommands};
///
/// let state = PlayerState {
///     last_command: PlayerCommands::Play,
///     is_paused: false,
///     volume: 0.8,
/// };
/// ```
#[derive(Clone, Default, Debug, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub struct PlayerState {
    /// The last command received by the player.
    pub last_command: PlayerCommands,
    pub is_paused: bool,
    /// Current volume level from 0.0 to 1.0 or higher, depending on audio sink.
    pub volume: f32,
}

/// Metadata for a single audio track.
///
/// # Example
/// ```
/// use resubnance::types::MetaData;
///
/// let meta = MetaData {
///     id: "track-1".into(),
///     title: "Bohemian Rhapsody".into(),
///     artist: "Queen".into(),
///     duration: 354,
///     elapsed: 0,
///     mime: "audio/mpeg".into(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub struct MetaData {
    /// Unique identifier for the track provided by subsonic.
    pub id: TrackId,
    pub title: String,
    pub artist: String,
    /// Duration of the track in seconds.
    pub duration: u64,
    /// Elapsed playback time in seconds.
    pub elapsed: u64,
    /// (MIME) type of the audio file.
    pub mime: String,
}

/// A playlist from subsonic.
///
/// # Example
/// ```
/// use resubnance::types::{Playlist, MetaData};
///
/// let playlist = Playlist {
///     id: "pl-1".into(),
///     name: "My Playlist".into(),
///     tracks: vec![],
///     duration: 0,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub struct Playlist {
    pub id: String,
    /// Display name of the playlist.
    pub name: String,
    /// Tracks contained in the playlist.
    pub tracks: Vec<MetaData>,
    /// Total duration of all tracks in seconds.
    pub duration: u64,
}
