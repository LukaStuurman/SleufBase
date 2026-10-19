//! Bounded, cancellable file reads shared by local importers.
use std::{
    fs::File,
    io::Read,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub(crate) static NEVER_CANCEL: AtomicBool = AtomicBool::new(false);
pub(crate) fn check(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Laden geannuleerd".into())
    } else {
        Ok(())
    }
}
pub(crate) fn read(
    path: &Path,
    limit: u64,
    label: &str,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    check(cancel)?;
    let mut file = File::open(path).map_err(|e| format!("{label} niet bereikbaar: {e}"))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > limit {
        return Err(format!("{label} is te groot"));
    }
    read_chunks(&mut file, limit, label, cancel)
}
pub(crate) fn read_chunks(
    reader: &mut impl Read,
    limit: u64,
    label: &str,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        check(cancel)?;
        // Read one byte beyond the limit to detect a growing file as well.
        let allowed = (limit.saturating_add(1).saturating_sub(output.len() as u64))
            .min(chunk.len() as u64) as usize;
        if allowed == 0 {
            return Err(format!("{label} is te groot"));
        }
        let count = match reader.read(&mut chunk[..allowed]) {
            Ok(count) => count,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("{label} lezen mislukt: {e}")),
        };
        if count == 0 {
            break;
        }
        check(cancel)?;
        if (output.len() as u64).saturating_add(count as u64) > limit {
            return Err(format!("{label} is te groot"));
        }
        output
            .try_reserve(count)
            .map_err(|_| format!("Onvoldoende geheugen voor {label}"))?;
        output.extend_from_slice(&chunk[..count]);
    }
    check(cancel)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_limit_bounds_the_actual_read_even_when_a_stream_keeps_growing() {
        struct Endless {
            read: usize,
        }
        impl Read for Endless {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                buf.fill(b'x');
                self.read += buf.len();
                Ok(buf.len())
            }
        }
        let mut source = Endless { read: 0 };
        assert!(read_chunks(&mut source, 7, "JSON", &NEVER_CANCEL)
            .unwrap_err()
            .contains("te groot"));
        assert_eq!(source.read, 8, "do not copy the whole growing source");
        assert_eq!(
            read_chunks(&mut &b"1234567"[..], 7, "JSON", &NEVER_CANCEL).unwrap(),
            b"1234567"
        );
        assert!(read_chunks(&mut &b""[..], 0, "JSON", &NEVER_CANCEL)
            .unwrap()
            .is_empty());
    }
    #[test]
    fn cancelled_reads_never_open_a_path_or_consume_another_chunk() {
        let cancel = AtomicBool::new(true);
        assert!(read(Path::new("missing-file"), 10, "JSON", &cancel)
            .unwrap_err()
            .contains("geannuleerd"));
        struct CancelAfterChunk<'a>(&'a AtomicBool);
        impl Read for CancelAfterChunk<'_> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                self.0.store(true, Ordering::Relaxed);
                buf[0] = b'x';
                Ok(1)
            }
        }
        cancel.store(false, Ordering::Relaxed);
        assert!(
            read_chunks(&mut CancelAfterChunk(&cancel), 10, "JSON", &cancel)
                .unwrap_err()
                .contains("geannuleerd")
        );
    }
    #[test]
    fn interrupted_operating_system_reads_are_retried() {
        struct Interrupted(u8);
        impl Read for Interrupted {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                match self.0 {
                    0 => {
                        self.0 = 1;
                        Err(std::io::ErrorKind::Interrupted.into())
                    }
                    1 => {
                        self.0 = 2;
                        buf[0] = b'x';
                        Ok(1)
                    }
                    _ => Ok(0),
                }
            }
        }
        assert_eq!(
            read_chunks(&mut Interrupted(0), 10, "JSON", &NEVER_CANCEL).unwrap(),
            b"x"
        );
    }
}
