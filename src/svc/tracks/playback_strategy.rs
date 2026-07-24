use std::{
    fmt,
    fs::{
        self,
        File,
    },
    io::{
        self,
        BufReader,
        BufWriter,
        Read,
        Write,
    },
    path::PathBuf,
    sync::atomic::{
        AtomicBool,
        Ordering::Relaxed,
    },
};

use tracing::{
    debug,
    error,
};

pub trait PlaybackStrategyAdapter: fmt::Debug {
    type Output: io::Read + Send;
    fn apply(self, input: Box<dyn io::Read + Send>) -> io::Result<Self::Output>;
    fn should_apply(&mut self) -> io::Result<bool> {
        Ok(true)
    }
}

#[derive(Debug, Default)]
pub struct StreamOnly {}

impl StreamOnly {
    pub fn new() -> Self {
        Self {}
    }
}

impl PlaybackStrategyAdapter for StreamOnly {
    type Output = Box<dyn io::Read + Send>;
    fn apply(self, input: Box<dyn io::Read + Send>) -> io::Result<Self::Output> {
        // Do nothing with the input stream
        Ok(input)
    }
}

#[derive(Debug)]
pub struct DownloadWhileStreaming {
    download_to: PathBuf,
    min_cache_bytes: usize,
    completed: AtomicBool,
}

impl DownloadWhileStreaming {
    pub fn new(download_to: PathBuf, min_cache_bytes: usize) -> Self {
        Self {
            download_to,
            min_cache_bytes,
            completed: AtomicBool::new(false),
        }
    }
}

impl PlaybackStrategyAdapter for DownloadWhileStreaming {
    type Output = BufReader<File>;

    #[tracing::instrument(skip_all)]
    fn apply(self, input: Box<dyn io::Read + Send>) -> io::Result<Self::Output> {
        // CachingStreamReader::new(Mutex::new(stream), Some(PathBuf::from(cache_path)))

        let download_to = self.download_to.clone();
        let mut cache_writer = BufWriter::new(
            File::options()
                .create(true)
                .append(true)
                .open(&download_to)?,
        );

        // downloader task
        let mut reader = BufReader::new(input);
        let mut buf = vec![0_u8];
        let mut bytes_read = 0;
        debug!(dest=?download_to, "downloading song");
        loop {
            let read = reader.read(&mut buf).unwrap();
            bytes_read += read;
            if read > 0 {
                let _ = cache_writer.write(&buf)?;
            }
            if bytes_read > self.min_cache_bytes {
                break;
            }
        }
        cache_writer.flush()?;

        // continue downloading in the background
        let dest = download_to.clone();
        tokio::task::spawn_blocking(move || match io::copy(&mut reader, &mut cache_writer) {
            Ok(s) => {
                let total_read = bytes_read as u64 + s;
                debug!(total_read=total_read, dest=?dest, "download done");

                self.completed.store(true, Relaxed)
            }
            Err(e) => error!(err=?e, "download failed"),
        });
        debug!(dest=?download_to, "done downloading initial parts.");
        File::options()
            .read(true)
            .open(&download_to)
            .map(BufReader::new)
    }

    fn should_apply(&mut self) -> io::Result<bool> {
        let cache_hit = self.download_to.exists() || self.completed.load(Relaxed);
        // no other way to check for transcoded streamed content...
        self.completed.store(cache_hit, Relaxed);
        Ok(!cache_hit)
    }
}

impl Drop for DownloadWhileStreaming {
    fn drop(&mut self) {
        debug!("dropping cache");
        if !self.completed.load(Relaxed) {
            let _ = fs::remove_file(self.download_to.clone());
            debug!("deleted cached file because it's incomplete: {:?}", self);
        }
    }
}
