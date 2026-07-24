use crate::svc::{
    ServiceFactory,
    error::SvcError,
    tracks::{
        MetaData,
        Playable,
        PlayableAudioSourceType,
        PlayableSourceProvider,
        Playlist,
        TracklistSourceProvider,
    },
};

#[derive(Debug)]
pub struct MockAudioSource;

impl PlayableAudioSourceType for MockAudioSource {}

impl std::io::Read for MockAudioSource {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(0)
    }
}

impl std::io::Seek for MockAudioSource {
    fn seek(&mut self, _pos: std::io::SeekFrom) -> std::io::Result<u64> {
        Ok(0)
    }
}

#[derive(Debug, Clone)]
pub struct MockPlayableSource;

impl Playable for MockPlayableSource {
    type Output = MockAudioSource;

    fn data(&self) -> std::io::Result<MockAudioSource> {
        Ok(MockAudioSource)
    }

    fn init<L: super::playback_strategy::PlaybackStrategyAdapter>(
        &mut self,
        loading_strategy: L,
    ) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct MockSubsonicMusicSource {
    tracks: Vec<MetaData>,
    playlists: Vec<Playlist>,
}

impl MockSubsonicMusicSource {
    pub fn new() -> Self {
        Self {
            tracks: Self::default_tracks(),
            playlists: Self::default_playlists(),
        }
    }

    pub fn with_tracks(mut self, tracks: Vec<MetaData>) -> Self {
        self.tracks = tracks;
        self
    }

    pub fn with_playlists(mut self, playlists: Vec<Playlist>) -> Self {
        self.playlists = playlists;
        self
    }

    fn default_tracks() -> Vec<MetaData> {
        vec![
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
            MetaData {
                id: "track-004".into(),
                title: "Imagine".into(),
                artist: "John Lennon".into(),
                duration: 183,
                elapsed: 0,
                mime: "audio/mpeg".into(),
            },
            MetaData {
                id: "track-005".into(),
                title: "Smells Like Teen Spirit".into(),
                artist: "Nirvana".into(),
                duration: 301,
                elapsed: 0,
                mime: "audio/mpeg".into(),
            },
        ]
    }

    fn default_playlists() -> Vec<Playlist> {
        vec![
            Playlist {
                id: "playlist-001".into(),
                name: "Classic Rock".into(),
                duration: 1227,
                tracks: vec![
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
                ],
            },
            Playlist {
                id: "playlist-002".into(),
                name: "90s Alternative".into(),
                duration: 484,
                tracks: vec![
                    MetaData {
                        id: "track-004".into(),
                        title: "Imagine".into(),
                        artist: "John Lennon".into(),
                        duration: 183,
                        elapsed: 0,
                        mime: "audio/mpeg".into(),
                    },
                    MetaData {
                        id: "track-005".into(),
                        title: "Smells Like Teen Spirit".into(),
                        artist: "Nirvana".into(),
                        duration: 301,
                        elapsed: 0,
                        mime: "audio/mpeg".into(),
                    },
                ],
            },
        ]
    }
}

impl Default for MockSubsonicMusicSource {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayableSourceProvider for MockSubsonicMusicSource {
    type PlayableType = MockPlayableSource;

    async fn playable_data(&self, ids: Vec<String>) -> Result<Vec<Self::PlayableType>, SvcError> {
        Ok(ids.into_iter().map(|_| MockPlayableSource).collect())
    }
}

impl TracklistSourceProvider for MockSubsonicMusicSource {
    async fn init(&mut self) -> Result<(), SvcError> {
        Ok(())
    }

    async fn random(&self, n: usize) -> Result<Vec<MetaData>, SvcError> {
        let count = n.min(self.tracks.len());
        Ok(self.tracks.iter().take(count).cloned().collect())
    }

    async fn search(&self, query: String) -> Result<Vec<MetaData>, SvcError> {
        if query.is_empty() {
            return Ok(vec![]);
        }

        let query_lower = query.to_lowercase();
        let results = self
            .tracks
            .iter()
            .filter(|t| {
                t.title.to_lowercase().contains(&query_lower)
                    || t.artist.to_lowercase().contains(&query_lower)
            })
            .cloned()
            .collect();

        Ok(results)
    }

    async fn playlists(&self) -> Result<Vec<Playlist>, SvcError> {
        Ok(self.playlists.clone())
    }

