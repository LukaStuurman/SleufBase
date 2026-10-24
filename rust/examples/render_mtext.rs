//! Bounded/atomic compiled complex-MTEXT font/geometry audit.
use sleufbase_rust::mtext_render::{EstimateRequest, Renderer, Request, SimpleRequest};
use std::{path::PathBuf, sync::atomic::AtomicBool};
#[path = "support/atomic_json.rs"]
mod atomic_json;
#[path = "../tests/common/mtext_render.rs"]
mod common;
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Input {
    Complex(Request),
    Simple(SimpleRequest),
    Estimate(EstimateRequest),
}
fn main() -> Result<(), String> {
    let cancel = AtomicBool::new(false);
    let root = std::env::var_os("SLEUFBASE_MTEXT_RENDER_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(common::root);
    let mut catalog = common::catalog(&root, &cancel)?;
    atomic_json::run(1024, |r: Input, out| {
        // Limits apply per composed document. Font bytes remain shared across
        // the audit; per-document glyph/descriptor caches cannot grow forever.
        let result = match r {
            Input::Complex(r) => Renderer::new(&mut catalog, &cancel)
                .render(r)
                .map(common::summary),
            Input::Simple(r) => Renderer::new(&mut catalog, &cancel)
                .render_simple(r)
                .and_then(|r| serde_json::to_value(r).map_err(|e| e.to_string())),
            Input::Estimate(r) => Renderer::new(&mut catalog, &cancel)
                .estimate(r)
                .and_then(|r| serde_json::to_value(r).map_err(|e| e.to_string())),
        };
        let value = match result {
            Ok(output) => serde_json::json!({"output":output}),
            Err(error) => serde_json::json!({"error":error}),
        };
        serde_json::to_writer(out, &value).map_err(|e| e.to_string())
    })
}
