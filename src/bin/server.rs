#![allow(clippy::result_large_err)]
use std::env;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::routing::{
    delete,
    get,
};
use resubnance::web::webapp::{
    cache_delete_all,
    cache_delete_by_id,
};
use resubnance::web::ws::{
    self,
    WebSocketMessageHandler,
};
use resubnance::web::{
    WebAppState,
    WebSocketState,
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
        cache,
        webapp,
    },
};
use tower_http::{
    services::ServeFile,
    trace::{
        DefaultMakeSpan,
        TraceLayer,
    },
};

use resubnance::config;
use tokio::net::TcpListener;
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
    tracing::debug!(config=?config, "scanning library");
    // A test instance
    source_svc_factory.get_instance().await.init().await?;

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

    let dl = source_svc_factory.get_instance().await;
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

    let webapp_state = WebAppState {
        config: Arc::new(config.clone()),
    };
    let srv = Router::new()
        .merge(
            Router::new()
                .route("/", get(webapp))
                .route("/cache", get(cache)),
        )
        .with_state(webapp_state.clone())
        .nest(
            "/api",
            Router::new()
                .route("/cache", delete(cache_delete_all))
                .route("/cache/{id}", delete(cache_delete_by_id))
                .with_state(webapp_state.clone()),
        )
        .route_service("/favicon.svg", ServeFile::new("static/favicon.svg"))
        .route_service("/style.css", ServeFile::new("static/style.css"))
        .route_service("/handlers.js", ServeFile::new("static/handlers.js"))
        .nest(
            "/schema",
            Router::new()
                .route("/request", get(ws::ws_schema_requests))
                .route("/response", get(ws::ws_schema_responses)),
        )
        .route(
            "/ws",
            get(ws::ws_control).with_state(WebSocketState {
                music_source_factory: Arc::new(ws_api),
                events_rx: Arc::new(q_state_events_rx),
            }),
        )
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        );
    axum::serve(
        TcpListener::bind(&config.server.url).await?,
        srv.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
