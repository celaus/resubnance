use std::{
    env,
    path::PathBuf,
    sync::Arc,
};

use crate::{
    config::ResubnanceConfig,
    svc::{
        ServiceEventsRx,
        tracks::subsonic::SubsonicMusicSourceFactory,
    },
    types::QueueState,
    web::ws::WebSocketMessageHandler,
};

pub mod webapp;

pub mod ws;

#[derive(Clone, Debug)]
pub struct WebAppState {
    pub config: Arc<ResubnanceConfig>,
}

impl WebAppState {
    pub fn cache_dir_or_env_tempdir(&self) -> PathBuf {
        self.config
            .subsonic
            .caching_strategy
            .cache_dir()
            .unwrap_or(env::temp_dir())
    }
}

#[derive(Clone, Debug)]
pub struct WebSocketState {
    pub music_source_factory: Arc<WebSocketMessageHandler<SubsonicMusicSourceFactory>>,
    pub events_rx: Arc<ServiceEventsRx<Arc<QueueState>>>,
}
