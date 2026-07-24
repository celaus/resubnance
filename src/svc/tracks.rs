use std::{
    fmt,
    io::{
        self,
        Read,
        Seek,
    },
};

use crate::{
    svc::{
        error::{
            self,
            SvcError,
        },
        tracks::playback_strategy::PlaybackStrategyAdapter,
    },
    types::{
        MetaData,
        Playlist,
    },
};

use thiserror::Error;

mod playable;
pub mod playback_strategy;
pub mod subsonic;

pub(crate) const MUSIC_FORMAT_EXT: &str = "mp3";

/// Errors for track sources.
#[derive(Debug, Error)]
pub enum TrackSourceError {
    /// Error from the Subsonic API client.
    #[error("Subsonic API Error: {0}")]
    SubsonicSourceError(#[from] sunk::Error),
}

/// Trait for playable audio source types for sunk.
pub trait PlayableAudioSourceType: fmt::Debug + Read + Seek + Sync + Send + 'static {}

/// Trait accessing the binary data to send to the player.
pub trait Playable {
    type Output: PlayableAudioSourceType;

    /// Returns the audio source data for playback.
    fn data(&self) -> io::Result<Self::Output>;

    /// Initializes the playable data (e.g. pre-load the cache)
    fn init<L: PlaybackStrategyAdapter>(&mut self, loading_strategy: L) -> io::Result<()>;
}

/// Trait for accessing the binary data for the provided ids.
pub trait PlayableSourceProvider {
    type PlayableType: Playable + Send + Sync;

    /// Return playable audio data for the given track IDs. Should fail early.
    fn playable_data(
        &self,
        ids: Vec<String>,
    ) -> impl std::future::Future<Output = Result<Vec<Self::PlayableType>, error::SvcError>> + Send;
}

/// Trait for services providing track metadata through search/random/playlists/. Implement using `async`.
pub trait TracklistSourceProvider {
    /// Initializes the track source (e.g., connects to a remote API).
    fn init(&mut self) -> impl std::future::Future<Output = Result<(), SvcError>> + Send;

    /// Returns (up to?) `n` random tracks.
    fn random(
        &self,
        n: usize,
    ) -> impl std::future::Future<Output = Result<Vec<MetaData>, SvcError>> + Send;

    /// Searches the source across all properties (artist/title/...).
    fn search(
        &self,
        query: String,
    ) -> impl std::future::Future<Output = Result<Vec<MetaData>, SvcError>> + Send;

    /// Returns all accessible playlists.
    fn playlists(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<Playlist>, SvcError>> + Send;

    /// Looks up metadata for the given track IDs.
    fn lookup(
        &self,
        ids: Vec<String>,
    ) -> impl std::future::Future<Output = Vec<Result<MetaData, SvcError>>> + Send;
}

#[cfg(test)]
pub mod mock;
