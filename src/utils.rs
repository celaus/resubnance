use std::io::{
    BufRead,
    Cursor,
    Read,
    Seek,
    SeekFrom,
};

// Unused, maybe for later
pub struct BufferedSeek<R: BufRead + Sync + Send + 'static> {
    data: Vec<u8>,
    reader: R,
}

impl<R> BufferedSeek<R>
where
    R: BufRead + Sync + Send + 'static,
{
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            data: vec![],
        }
    }
}

impl<R> Read for BufferedSeek<R>
where
    R: BufRead + Sync + Send + 'static,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let res = self.reader.read(buf);
        if res.is_ok() {
            self.data.extend_from_slice(buf);
        }
        res
    }
}

impl<R> Seek for BufferedSeek<R>
where
    R: BufRead + Sync + Send + 'static,
{
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let data = &self.data;
        Cursor::new(data).seek(pos)
    }
}
