use std::sync::Arc;

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

#[derive(Clone, Debug)]
pub struct WebSocketState {
    pub music_source_factory: Arc<WebSocketMessageHandler<SubsonicMusicSourceFactory>>,
    pub events_rx: Arc<ServiceEventsRx<Arc<QueueState>>>,
}
