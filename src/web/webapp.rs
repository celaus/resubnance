use std::collections::HashMap;
use std::error::Error;
use std::{
    collections::BTreeMap,
    env,
    fs,
    io,
    path::PathBuf,
};

use poem::Endpoint;
use poem::http::StatusCode;
use poem::{
    error::InternalServerError,
    handler,
    web::{
        Data,
        Html,
    },
};
use serde::{
    Deserialize,
    Serialize,
};
use tera::{
    Context,
    Tera,
};

use lazy_static::lazy_static;

use crate::{
    config::{
        self,
        ResubnanceConfig,
    },
    svc::{
        ServiceFactory,
        tracks::{
            MUSIC_FORMAT_EXT,
            TracklistSourceProvider,
            subsonic::SubsonicMusicSourceFactory,
        },
    },
};

lazy_static! {
    pub static ref TEMPLATES: Tera = {
        tracing::debug!("Reading templates...");
        let mut tera = match Tera::new("templates/*") {
            Ok(mut t) => {
                t.register_filter("with_leading_zeros", with_leading_zeros);
                t
            },
            Err(e) => {
                tracing::error!("Parsing error(s): {}", e);
                ::std::process::exit(1);
            }
        };
        // tera.autoescape_on(vec![".html", ".sql"]);
        tera.autoescape_on(vec![]);
        tera
    };
}

#[handler]
#[tracing::instrument(skip(config))]
pub fn webapp(config: Data<&ResubnanceConfig>) -> Result<Html<String>, poem::Error> {
    let mut context = Context::new();
    context.insert("name", &config.server.name);
    context.insert("baseUrl", &config.server.external_url);
    context.insert("version", &format!("v{}", env!("CARGO_PKG_VERSION")));
    TEMPLATES
        .render("index.html", &context)
        .map_err(InternalServerError)
        .map(Html)
}

#[derive(Debug)]
pub struct DeleteIdFromCacheEp {
    cache_dir: PathBuf,
}

impl DeleteIdFromCacheEp {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }
}

impl Endpoint for DeleteIdFromCacheEp {
    type Output = ();

    #[tracing::instrument]
    fn call(&self, req: poem::Request) -> impl Future<Output = poem::Result<Self::Output>> + Send {
        let cache_dir = self.cache_dir.clone();
        return async move {
            let id = req.path_params::<String>()?;
            let all_chached_mp3s =
                tokio::task::spawn_blocking(move || find_all_cached_files(cache_dir))
                    .await
                    .unwrap()
                    .map_err(internal_server_error)?;
            let to_delete = all_chached_mp3s
                .into_iter()
                .filter_map(
                    |m| match m.file_stem().map(|f| f.to_string_lossy().to_string()) {
                        Some(name) if name == id => Some(m),
                        _ => None,
                    },
                )
                .next();
            if let Some(f) = to_delete {
                tokio::fs::remove_file(&f)
                    .await
                    .map_err(internal_server_error)
            } else {
                Err(poem::Error::from_status(StatusCode::NOT_FOUND))
            }
        };
    }
}

#[derive(Debug)]
pub struct EmptyCacheEp {
    cache_dir: PathBuf,
}

impl EmptyCacheEp {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }
}

impl Endpoint for EmptyCacheEp {
    type Output = ();

    #[tracing::instrument]
    fn call(&self, req: poem::Request) -> impl Future<Output = poem::Result<Self::Output>> + Send {
        let cache_dir = self.cache_dir.clone();
        return async move {
            let all_chached_mp3s =
                tokio::task::spawn_blocking(move || find_all_cached_files(cache_dir))
                    .await
                    .unwrap()
                    .map_err(internal_server_error)?;
            for mp3 in all_chached_mp3s {
                if let Err(e) = tokio::fs::remove_file(mp3.as_path()).await {
                    tracing::warn!(
                        mp3 = mp3.to_str().unwrap(),
                        action = "delete",
                        msg = "Cannot delete file"
                    );
                    return Err(internal_server_error(e));
                }
            }
            Ok(())
        };
    }
}

