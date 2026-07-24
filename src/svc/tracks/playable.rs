use std::{
    fmt,
    fs::File,
    io::{
        self,
        BufReader,
        SeekFrom,
    },
    path::{
        Path,
        PathBuf,
    },
    sync::Arc,
};

use sunk::{
    Streamable,
    song::Song,
};

use crate::svc::tracks::{
    MUSIC_FORMAT_EXT,
    Playable,
    PlayableAudioSourceType,
};

use super::playback_strategy::PlaybackStrategyAdapter;

const MAX_BITRATE: usize = 320;

/// Implementation of a playable track. Downloads the song while streaming into the cache directory.
#[derive(Debug)]
pub struct CachingStreamingSubsonicMusicSource {
    id: String,
    cached_file_name: PathBuf,
    transcode_target_fmt: String,
    client: Arc<sunk::Client>,
}

impl Playable for CachingStreamingSubsonicMusicSource {
    type Output = CachingStreamReader<BufReader<File>>;

    fn data(&self) -> io::Result<CachingStreamReader<BufReader<File>>> {
        let cache_path = self
            .cache_file_name()
            .to_str()
            .expect("invalid file cache")
            .to_string();
        let f = BufReader::new(File::options().read(true).open(cache_path)?);
        let r = CachingStreamReader::new(f);
        Ok(r)
    }

    /// Opens and sets the stream according to the provided strategy
    #[tracing::instrument(skip(self), fields(id=self.id))]
    fn init<L: PlaybackStrategyAdapter>(&mut self, mut loading_strategy: L) -> io::Result<()> {
        if loading_strategy.should_apply()? {
            let id = self.id.clone();
            let format = self.transcode_target_fmt.clone();
            // TODO: remove expect and do something useful instead
            let mut song = Song::get(&self.client, id).expect("Song not found by id");
            song.set_max_bit_rate(MAX_BITRATE);
            song.set_transcoding(&format);
            let stream = song.stream(&self.client).map_err(map_error)?;
            // ignore the returned reader
            let _ = loading_strategy.apply(stream)?;
        }
        Ok(())
    }
}

impl CachingStreamingSubsonicMusicSource {
    pub fn new(id: String, client: Arc<sunk::Client>, mut cached_file_name: PathBuf) -> Self {
        let transcode_target_fmt = MUSIC_FORMAT_EXT.into();
        cached_file_name.push(&id);
        cached_file_name.set_extension(&transcode_target_fmt);

        Self {
            id,
            transcode_target_fmt,
            cached_file_name,
            client,
        }
    }

    pub fn cache_file_name(&self) -> &Path {
        &self.cached_file_name
    }
}

/// A stream reader that writes the contents to a file cache while reading.
/// if a file cache is not provided, the stream just reads.
/// Sync only.
pub struct CachingStreamReader<R: io::Read + io::Seek + Send + Sync> {
    reader: R,
}

impl<R> fmt::Debug for CachingStreamReader<R>
where
    R: io::Read + io::Seek + Send + Sync,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CachingStreamReader")
            .field("reader", &"...")
            .finish()
    }
}

impl<R> CachingStreamReader<R>
where
    R: io::Read + io::Seek + Send + Sync + 'static,
{
    pub fn new(reader: R) -> Self {
        Self { reader }
    }
}

impl<R> PlayableAudioSourceType for CachingStreamReader<R> where
    R: io::Read + io::Seek + Send + Sync + 'static
{
}

impl<R> io::Seek for CachingStreamReader<R>
where
    R: io::Read + io::Seek + Send + Sync,
{
    fn seek(&mut self, s: SeekFrom) -> std::io::Result<u64> {
        self.reader.seek(s)
    }
}
impl<R> io::Read for CachingStreamReader<R>
where
    R: io::Read + io::Seek + Send + Sync,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf)
    }
}

#[tracing::instrument(level = "warn")]
fn map_error(e: sunk::Error) -> io::Error {
    match e {
        sunk::Error::Io(error) => error,
        sunk::Error::Connection(inner) => {
            io::Error::other(format!("Connection error: status code {}", inner))
        }
        sunk::Error::Url(inner) => io::Error::other(format!("URL error: {}", inner)),
        sunk::Error::Api(inner) => io::Error::other(format!("API error: {}", inner)),
        sunk::Error::Parse(inner) => io::Error::other(inner),
        sunk::Error::Reqwest(inner) => io::Error::other(inner),
        sunk::Error::Serde(inner) => io::Error::other(inner),
        sunk::Error::Other(inner) => io::Error::other(inner),
    }
}

#[cfg(test)]
mod test {
    use super::*;
}
