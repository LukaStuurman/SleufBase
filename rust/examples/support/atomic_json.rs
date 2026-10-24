//! Shared bounded/atomic transport for the native MTEXT audit examples.
use serde::de::DeserializeOwned;
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

struct Bounded<W: Write> {
    out: W,
    written: usize,
}
impl<W: Write> Write for Bounded<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.written = self
            .written
            .checked_add(buf.len())
            .filter(|&n| n <= 64 * 1024 * 1024)
            .ok_or_else(|| io::Error::other("MTEXT audit output exceeds 64 MiB"))?;
        self.out.write_all(buf)?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}
pub fn run<T: DeserializeOwned>(
    max_cases: usize,
    mut write: impl FnMut(T, &mut dyn Write) -> Result<(), String>,
) -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() != 2 {
        return Err("Input.json and a new output.json required".into());
    }
    if fs::metadata(&args[0]).map_err(|e| e.to_string())?.len() > 32 * 1024 * 1024 {
        return Err("MTEXT audit request exceeds 32 MiB".into());
    }
    // Decode through Value, as the native project reader does. With global
    // arbitrary_precision, serde's internally tagged Content can otherwise
    // misinterpret a valid decimal token as the private Number map.
    let requests: Vec<serde_json::Value> =
        serde_json::from_slice(&fs::read(&args[0]).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if requests.len() > max_cases {
        return Err("Too many MTEXT audit cases".into());
    }
    let parent = args[1]
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    {
        let mut out = Bounded {
            out: io::BufWriter::new(temp.as_file_mut()),
            written: 0,
        };
        out.write_all(b"[").map_err(|e| e.to_string())?;
        for (i, r) in requests.into_iter().enumerate() {
            if i > 0 {
                out.write_all(b",").map_err(|e| e.to_string())?;
            }
            let r: T = serde_json::from_value(r).map_err(|e| e.to_string())?;
            write(r, &mut out)?;
        }
        out.write_all(b"]").map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
    }
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist_noclobber(&args[1])
        .map_err(|e| e.to_string())?;
    Ok(())
}
