#![allow(clippy::result_large_err)]
use std::env;
use std::path::Path;
use std::sync::Arc;

use poem::endpoint::StaticFileEndpoint;
use poem::middleware::AddData;
use poem::{
    EndpointExt,
    delete,
    get,
};
use resubnance::web::webapp::EmptyCacheEp;
use resubnance::web::ws::{
    self,
    WebSocketMessageHandler,
};
use resubnance::{
    svc::{
        ServiceFactory,
        audio::{
            audio_sink::DefaultAudioSink,
            queue_manager::QueueManagerService,
        },
        error::SvcError,
        ext,
        supervisor,
        tracks::{
            TracklistSourceProvider,
            subsonic::SubsonicMusicSourceFactory,
        },
    },
    web::webapp::{
        DeleteIdFromCacheEp,
        cache,
        webapp,
    },
};

use poem::{
    Route,
    listener::TcpListener,
};

use resubnance::config;
use tokio::sync::broadcast;

const CHANNEL_SIZE: usize = 20;

#[tokio::main]
async fn main() -> Result<(), SvcError> {
    // install global subscriber configured based on RUST_LOG envvar.
    tracing_subscriber::fmt::init();

    ws::print_all_possible_messages();

    let config: config::ResubnanceConfig = config::parse(Path::new("config.toml"))?;
    tracing::info!(
        "🔉 Welcome to {}, v{}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );
    let source_svc_factory = SubsonicMusicSourceFactory::new(config.subsonic.clone());

    // A test instance
    source_svc_factory.get_instance().init().await.unwrap();

    let (audio_sink_tx, audio_sink_rx) = tokio::sync::mpsc::channel(CHANNEL_SIZE);
    let (player_events_tx, player_events_rx) = tokio::sync::mpsc::channel(CHANNEL_SIZE);
    let (queue_mgr_tx, queue_mgr_rx) = tokio::sync::mpsc::channel(CHANNEL_SIZE);
    let (q_state_events_tx, q_state_events_rx) = tokio::sync::broadcast::channel(CHANNEL_SIZE);

    let audio_conf = config.audio.clone();
    let sink_svc = tokio::task::spawn_blocking(|| {
        DefaultAudioSink::new(audio_conf, audio_sink_rx, player_events_tx)
    })
    .await
    .unwrap();

    let dl = source_svc_factory.get_instance();
    let queue_mgr_svc = tokio::task::spawn_blocking(|| {
        QueueManagerService::new(player_events_rx, queue_mgr_rx, dl, q_state_events_tx)
    })
    .await
    .unwrap();

    let (ext_tx, _) = broadcast::channel(20); //20 may be high or low?
    let mut supervisor = supervisor::ServiceSupervisor::new(ext_tx.clone());

    for externals in config.additional.iter() {
        match externals {
            config::ExternalServices::CoverImageUploader { .. } => {
                let s = ext::cover_img::CoverImageDisplayService::new(
                    externals.clone(),
                    supervisor.external_rx(),
                );
                supervisor.start(s).await;
            }
            config::ExternalServices::Other => {}
        }
    }
    supervisor.start(sink_svc).await;
    supervisor.start(queue_mgr_svc).await;

    let ws_api = WebSocketMessageHandler::new(
        source_svc_factory.clone().into(),
        queue_mgr_tx.clone(),
        audio_sink_tx.clone(),
    );
    let cache_dir = config
        .subsonic
        .caching_strategy
        .cache_dir()
        .unwrap_or(env::temp_dir());

    let srv = Route::new()
        .at("/", get(webapp.data(config.clone())))
        .at("/cache", get(cache.data(config.clone())))
        .at("/api/cache", delete(EmptyCacheEp::new(cache_dir.clone())))
        .at(
            "/api/cache/:id",
            delete(DeleteIdFromCacheEp::new(cache_dir.clone())),
        )
        .at(
            "/favicon.svg",
            StaticFileEndpoint::new("static/favicon.svg"),
        )
        .at("/style.css", StaticFileEndpoint::new("static/style.css"))
        .at(
            "/handlers.js",
            StaticFileEndpoint::new("static/handlers.js"),
        )
        .nest(
            "/schema",
            Route::new()
                .at("/request", ws::ws_schema_requests)
                .at("/response", ws::ws_schema_responses),
        )
        .at(
            "/ws",
            get(ws::ws_control).with(AddData::new((
                Arc::new(ws_api),
                Arc::new(q_state_events_rx),
            ))),
        );

    poem::Server::new(TcpListener::bind(&config.server.url))
        .run(srv)
        .await
        .map_err(SvcError::Io)
}
