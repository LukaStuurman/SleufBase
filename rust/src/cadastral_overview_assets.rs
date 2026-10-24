//! Bounded, cancellable overview assets within the existing export transaction.
use crate::{
    export_control::{CheckedWriter, Control},
    input,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};
pub const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub struct Budget {
    used: u64,
    limit: u64,
}
impl Budget {
    pub fn new() -> Self {
        Self {
            used: 0,
            limit: MAX_BYTES,
        }
    }
    pub fn remaining(&self) -> u64 {
        self.limit - self.used
    }
    pub fn consume(&mut self, bytes: u64) -> Result<(), String> {
        if bytes > self.remaining() {
            return Err("Overzichtsrasters overschrijden het exportbudget".into());
        }
        self.used += bytes;
        Ok(())
    }
    pub fn copy(
        &mut self,
        source: &Path,
        destination: &Path,
        control: &Control,
    ) -> Result<(), String> {
        control.check()?;
        let mut source = File::open(source).map_err(|e| format!("TIFF niet bereikbaar: {e}"))?;
        let expected = source.metadata().map_err(|e| e.to_string())?.len();
        if expected == 0 || expected > self.remaining() {
            return Err("TIFF is leeg of overschrijdt het resterende exportbudget".into());
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(|e| e.to_string())?;
        self.copy_stream(&mut source, &mut output, control)
    }
    fn copy_stream(
        &mut self,
        source: &mut impl Read,
        output: &mut impl Write,
        control: &Control,
    ) -> Result<(), String> {
        let mut buffer = [0u8; 64 * 1024];
        loop {
            control.check()?;
            // One extra byte detects a source that grows after metadata was
            // read, without consuming an unbounded stream or writing over quota.
            let allowed = (self.remaining().saturating_add(1)).min(buffer.len() as u64) as usize;
            let n = match source.read(&mut buffer[..allowed]) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                result => result.map_err(|e| e.to_string())?,
            };
            control.check()?;
            if n == 0 {
                break;
            }
            self.consume(n as u64)?;
            output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        }
        control.check()
    }
    pub fn virtual_tiff(
        &mut self,
        image: &image::RgbaImage,
        destination: &Path,
        control: &Control,
    ) -> Result<(), String> {
        control.check()?;
        let minimum = u64::from(image.width()) * u64::from(image.height()) * 4;
        if minimum > self.remaining() {
            return Err("Virtuele TIFF overschrijdt het resterende exportbudget".into());
        }
        let file = OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .open(destination)
            .map_err(|e| e.to_string())?;
        let writer = LimitedFile {
            inner: CheckedWriter { out: file, control },
            limit: self.remaining(),
        };
        let mut encoder = tiff::encoder::TiffEncoder::new(writer).map_err(|e| e.to_string())?;
        encoder
            .write_image::<tiff::encoder::colortype::RGBA8>(
                image.width(),
                image.height(),
                image.as_raw(),
            )
            .map_err(|e| e.to_string())?;
        drop(encoder);
        control.check()?;
        self.consume(fs::metadata(destination).map_err(|e| e.to_string())?.len())
    }
}
struct LimitedFile<'a> {
    inner: CheckedWriter<'a, File>,
    limit: u64,
}
impl Write for LimitedFile<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        input::check(self.inner.control.flag()).map_err(io::Error::other)?;
        if self
            .inner
            .stream_position()?
            .saturating_add(bytes.len() as u64)
            > self.limit
        {
            return Err(io::Error::other("TIFF overschrijdt het exportbudget"));
        }
        self.inner.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
impl Seek for LimitedFile<'_> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        input::check(self.inner.control.flag()).map_err(io::Error::other)?;
        let position = self.inner.seek(position)?;
        if position > self.limit {
            return Err(io::Error::other("TIFF overschrijdt het exportbudget"));
        }
        Ok(position)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn growing_stream_stops_at_remaining_quota_without_overwriting_output() {
        struct Growing(usize);
        impl Read for Growing {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                self.0 += buf.len();
                buf.fill(7);
                Ok(buf.len())
            }
        }
        let mut source = Growing(0);
        let mut output = Vec::new();
        let mut budget = Budget { used: 0, limit: 7 };
        assert!(budget
            .copy_stream(&mut source, &mut output, &Control::default())
            .is_err());
        assert_eq!(source.0, 8);
        assert!(output.is_empty());
    }
    #[test]
    fn cancellation_after_a_read_cannot_write_the_chunk() {
        struct Cancels<'a>(&'a Control);
        impl Read for Cancels<'_> {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                assert!(self.0.cancel());
                buf[0] = 1;
                Ok(1)
            }
        }
        let control = Control::default();
        let mut output = Vec::new();
        assert!(Budget::new()
            .copy_stream(&mut Cancels(&control), &mut output, &control)
            .is_err());
        assert!(output.is_empty());
    }
    #[test]
    fn metadata_quota_rejection_happens_before_creating_any_target() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.tif");
        fs::write(&source, [1; 8]).unwrap();
        let dest = dir.path().join("target.tif");
        assert!(Budget { used: 0, limit: 7 }
            .copy(&source, &dest, &Control::default())
            .is_err());
        assert!(!dest.exists());
    }
}
