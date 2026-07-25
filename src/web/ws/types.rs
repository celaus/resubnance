use std::sync::Arc;

use axum::response::Response;

use serde::{
    Deserialize,
    Serialize,
};

use schemars::{
    JsonSchema,
    schema_for,
};

use crate::{
    svc::{
        AudioServiceResponse,
        QueueManagerServiceResponse,
        error::SvcError,
        tracks::MUSIC_FORMAT_EXT,
    },
    types::{
        MetaData,
        PlayerCommands,
        PlayerState,
        PlayerStateInput,
        QueueState,
        TrackId,
    },
};

#[derive(Clone, Default, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub struct QueueStateInputRq {
    pub meta: Vec<TrackId>,
    #[serde(default)]
    pub currently_playing_position: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase", tag = "t", content = "c")]
pub enum Player {
    Set(PlayerStateInput),
    Get,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase", tag = "t", content = "c")]
pub enum Queue {
    Set(QueueStateInputRq),
    Get,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase", tag = "t", content = "c")]
pub enum WsControlRq {
    Player(Player),
    Queue(Queue),

    Search(String),
    Playlists,

    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub struct Playlist {
    pub name: String,
    pub tracks: Vec<MetaData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase", tag = "t", content = "c")]
pub enum WsControlRp {
    SearchResults(Vec<MetaData>),
    Playlists(Vec<Playlist>),

    QueueState(Arc<QueueState>),
    PlayerState(Arc<PlayerState>),
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub struct WsResponse {
    pub rp: Option<WsControlRp>,
    pub error: Option<String>,
}

impl WsResponse {
    pub fn error(e: SvcError) -> Self {
        e.into()
    }

    pub fn with_error_msg<S: Into<String>>(e: S) -> Self {
        Self {
            error: Some(e.into()),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub enum WsResponseInternal {
    Data(WsResponse),
    Pong,
    Close,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub struct WsRequest {
    pub rq: WsControlRq,
}

impl From<WsControlRp> for WsResponse {
    fn from(value: WsControlRp) -> Self {
        WsResponse {
            rp: Some(value),
            ..Default::default()
        }
    }
}

impl From<AudioServiceResponse> for WsResponse {
    fn from(value: AudioServiceResponse) -> Self {
        let mut r = WsResponse::default();
        match value {
            AudioServiceResponse::Error(svc_error) => r.error = Some(svc_error.to_string()),
            AudioServiceResponse::PlayerState(state) => {
                r.rp = Some(WsControlRp::PlayerState(state))
            }
            AudioServiceResponse::Unknown => {
                // ???
            }
        };
        r
    }
}

impl From<QueueManagerServiceResponse> for WsResponse {
    fn from(value: QueueManagerServiceResponse) -> Self {
        let mut r = WsResponse::default();
        match value {
            QueueManagerServiceResponse::QueueState(state) => {
                r.rp = Some(WsControlRp::QueueState(state))
            }
            QueueManagerServiceResponse::Error(svc_error) => r.error = Some(svc_error.to_string()),
        };
        r
    }
}

impl From<Arc<QueueState>> for WsResponse {
    fn from(value: Arc<QueueState>) -> Self {
        Self {
            rp: Some(WsControlRp::QueueState(value)),
            ..Default::default()
        }
    }
}

impl From<SvcError> for WsResponse {
    fn from(value: SvcError) -> Self {
        WsResponse {
            error: Some(value.to_string()),
            ..Default::default()
        }
    }
}

pub fn print_all_possible_messages() {
    let rq = [
        WsRequest {
            rq: WsControlRq::Player(Player::Get),
        },
        WsRequest {
            rq: WsControlRq::Player(Player::Set(PlayerStateInput {
                command: PlayerCommands::Play,
                volume: 0.5,
            })),
        },
        WsRequest {
            rq: WsControlRq::Queue(Queue::Set(QueueStateInputRq {
                meta: vec![],
                currently_playing_position: Some(1),
            })),
        },
        WsRequest {
            rq: WsControlRq::Search("query".into()),
        },
        WsRequest {
            rq: WsControlRq::Playlists,
        },
        WsRequest {
            rq: WsControlRq::Unknown,
        },
    ];

    for msg in rq {
        let formatted = serde_json::to_string_pretty(&msg).unwrap();
        tracing::debug!("{}", formatted)
    }

    let rp = [
        WsResponse {
            rp: None,
            error: Some("error".into()),
        },
        WsResponse {
            rp: Some(WsControlRp::Playlists(vec![Playlist {
                name: "name".into(),
                tracks: vec![MetaData {
                    title: "title".into(),
                    artist: "artist".into(),
                    duration: 100,
                    id: uuid::Uuid::new_v4().to_string(),
                    mime: MUSIC_FORMAT_EXT.into(),
                    elapsed: 10,
                }],
            }])),
            error: None,
        },
        WsResponse {
            rp: Some(WsControlRp::SearchResults(vec![MetaData {
                title: "title".into(),
                artist: "artist".into(),
                duration: 100,
                id: uuid::Uuid::new_v4().to_string(),
                mime: MUSIC_FORMAT_EXT.into(),
                elapsed: 10,
            }])),
            error: None,
        },
        WsResponse {
            rp: Some(WsControlRp::QueueState(
                QueueState {
                    version: 1,
                    meta: vec![
                        MetaData {
                            title: "title".into(),
                            artist: "artist".into(),
                            duration: 100,
                            id: uuid::Uuid::new_v4().to_string(),
                            mime: MUSIC_FORMAT_EXT.into(),
                            elapsed: 10,
                        }
                        .into(),
                    ],
                    currently_playing_position: Some(0),
                    next_playing_position: Some(1),
                    downloading: vec![uuid::Uuid::new_v4().to_string()],
                }
                .into(),
            )),
            error: None,
        },
        WsResponse {
            rp: Some(WsControlRp::PlayerState(
                PlayerState {
                    last_command: PlayerCommands::Play,
                    is_paused: false,
                    volume: 1.0,
                }
                .into(),
            )),
            error: None,
        },
    ];

    for msg in rp {
        let formatted = serde_json::to_string_pretty(&msg).unwrap();
        tracing::debug!("{}", formatted)
    }
}

const SCHEMA_CONTENT_TYPE: &str = "application/schema+json";

pub async fn ws_schema_requests() -> Response {
    let request = schema_for!(WsRequest);
    let schema = serde_json::to_string_pretty(&request).unwrap();
    Response::builder()
        .header(http::header::CONTENT_TYPE, SCHEMA_CONTENT_TYPE)
        .body(schema.into())
        .unwrap()
}

pub async fn ws_schema_responses() -> Response {
    let response = schema_for!(WsResponse);
    let schema = serde_json::to_string_pretty(&response).unwrap();
    Response::builder()
        .header(http::header::CONTENT_TYPE, SCHEMA_CONTENT_TYPE)
        .body(schema.into())
        .unwrap()
}
