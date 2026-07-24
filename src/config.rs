use std::path::{
    Path,
    PathBuf,
};

use figment::{
    Figment,
    providers::{
        Env,
        Format,
        Toml,
    },
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::svc::error::SvcError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResubnanceConfig {
    pub subsonic: Subsonic,
    pub server: Server,
    pub audio: AudioSink,
    #[serde(default)]
    pub additional: Vec<ExternalServices>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subsonic {
    pub url: String,
    pub username: String,
    pub password: String,
    pub caching_strategy: CachingStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CachingStrategy {
    DiskCache {
        path: String,
        min_cache_before_play: usize,
    },
    StreamOnly,
}

impl CachingStrategy {
    pub fn cache_dir(&self) -> Option<PathBuf> {
        match &self {
            CachingStrategy::DiskCache { path, .. } => Some(PathBuf::from(path)),
            _ => None, // inconsequential
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub url: String,
    pub external_url: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSink {
    pub buffer_size: u32, // min 512, better 1024 or 2048, 4096
    pub sampling_rate: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "name")]
pub enum ExternalServices {
    CoverImageUploader { url: url::Url },
    Other,
}

pub fn parse(path: &Path) -> Result<ResubnanceConfig, SvcError> {
    Figment::new()
        .merge(Toml::file(path))
        .merge(Env::prefixed("RESUB_"))
        .extract()
        .map_err(|e| SvcError::Config(Box::new(e)))
}
