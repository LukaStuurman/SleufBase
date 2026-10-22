//! Bounded/atomic compiled complex-MTEXT font/geometry audit.
use sleufbase_rust::mtext_render::{Renderer, Request};
use std::{path::PathBuf, sync::atomic::AtomicBool};
#[path = "support/atomic_json.rs"]
mod atomic_json;
#[path = "../tests/common/mtext_render.rs"]
mod common;
fn main() -> Result<(), String> {
    let cancel = AtomicBool::new(false);
    let root = std::env::var_os("SLEUFBASE_MTEXT_RENDER_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(common::root);
    let mut catalog = common::catalog(&root, &cancel)?;
    atomic_json::run(1024, |r: Request, out| {
        // Limits apply per composed document. Font bytes remain shared across
        // the audit; per-document glyph/descriptor caches cannot grow forever.
        let value = match Renderer::new(&mut catalog, &cancel).render(r) {
            Ok(output) => serde_json::json!({"output":common::summary(output)}),
            Err(error) => serde_json::json!({"error":error}),
        };
        serde_json::to_writer(out, &value).map_err(|e| e.to_string())
    })
}
