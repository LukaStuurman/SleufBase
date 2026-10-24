//! Cold decode + RD preparation versus shared, decoded native cache requests.
use sleufbase_rust::{
    bgt_surface::Geometry,
    bgt_tile_client::{Client, Options, Transport},
    bgt_tiles::{self, TileId},
};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Instant,
};
const SOURCE: &[u8] = include_bytes!("../tests/fixtures/bgt-tiles/pdok-12-2420-2075.pbf");
struct Snapshot(Arc<AtomicUsize>);
impl Transport for Snapshot {
    fn load(&self, _: TileId, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(SOURCE.to_vec())
    }
}
fn main() -> Result<(), String> {
    let id = TileId {
        row: 2420,
        col: 2075,
    };
    let cancel = AtomicBool::new(false);
    let loops = 20;
    let mut features = 0;
    let mut surfaces = 0;
    let begin = Instant::now();
    for _ in 0..loops {
        let tile = bgt_tiles::decode(SOURCE, id, &cancel)?;
        features += tile.features.len();
        surfaces += tile
            .features
            .iter()
            .filter(|f| {
                !f.physical_appearance.is_empty()
                    && matches!(f.geometry, Geometry::Polygon(_) | Geometry::MultiPolygon(_))
            })
            .count();
        std::hint::black_box(tile);
    }
    let cold_ms = begin.elapsed().as_secs_f64() * 1000.;
    let calls = Arc::new(AtomicUsize::new(0));
    let client = Client::with_transport(Snapshot(calls.clone()), Options::default())?;
    let first = client.tile(id, &cancel)?;
    let cache_requests = 20000;
    let begin = Instant::now();
    for _ in 0..cache_requests {
        let current = client.tile(id, &cancel)?;
        if !Arc::ptr_eq(&first, &current) {
            return Err("cache copied geometry".into());
        }
        std::hint::black_box(current);
    }
    let cache_ms = begin.elapsed().as_secs_f64() * 1000.;
    println!(
        "{}",
        serde_json::json!({"loops":loops,"bytes":SOURCE.len(),"features":features,"surfaces":surfaces,"cold_ms":cold_ms,"cache_requests":cache_requests,"cache_ms":cache_ms,"transport_calls":calls.load(Ordering::Relaxed),"cached_memory_estimate":first.memory_bytes})
    );
    Ok(())
}
