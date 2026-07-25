use std::{
    env,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use crate::{
    config::{
        self,
        CachingStrategy,
    },
    svc::{
        ServiceFactory,
        error::SvcError,
        tracks::{
            MetaData,
            Playable,
            PlayableSourceProvider,
            Playlist,
            TrackSourceError,
            TracklistSourceProvider,
            playable::CachingStreamingSubsonicMusicSource,
            playback_strategy::{
                DownloadWhileStreaming,
                StreamOnly,
            },
        },
    },
};
use reqwest::blocking::Client as ReqwestClient;
use sunk::{
    collections::playlist::get_playlists,
    search,
    song::Song,
};

/// Audio source wrapping a [`sunk::Client`].
#[derive(Debug, Clone)]
pub struct SubsonicMusicSource {
    pub client: Arc<sunk::Client>,
    pub config: CachingStrategy,
}

impl SubsonicMusicSource {
    pub fn new(config: &config::Subsonic) -> Result<Self, sunk::Error> {
        let reqclient = ReqwestClient::builder()
            .connect_timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        let client = sunk::Client::new(&config.url, &config.username, &config.password)?
            .with_client(reqclient);
        Ok(SubsonicMusicSource {
            client: Arc::new(client),
            config: config.caching_strategy.clone(),
        })
    }
}

impl PlayableSourceProvider for SubsonicMusicSource {
    type PlayableType = CachingStreamingSubsonicMusicSource;

    async fn playable_data(&self, ids: Vec<String>) -> Result<Vec<Self::PlayableType>, SvcError> {
        let client = self.client.clone();
        let mut audio_sources = vec![];
        for id in ids {
            tracing::info!(song_id = id);
            let src = match &self.config {
                CachingStrategy::DiskCache {
                    path,
                    min_cache_before_play,
                } => {
                    let pb = PathBuf::from(path);
                    if !pb.exists() {
                        tokio::fs::create_dir(&pb).await?;
                    }
                    let mut src = CachingStreamingSubsonicMusicSource::new(id, client.clone(), pb);
                    let cache_file_name = src.cache_file_name().to_path_buf();
                    tokio::task::block_in_place(|| {
                        src.init(DownloadWhileStreaming::new(
                            cache_file_name,
                            *min_cache_before_play,
                        ))
                    })?;
                    src
                }
                CachingStrategy::StreamOnly => {
                    let mut src = CachingStreamingSubsonicMusicSource::new(
                        id,
                        client.clone(),
                        env::temp_dir(),
                    );
                    tokio::task::block_in_place(|| src.init(StreamOnly {}))?;
                    src
                }
            };
            audio_sources.push(src);
        }

        Ok(audio_sources)
    }
}

impl TracklistSourceProvider for SubsonicMusicSource {
    #[tracing::instrument(level = "info")]
    async fn init(&mut self) -> Result<(), SvcError> {
        let client = self.client.clone();
        tokio::task::block_in_place(move || -> Result<(), SvcError> {
            client.ping().and(client.scan_library()).map_err(|e| {
                tracing::error!(err=?e, "Scanning library failed");
                SvcError::Source(TrackSourceError::SubsonicSourceError(e))
            })?;
            Ok(())
        })
    }

    #[tracing::instrument(level = "debug")]
    async fn random(&self, n: usize) -> Result<Vec<MetaData>, SvcError> {
        let client = self.client.clone();
        tokio::task::block_in_place(move || -> Result<Vec<MetaData>, SvcError> {
            let random = sunk::song::Song::random(&client, n)
                .map_err(|e| SvcError::Source(TrackSourceError::SubsonicSourceError(e)))?;
            Ok(random.iter().map(MetaData::from).collect())
        })
    }

    #[tracing::instrument(level = "debug")]
    async fn search(&self, query: String) -> Result<Vec<MetaData>, SvcError> {
        let client = self.client.clone();
        tokio::task::block_in_place(move || -> Result<Vec<MetaData>, SvcError> {
            let results = client
                .search(&query, search::NONE, search::NONE, search::ALL)
                .map_err(|e| SvcError::Source(TrackSourceError::SubsonicSourceError(e)))?;
            Ok(results.songs.iter().map(MetaData::from).collect())
        })
    }

    #[tracing::instrument(level = "debug")]
    async fn playlists(&self) -> Result<Vec<Playlist>, SvcError> {
        let client = self.client.clone();
        tokio::task::block_in_place(move || -> Result<Vec<Playlist>, SvcError> {
            let playlists = get_playlists(&client, None)
                .map_err(|e| SvcError::Source(TrackSourceError::SubsonicSourceError(e)))?;
            Ok(playlists.iter().map(From::from).collect())
        })
    }

    #[tracing::instrument(level = "debug")]
    async fn lookup(&self, ids: Vec<String>) -> Vec<Result<MetaData, SvcError>> {
        let handles: Vec<_> = ids
            .into_iter()
            .map(Arc::new)
            .map(|id| {
                let client = self.client.clone();
                tokio::task::spawn_blocking(move || {
                    Song::get(&client, id.as_str())
                        .map(|s| MetaData::from(&s))
                        .map_err(TrackSourceError::from)
                })
            })
            .collect();

        let mut results: Vec<Result<MetaData, SvcError>> = vec![];
        for h in handles {
            results.push(match h.await {
                Ok(ref d) if let Ok(m) = d => Ok(m.clone()),
                Err(e) => Err(SvcError::from(e)),
                Ok(_) => Err(SvcError::Internal()),
            });
        }
        results
    }
}

impl From<&sunk::Playlist> for Playlist {
    fn from(s: &sunk::Playlist) -> Self {
        let value = s;
        Playlist {
            name: value.name.clone(),
            duration: value.duration,
            id: value.id.to_string(),
            tracks: value.songs.iter().map(From::from).collect(),
        }
    }
}

impl From<&Song> for MetaData {
    fn from(s: &Song) -> Self {
        let value = &s;
        MetaData {
            title: value.title.clone(),
            artist: value.artist.clone().unwrap_or("unknown".to_string()),
            duration: value.duration.unwrap_or_default(),
            id: value.id.to_string(),
            mime: value.content_type.to_string(),
            elapsed: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SubsonicMusicSourceFactory {
    config: config::Subsonic,
}

impl SubsonicMusicSourceFactory {
    pub fn new(config: config::Subsonic) -> Self {
        Self { config }
    }
}

impl ServiceFactory for SubsonicMusicSourceFactory {
    type Service = SubsonicMusicSource;
    async fn get_instance(&self) -> Self::Service {
        tokio::task::block_in_place(|| SubsonicMusicSource::new(&self.config).unwrap())
    }
}
