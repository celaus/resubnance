use tokio::sync::mpsc;

use crate::{
    svc::{
        AudioServiceCommand,
        AudioServiceCommandRx,
        AudioServiceResponse,
        QueueManagerServiceCommand,
        QueueManagerServiceCommandRx,
        QueueManagerServiceResponse,
        tracks::mock::MockSubsonicMusicSourceFactory,
    },
    types::{
        MetaData,
        PlayerCommands,
        PlayerState,
        PlayerStateInput,
        QueueState,
        QueueStateInput,
    },
    web::ws::{
        WebSocketMessageHandler,
        types::*,
    },
};
use std::sync::Arc;

fn ws_handler() -> (
    WebSocketMessageHandler<MockSubsonicMusicSourceFactory>,
    AudioServiceCommandRx,
    QueueManagerServiceCommandRx,
) {
    let (queue_mgr_tx, queue_mgr_rx) = tokio::sync::mpsc::channel(20);
    let (audio_sink_tx, audio_sink_rx) = tokio::sync::mpsc::channel(20);
    let source_svc_factory = MockSubsonicMusicSourceFactory::new();

    (
        WebSocketMessageHandler::<MockSubsonicMusicSourceFactory>::new(
            source_svc_factory.into(),
            queue_mgr_tx,
            audio_sink_tx,
        ),
        audio_sink_rx,
        queue_mgr_rx,
    )
}

async fn ws_audio_handler_roundtrip(
    expected_cmd: AudioServiceCommand,
    request: WsRequest,
    response: AudioServiceResponse,
) -> WsResponseInternal {
    let (handler, mut audio_rx, _queue_rx) = ws_handler();
    let (tx, mut rx) = mpsc::channel(1);
    tokio::spawn(handler.handle(request, tx));
    let player_cmd = audio_rx.recv().await.unwrap();
    assert_eq!(player_cmd.cmd, expected_cmd);
    player_cmd.response_channel.send(response).await.unwrap();
    rx.recv().await.unwrap()
}

async fn ws_queue_handler_roundtrip(
    expected_cmd: QueueManagerServiceCommand,
    request: WsRequest,
    response: QueueManagerServiceResponse,
) -> WsResponseInternal {
    let (handler, _audio_rx, mut queue_rx) = ws_handler();
    let (tx, mut rx) = mpsc::channel(1);
    tokio::spawn(handler.handle(request, tx));
    let player_cmd = queue_rx.recv().await.unwrap();
    assert_eq!(player_cmd.cmd, expected_cmd);
    player_cmd.response_channel.send(response).await.unwrap();
    rx.recv().await.unwrap()
}

async fn ws_source_handler_roundtrip(request: WsRequest) -> WsResponseInternal {
    let (handler, _audio_rx, _queue_rx) = ws_handler();
    let (tx, mut rx) = mpsc::channel(1);
    tokio::spawn(handler.handle(request, tx));
    rx.recv().await.unwrap()
}

#[tokio::test]
async fn test_ws_handler_player_get() {
    let expected = PlayerState {
        last_command: PlayerCommands::Play,
        is_paused: false,
        volume: 0.8,
    };
    let WsResponseInternal::Data(response) = ws_audio_handler_roundtrip(
        AudioServiceCommand::Get,
        WsRequest {
            rq: WsControlRq::Player(Player::Get),
        },
        AudioServiceResponse::PlayerState(Arc::new(expected.clone())),
    )
    .await
    else {
        panic!("unexpected response")
    };
    assert!(response.error.is_none());
    let WsControlRp::PlayerState(actual) = response.rp.unwrap() else {
        panic!("unexpected inner response")
    };
    assert_eq!(Arc::into_inner(actual).unwrap(), expected);
}

