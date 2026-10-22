//! Bounded/atomic driver for the native MTEXT inline parser, not its renderer.
use serde::{Deserialize, Serialize};
use sleufbase_rust::mtext::{self, Content, Context, Error, Token};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    content: String,
    #[serde(default)]
    initial: Option<Context>,
    #[serde(default)]
    properties: bool,
}
#[derive(Serialize)]
struct Entry<'a> {
    content: &'a Content,
    context: usize,
}
#[derive(Serialize)]
struct Document<'a> {
    contexts: Vec<Arc<Context>>,
    tokens: Vec<Entry<'a>>,
}
#[derive(Serialize)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
enum Failure {
    Cancelled(String),
    Limit(String),
    Invalid(String),
}
#[derive(Serialize)]
#[serde(untagged)]
enum Reply<'a> {
    Ok(Document<'a>),
    Err { error: Failure },
}
fn document(tokens: &[Token]) -> Document<'_> {
    let mut contexts = Vec::new();
    let mut ids = BTreeMap::new();
    let mut entries = Vec::with_capacity(tokens.len());
    for token in tokens {
        let key = Arc::as_ptr(&token.context) as usize;
        let id = *ids.entry(key).or_insert_with(|| {
            let id = contexts.len();
            contexts.push(token.context.clone());
            id
        });
        entries.push(Entry {
            content: &token.content,
            context: id,
        });
    }
    Document {
        contexts,
        tokens: entries,
    }
}
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
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() != 2 {
        return Err("Input.json and a new output.json required".into());
    }
    if fs::metadata(&args[0]).map_err(|e| e.to_string())?.len() > 32 * 1024 * 1024 {
        return Err("MTEXT audit request exceeds 32 MiB".into());
    }
    let requests: Vec<Request> =
        serde_json::from_slice(&fs::read(&args[0]).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if requests.len() > 2048 {
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
            match mtext::parse(
                &r.content,
                r.initial.unwrap_or_default(),
                r.properties,
                &AtomicBool::new(false),
            ) {
                Ok(tokens) => serde_json::to_writer(&mut out, &Reply::Ok(document(&tokens))),
                Err(error) => {
                    let message = error.to_string();
                    let failure = match error {
                        Error::Cancelled => Failure::Cancelled(message),
                        Error::Limit(_) => Failure::Limit(message),
                        Error::Invalid(_) => Failure::Invalid(message),
                    };
                    serde_json::to_writer(&mut out, &Reply::Err { error: failure })
                }
            }
            .map_err(|e| e.to_string())?;
        }
        out.write_all(b"]").map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
    }
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist_noclobber(&args[1])
        .map_err(|e| e.to_string())?;
    Ok(())
}