#[handler]
#[tracing::instrument(skip(config))]
pub async fn cache(config: Data<&ResubnanceConfig>) -> Result<Html<String>, poem::Error> {
    let cache_dir = config
        .subsonic
        .caching_strategy
        .cache_dir()
        .unwrap_or(env::temp_dir());

    let cached_items = build_cached_items(cache_dir.clone(), config.subsonic.clone())
        .await
        .map_err(|e| poem::Error::new(e, StatusCode::INTERNAL_SERVER_ERROR))?;

    let mut context = Context::new();
    context.insert("name", &config.server.name);
    context.insert("baseUrl", &config.server.external_url);
    context.insert("version", &format!("v{}", env!("CARGO_PKG_VERSION")));
    context.insert("cache", &cached_items);
    context.insert("cache_dir", &cache_dir.to_string_lossy().to_string());

    TEMPLATES
        .render("cache.html", &context)
        .map_err(internal_server_error)
        .map(Html)
}

fn with_leading_zeros(
    v: &tera::Value,
    _: &HashMap<String, tera::Value>,
) -> tera::Result<tera::Value> {
    match v {
        tera::Value::Array(_)
        | tera::Value::Object(_)
        | tera::Value::Bool(_)
        | tera::Value::Null => Err(tera::Error::msg("Invalid value to call this filter with")),
        tera::Value::String(s) => Ok(tera::Value::String(format!("{:02}", s))),
        tera::Value::Number(n) => Ok(tera::Value::String(format!("{:02}", n.as_f64().unwrap()))),
    }
}

fn find_all_cached_files(cache_dir: PathBuf) -> io::Result<Vec<PathBuf>> {
    let files = fs::read_dir(&cache_dir)?;
    let all_chached_mp3s: Vec<_> = files
        .filter_map(|f| f.ok())
        .map(|f| f.path())
        .filter(|f| {
            f.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == MUSIC_FORMAT_EXT)
                .unwrap_or(false)
        })
        .collect();
    Ok(all_chached_mp3s)
}

#[tracing::instrument]
async fn build_cached_items(
    cache_dir: PathBuf,
    config: config::Subsonic,
) -> Result<BTreeMap<String, CachedItemMetadata>, io::Error> {
    let svc = SubsonicMusicSourceFactory::new(config).get_instance();
    let mut map = BTreeMap::new();
    let all_chached_mp3s = tokio::task::spawn_blocking(move || find_all_cached_files(cache_dir))
        .await
        .unwrap()?;
    let ids: Vec<_> = all_chached_mp3s
        .iter()
        .filter_map(|f| f.file_stem())
        .map(|f| f.to_string_lossy().to_string())
        .collect();

    let data = svc.lookup(ids).await;
    for (f, data) in all_chached_mp3s.into_iter().zip(data) {
        let file_id = f
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let size = f.metadata().unwrap().len();

        let val = match data {
            Ok(d) => CachedItemMetadata {
                id: file_id.clone(),
                title: d.title,
                artist: d.artist,
                duration: d.duration,
                size,
            },
            Err(e) => {
                tracing::warn!(msg="Error fetching metadata for cached item", id=file_id, e=?e);
                CachedItemMetadata {
                    id: file_id.clone(),
                    title: e.to_string(),
                    artist: "".into(),
                    duration: 0,
                    size,
                }
            }
        };
        map.insert(file_id, val);
    }
    Ok(map)
}

#[derive(Serialize, Deserialize)]
struct CachedItemMetadata {
    id: String,
    title: String,
    artist: String,
    duration: u64,
    size: u64,
}

fn internal_server_error<E: Error + Send + Sync + 'static>(e: E) -> poem::Error {
    tracing::info!(err=?e, returning=?StatusCode::INTERNAL_SERVER_ERROR);
    poem::Error::new(e, StatusCode::INTERNAL_SERVER_ERROR)
}