#[tokio::test]
async fn test_ws_handler_player_set() {
    let input = PlayerStateInput {
        command: PlayerCommands::Pause,
        volume: 0.5,
    };
    let expected = PlayerState {
        last_command: PlayerCommands::Pause,
        is_paused: true,
        volume: 0.5,
    };
    let WsResponseInternal::Data(response) = ws_audio_handler_roundtrip(
        AudioServiceCommand::Set(input.clone()),
        WsRequest {
            rq: WsControlRq::Player(Player::Set(input)),
        },
        AudioServiceResponse::PlayerState(Arc::new(expected.clone())),
    )
    .await
    else {
        panic!("unexpected response")
    };
    assert!(response.error.is_none());
    let WsControlRp::PlayerState(actual) = response.rp.unwrap() else {
        panic!("unexpected inner response")
    };
    assert_eq!(Arc::into_inner(actual).unwrap(), expected);
}

#[tokio::test]
async fn test_ws_handler_queue_get() {
    let expected = QueueState {
        version: 1,
        meta: vec![
            Arc::new(MetaData {
                id: "track-1".into(),
                title: "Track 1".into(),
                artist: "Artist 1".into(),
                duration: 180,
                elapsed: 0,
                mime: "audio/mpeg".into(),
            }),
            Arc::new(MetaData {
                id: "track-2".into(),
                title: "Track 2".into(),
                artist: "Artist 2".into(),
                duration: 200,
                elapsed: 0,
                mime: "audio/mpeg".into(),
            }),
        ],
        currently_playing_position: Some(0),
        next_playing_position: Some(1),
        downloading: vec![],
    };
    let WsResponseInternal::Data(response) = ws_queue_handler_roundtrip(
        QueueManagerServiceCommand::GetQueueState,
        WsRequest {
            rq: WsControlRq::Queue(Queue::Get),
        },
        QueueManagerServiceResponse::QueueState(Arc::new(expected.clone())),
    )
    .await
    else {
        panic!("unexpected response")
    };
    assert!(response.error.is_none());
    let WsControlRp::QueueState(actual) = response.rp.unwrap() else {
        panic!("unexpected inner response")
    };
    let actual = Arc::into_inner(actual).unwrap();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_ws_handler_queue_set() {
    let track_ids = vec!["track-001".into(), "track-002".into()];
    let input = QueueStateInputRq {
        meta: track_ids.clone(),
        currently_playing_position: Some(0),
    };
    let expected_meta = vec![
        MetaData {
            id: "track-001".into(),
            title: "Bohemian Rhapsody".into(),
            artist: "Queen".into(),
            duration: 354,
            elapsed: 0,
            mime: "audio/mpeg".into(),
        },
        MetaData {
            id: "track-002".into(),
            title: "Stairway to Heaven".into(),
            artist: "Led Zeppelin".into(),
            duration: 482,
            elapsed: 0,
            mime: "audio/mpeg".into(),
        },
    ];
    let expected = QueueState {
        version: 1,
        meta: expected_meta.clone().into_iter().map(Arc::new).collect(),
        currently_playing_position: Some(0),
        next_playing_position: Some(1),
        downloading: vec![],
    };
    let WsResponseInternal::Data(response) = ws_queue_handler_roundtrip(
        QueueManagerServiceCommand::ReplaceQueueState(QueueStateInput {
            meta: expected_meta.clone(),
            currently_playing_position: Some(0),
        }),
        WsRequest {
            rq: WsControlRq::Queue(Queue::Set(input)),
        },
        QueueManagerServiceResponse::QueueState(Arc::new(expected.clone())),
    )
    .await
    else {
        panic!("unexpected response")
    };
    assert!(response.error.is_none());
    let WsControlRp::QueueState(actual) = response.rp.unwrap() else {
        panic!("unexpected inner response")
    };
    let actual = Arc::into_inner(actual).unwrap();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_ws_handler_queue_set_skip_one() {
    let track_ids = vec!["track-001".into(), "track-002".into(), "track-003".into()];
    let input = QueueStateInputRq {
        meta: track_ids.clone(),
        currently_playing_position: Some(0),
    };
    let expected_meta = vec![
        MetaData {
            id: "track-001".into(),
            title: "Bohemian Rhapsody".into(),
            artist: "Queen".into(),
            duration: 354,
            elapsed: 0,
            mime: "audio/mpeg".into(),
        },
        MetaData {
            id: "track-002".into(),
            title: "Stairway to Heaven".into(),
            artist: "Led Zeppelin".into(),
            duration: 482,
            elapsed: 0,
            mime: "audio/mpeg".into(),
        },
        MetaData {
            id: "track-003".into(),
            title: "Hotel California".into(),
            artist: "Eagles".into(),
            duration: 391,
            elapsed: 0,
            mime: "audio/mpeg".into(),
        },
    ];
    let expected = QueueState {
        version: 1,
        meta: expected_meta.clone().into_iter().map(Arc::new).collect(),
        currently_playing_position: Some(0),
        next_playing_position: Some(1),
        downloading: vec![],
    };
    let WsResponseInternal::Data(response) = ws_queue_handler_roundtrip(
        QueueManagerServiceCommand::ReplaceQueueState(QueueStateInput {
            meta: expected_meta.clone(),
            currently_playing_position: Some(0),
        }),
        WsRequest {
            rq: WsControlRq::Queue(Queue::Set(input)),
        },
        QueueManagerServiceResponse::QueueState(Arc::new(expected.clone())),
    )
    .await
    else {
        panic!("unexpected response")
    };
    assert!(response.error.is_none());
    let WsControlRp::QueueState(actual) = response.rp.unwrap() else {
        panic!("unexpected inner response")
    };
    let actual = Arc::into_inner(actual).unwrap();
    assert_eq!(actual, expected);

    // send skip one
    let input = QueueStateInputRq {
        meta: track_ids.clone(),
        currently_playing_position: Some(2),
    };

    let expected = QueueState {
        version: 1,
        meta: expected_meta.clone().into_iter().map(Arc::new).collect(),
        currently_playing_position: Some(0),
        next_playing_position: input.currently_playing_position,
        downloading: vec![],
    };
    let WsResponseInternal::Data(response) = ws_queue_handler_roundtrip(
        QueueManagerServiceCommand::ReplaceQueueState(QueueStateInput {
            meta: expected_meta.clone(),
            currently_playing_position: input.currently_playing_position,
        }),
        WsRequest {
            rq: WsControlRq::Queue(Queue::Set(input)),
        },
        QueueManagerServiceResponse::QueueState(Arc::new(expected.clone())),
    )
    .await
    else {
        panic!("unexpected response")
    };

    assert!(response.error.is_none());
    let WsControlRp::QueueState(actual) = response.rp.unwrap() else {
        panic!("unexpected inner response")
    };
    let actual = Arc::into_inner(actual).unwrap();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_ws_handler_queue_set_clear() {
    let track_ids = vec!["track-001".into(), "track-002".into()];
    let input = QueueStateInputRq {
        meta: track_ids.clone(),
        currently_playing_position: Some(0),
    };
    let expected_meta = vec![
        MetaData {
            id: "track-001".into(),
            title: "Bohemian Rhapsody".into(),
            artist: "Queen".into(),
            duration: 354,
            elapsed: 0,
            mime: "audio/mpeg".into(),
        },
        MetaData {
            id: "track-002".into(),
            title: "Stairway to Heaven".into(),
            artist: "Led Zeppelin".into(),
            duration: 482,
            elapsed: 0,
            mime: "audio/mpeg".into(),
        },
    ];
    let expected = QueueState {
        version: 1,
        meta: expected_meta.clone().into_iter().map(Arc::new).collect(),
        currently_playing_position: Some(0),
        next_playing_position: Some(1),
        downloading: vec![],
    };
    let WsResponseInternal::Data(response) = ws_queue_handler_roundtrip(
        QueueManagerServiceCommand::ReplaceQueueState(QueueStateInput {
            meta: expected_meta.clone(),
            currently_playing_position: input.currently_playing_position,
        }),
        WsRequest {
            rq: WsControlRq::Queue(Queue::Set(input)),
        },
        QueueManagerServiceResponse::QueueState(Arc::new(expected.clone())),
    )
    .await
    else {
        panic!("unexpected response")
    };
    assert!(response.error.is_none());
    let WsControlRp::QueueState(actual) = response.rp.unwrap() else {
        panic!("unexpected inner response")
    };
    let actual = Arc::into_inner(actual).unwrap();
    assert_eq!(actual, expected);

    // clear the queue
    let input = QueueStateInputRq {
        meta: vec![],
        currently_playing_position: None,
    };

    let expected = QueueState {
        version: 2,
        meta: vec![],
        currently_playing_position: Some(0),
        next_playing_position: Some(1),
        downloading: vec![],
    };
    let WsResponseInternal::Data(response) = ws_queue_handler_roundtrip(
        QueueManagerServiceCommand::ReplaceQueueState(QueueStateInput {
            meta: vec![],
            currently_playing_position: input.currently_playing_position,
        }),
        WsRequest {
            rq: WsControlRq::Queue(Queue::Set(input)),
        },
        QueueManagerServiceResponse::QueueState(Arc::new(expected.clone())),
    )
    .await
    else {
        panic!("unexpected response")
    };

    assert!(response.error.is_none());
    let WsControlRp::QueueState(actual) = response.rp.unwrap() else {
        panic!("unexpected inner response")
    };
    let actual = Arc::into_inner(actual).unwrap();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_ws_handler_search() {
    let WsResponseInternal::Data(response) = ws_source_handler_roundtrip(WsRequest {
        rq: WsControlRq::Search("Bohemian".into()),
    })
    .await
    else {
        panic!("unexpected response")
    };
    assert!(response.error.is_none());
    let WsControlRp::SearchResults(results) = response.rp.unwrap() else {
        panic!("unexpected inner response")
    };
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Bohemian Rhapsody");
    assert_eq!(results[0].artist, "Queen");
}

#[tokio::test]
async fn test_ws_handler_search_empty_query() {
    let WsResponseInternal::Data(response) = ws_source_handler_roundtrip(WsRequest {
        rq: WsControlRq::Search("".into()),
    })
    .await
    else {
        panic!("unexpected response")
    };
    assert!(response.error.is_none());
    let WsControlRp::SearchResults(results) = response.rp.unwrap() else {
        panic!("unexpected inner response")
    };
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_ws_handler_search_no_results() {
    let WsResponseInternal::Data(response) = ws_source_handler_roundtrip(WsRequest {
        rq: WsControlRq::Search("nonexistent song xyz".into()),
    })
    .await
    else {
        panic!("unexpected response")
    };
    assert!(response.error.is_none());
    let WsControlRp::SearchResults(results) = response.rp.unwrap() else {
        panic!("unexpected inner response")
    };
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_ws_handler_search_by_artist() {
    let WsResponseInternal::Data(response) = ws_source_handler_roundtrip(WsRequest {
        rq: WsControlRq::Search("Nirvana".into()),
    })
    .await
    else {
        panic!("unexpected response")
    };
    assert!(response.error.is_none());
    let WsControlRp::SearchResults(results) = response.rp.unwrap() else {
        panic!("unexpected inner response")
    };
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].artist, "Nirvana");
    assert_eq!(results[0].title, "Smells Like Teen Spirit");
}

#[tokio::test]
async fn test_ws_handler_playlists() {
    let WsResponseInternal::Data(response) = ws_source_handler_roundtrip(WsRequest {
        rq: WsControlRq::Playlists,
    })
    .await
    else {
        panic!("unexpected response")
    };
    assert!(response.error.is_none());
    let WsControlRp::Playlists(playlists) = response.rp.unwrap() else {
        panic!("unexpected inner response")
    };
    assert_eq!(playlists.len(), 2);
    assert_eq!(playlists[0].name, "Classic Rock");
    assert_eq!(playlists[0].tracks.len(), 3);
    assert_eq!(playlists[1].name, "90s Alternative");
    assert_eq!(playlists[1].tracks.len(), 2);
}

#[tokio::test]
async fn test_ws_handler_unknown() {
    let WsResponseInternal::Data(response) = ws_source_handler_roundtrip(WsRequest {
        rq: WsControlRq::Unknown,
    })
    .await
    else {
        panic!("unexpected response")
    };
    assert!(response.rp.is_none());
    assert!(response.error.is_some());
    assert_eq!(response.error.unwrap(), "Unknown request");
}
