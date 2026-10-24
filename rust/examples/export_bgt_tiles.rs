//! Independent offline tile audit; --live additionally records the actual HTTP
//! response so Python can compare the exact same bytes without another request.
use sleufbase_rust::{
    bgt_tile_client::{Client, HttpTransport, Options, Transport},
    bgt_tiles::{self, Tile, TileId, ORIGIN_X, ORIGIN_Y, SPAN},
    storage, Bounds,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};
#[derive(serde::Deserialize)]
struct Case {
    name: String,
    path: String,
    id: TileId,
}
#[derive(serde::Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}
fn write_tile(output: &std::path::Path, tile: &Tile) -> Result<(), String> {
    storage::atomic_write(output, false, |out| {
        serde_json::to_writer(out, &tile.features).map_err(|e| e.to_string())
    })?;
    Ok(())
}
struct Record {
    inner: HttpTransport,
    bytes: Arc<Mutex<Option<Vec<u8>>>>,
    calls: Arc<AtomicUsize>,
}
impl Transport for Record {
    fn load(&self, id: TileId, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let bytes = self.inner.load(id, cancel)?;
        *self.bytes.lock().map_err(|_| "record lock")? = Some(bytes.clone());
        Ok(bytes)
    }
}
fn main() -> Result<(), String> {
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("fresh output directory required")?,
    );
    if output.exists() {
        return Err("Output already exists".into());
    }
    let cancel = AtomicBool::new(false);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    if std::env::args().any(|a| a == "--live") {
        let id = TileId {
            row: 2420,
            col: 2075,
        };
        let options = Options::default();
        let bytes = Arc::new(Mutex::new(None));
        let calls = Arc::new(AtomicUsize::new(0));
        let client = Client::with_transport(
            Record {
                inner: HttpTransport::new(options)?,
                bytes: bytes.clone(),
                calls: calls.clone(),
            },
            options,
        )?;
        let x = ORIGIN_X + id.col as f64 * SPAN;
        let y = ORIGIN_Y - id.row as f64 * SPAN;
        let bounds = Bounds {
            min_x: x + 20.,
            min_y: y - SPAN + 20.,
            max_x: x + SPAN - 20.,
            max_y: y - 20.,
        };
        let first = client.fetch(bounds, &cancel)?;
        let after_first = calls.load(Ordering::Relaxed);
        let second = client.fetch(bounds, &cancel)?;
        if first.len() != 1
            || second.len() != 1
            || !Arc::ptr_eq(&first[0].1, &second[0].1)
            || calls.load(Ordering::Relaxed) != after_first
        {
            return Err("Live cache reuse failed".into());
        }
        let recorded = bytes
            .lock()
            .map_err(|_| "record lock")?
            .take()
            .ok_or("missing recorded bytes")?;
        storage::atomic_write(&output.join("live.pbf"), false, |out| {
            use std::io::Write;
            out.write_all(&recorded).map_err(|e| e.to_string())
        })?;
        write_tile(&output.join("live.json"), &first[0].1)?;
        let metadata = serde_json::json!({"id":id,"http_calls":after_first,"cache_reused":true});
        storage::atomic_write(&output.join("live-metadata.json"), false, |out| {
            serde_json::to_writer(out, &metadata).map_err(|e| e.to_string())
        })?;
    } else {
        let fixture: Fixture =
            serde_json::from_str(include_str!("../tests/fixtures/bgt-tiles/reference.json"))
                .map_err(|e| e.to_string())?;
        for case in fixture.cases {
            let bytes = std::fs::read(root.join(case.path)).map_err(|e| e.to_string())?;
            let tile = bgt_tiles::decode(&bytes, case.id, &cancel)?;
            write_tile(&output.join(format!("{}.json", case.name)), &tile)?;
        }
    }
    Ok(())
}