    async fn lookup(&self, ids: Vec<String>) -> Vec<Result<MetaData, SvcError>> {
        ids.into_iter()
            .map(|id| {
                self.tracks
                    .iter()
                    .find(|t| t.id == id)
                    .cloned()
                    .ok_or_else(|| SvcError::Internal())
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct MockSubsonicMusicSourceFactory {
    tracks: Option<Vec<MetaData>>,
    playlists: Option<Vec<Playlist>>,
}

impl MockSubsonicMusicSourceFactory {
    pub fn new() -> Self {
        Self {
            tracks: None,
            playlists: None,
        }
    }

    pub fn with_tracks(mut self, tracks: Vec<MetaData>) -> Self {
        self.tracks = Some(tracks);
        self
    }

    pub fn with_playlists(mut self, playlists: Vec<Playlist>) -> Self {
        self.playlists = Some(playlists);
        self
    }
}

impl Default for MockSubsonicMusicSourceFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceFactory for MockSubsonicMusicSourceFactory {
    type Service = MockSubsonicMusicSource;

    async fn get_instance(&self) -> Self::Service {
        let mut source = MockSubsonicMusicSource::new();

        if let Some(tracks) = &self.tracks {
            source = source.with_tracks(tracks.clone());
        }

        if let Some(playlists) = &self.playlists {
            source = source.with_playlists(playlists.clone());
        }

        source
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_source_init() {
        let mut source = MockSubsonicMusicSource::new();
        assert!(source.init().await.is_ok());
    }

    #[tokio::test]
    async fn test_mock_source_search_by_title() {
        let source = MockSubsonicMusicSource::new();
        let results = source.search("Bohemian".into()).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Bohemian Rhapsody");
    }

    #[tokio::test]
    async fn test_mock_source_search_by_artist() {
        let source = MockSubsonicMusicSource::new();
        let results = source.search("Queen".into()).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].artist, "Queen");
    }

    #[tokio::test]
    async fn test_mock_source_search_empty_query() {
        let source = MockSubsonicMusicSource::new();
        let results = source.search("".into()).await.unwrap();

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_mock_source_search_no_matches() {
        let source = MockSubsonicMusicSource::new();
        let results = source.search("nonexistent song xyz".into()).await.unwrap();

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_mock_source_playlists() {
        let source = MockSubsonicMusicSource::new();
        let playlists = source.playlists().await.unwrap();

        assert_eq!(playlists.len(), 2);
        assert_eq!(playlists[0].name, "Classic Rock");
        assert_eq!(playlists[1].name, "90s Alternative");
    }

    #[tokio::test]
    async fn test_mock_source_lookup_existing() {
        let source = MockSubsonicMusicSource::new();
        let results = source
            .lookup(vec!["track-001".into(), "track-002".into()])
            .await;

        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_ok());
        assert_eq!(results[0].as_ref().unwrap().title, "Bohemian Rhapsody");
        assert_eq!(results[1].as_ref().unwrap().title, "Stairway to Heaven");
    }

    #[tokio::test]
    async fn test_mock_source_lookup_nonexistent() {
        let source = MockSubsonicMusicSource::new();
        let results = source.lookup(vec!["nonexistent".into()]).await;

        assert_eq!(results.len(), 1);
        assert!(results[0].is_err());
    }

    #[tokio::test]
    async fn test_mock_source_random() {
        let source = MockSubsonicMusicSource::new();
        let results = source.random(3).await.unwrap();

        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_mock_source_random_more_than_available() {
        let source = MockSubsonicMusicSource::new();
        let results = source.random(100).await.unwrap();

        assert_eq!(results.len(), 5);
    }

    #[tokio::test]
    async fn test_mock_source_playable_data() {
        let source = MockSubsonicMusicSource::new();
        let results = source
            .playable_data(vec!["track-001".into(), "track-002".into()])
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_mock_factory_default() {
        let factory = MockSubsonicMusicSourceFactory::new();
        let source = factory.get_instance().await;

        assert_eq!(source.tracks.len(), 5);
        assert_eq!(source.playlists.len(), 2);
    }

    #[tokio::test]
    async fn test_mock_factory_with_custom_tracks() {
        let custom_tracks = vec![MetaData {
            id: "custom-1".into(),
            title: "Custom Track".into(),
            artist: "Custom Artist".into(),
            duration: 100,
            elapsed: 0,
            mime: "audio/mpeg".into(),
        }];

        let factory = MockSubsonicMusicSourceFactory::new().with_tracks(custom_tracks);
        let source = factory.get_instance().await;

        assert_eq!(source.tracks.len(), 1);
        assert_eq!(source.tracks[0].title, "Custom Track");
    }

    #[tokio::test]
    async fn test_mock_factory_with_custom_playlists() {
        let custom_playlists = vec![Playlist {
            id: "custom-pl".into(),
            name: "Custom Playlist".into(),
            duration: 200,
            tracks: vec![],
        }];

        let factory = MockSubsonicMusicSourceFactory::new().with_playlists(custom_playlists);
        let source = factory.get_instance().await;

        assert_eq!(source.playlists.len(), 1);
        assert_eq!(source.playlists[0].name, "Custom Playlist");
    }
}
