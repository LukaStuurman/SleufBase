//! Geometry oracle driver for the native measured-cell MTEXT layouter.
use serde::Serialize;
use sleufbase_rust::mtext_layout::{self, Error, Output, Request};
use std::sync::atomic::AtomicBool;
#[path = "support/atomic_json.rs"]
mod atomic_json;
#[derive(Serialize)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
enum Failure {
    Cancelled(String),
    Limit(String),
    Invalid(String),
}
#[derive(Serialize)]
#[serde(untagged)]
enum Reply {
    Ok { output: Output },
    Err { error: Failure },
}
fn main() -> Result<(), String> {
    atomic_json::run(1024, |r: Request, out| {
        let reply = match mtext_layout::layout(r, &AtomicBool::new(false)) {
            Ok(output) => Reply::Ok { output },
            Err(error) => {
                let message = error.to_string();
                let error = match error {
                    Error::Cancelled => Failure::Cancelled(message),
                    Error::Limit(_) => Failure::Limit(message),
                    Error::Invalid(_) => Failure::Invalid(message),
                };
                Reply::Err { error }
            }
        };
        serde_json::to_writer(out, &reply).map_err(|e| e.to_string())
    })
}
