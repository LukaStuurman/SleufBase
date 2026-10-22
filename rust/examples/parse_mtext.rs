//! Bounded/atomic driver for the native MTEXT inline parser, not its renderer.
use serde::{Deserialize, Serialize};
use sleufbase_rust::mtext::{self, Content, Context, Error, Token};
use std::{
    collections::BTreeMap,
    sync::{atomic::AtomicBool, Arc},
};
#[path = "support/atomic_json.rs"]
mod atomic_json;

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
fn main() -> Result<(), String> {
    atomic_json::run(2048, |r: Request, out| {
        match mtext::parse(
            &r.content,
            r.initial.unwrap_or_default(),
            r.properties,
            &AtomicBool::new(false),
        ) {
            Ok(tokens) => serde_json::to_writer(out, &Reply::Ok(document(&tokens))),
            Err(error) => {
                let message = error.to_string();
                let error = match error {
                    Error::Cancelled => Failure::Cancelled(message),
                    Error::Limit(_) => Failure::Limit(message),
                    Error::Invalid(_) => Failure::Invalid(message),
                };
                serde_json::to_writer(out, &Reply::Err { error })
            }
        }
        .map_err(|e| e.to_string())
    })
}
