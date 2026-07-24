use thiserror::Error;
use tokio::task::JoinError;

use crate::svc::tracks::TrackSourceError;

#[derive(Debug, Error)]
pub enum SvcError {
    #[error("Invalid configuration: {0:?}")]
    Config(#[from] Box<figment::Error>),
    #[error("I/O error: {0:?}")]
    Io(#[from] std::io::Error),
    // #[error("Async I/O error: {0:?}")]
    // AsyncIo(#[from]tokio::io::Error)
    #[error("Internal Error")]
    Internal(),
    #[error("{0}")]
    Source(#[from] TrackSourceError),
}

#[allow(clippy::from_over_into)]
impl Into<i32> for SvcError {
    fn into(self) -> i32 {
        match self {
            SvcError::Config(_) => 1,
            SvcError::Io(_) => 2,
            SvcError::Internal() => 3,
            SvcError::Source(_) => 4,
        }
    }
}

impl From<JoinError> for SvcError {
    #[tracing::instrument(level = "debug")]
    fn from(value: JoinError) -> Self {
        SvcError::Internal()
    }
}
