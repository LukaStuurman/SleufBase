//! Explicit synthetic tiles through production WMTS planning and composition.
use image::{Rgba, RgbaImage};
use serde::Deserialize;
use serde_json::json;
use sleufbase_rust::{
    cadastral_tiles::{Plan, TileId},
    Bounds,
};
use std::sync::{atomic::AtomicBool, Arc};
#[path = "support/atomic_json.rs"]
mod atomic_json;
#[derive(Deserialize)]
struct Request {
    bounds: Bounds,
    size: [u32; 2],
    salt: u32,
    #[serde(default)]
    cache: Option<std::path::PathBuf>,
}
fn tile(id: TileId, salt: u32) -> Arc<RgbaImage> {
    Arc::new(RgbaImage::from_fn(256, 256, |x, y| {
        Rgba([
            ((x * 17 + y * 3 + id.x * 7 + id.y * 11 + salt) % 245) as u8,
            ((x * 7 + y * 11 + id.x * 13 + id.y * 3 + salt) % 245) as u8,
            ((x * 23 + y * 19 + id.x * 3 + id.y * 5 + salt) % 245) as u8,
            [0, 31, 127, 201, 255][((x + y + id.x + id.y) % 5) as usize],
        ])
    }))
}
fn main() -> Result<(), String> {
    atomic_json::run(256, |request: Request, out| {
        let result: Result<serde_json::Value, String> = (|| {
            if request.salt > 244 {
                return Err("Ongeldige proefkleur".into());
            }
            let plan = Plan::new(request.bounds, request.size)?;
            if request.size.iter().copied().map(u64::from).product::<u64>() > 1_000_000 {
                return Err("Te grote pixelaudit".into());
            }
            let image = if let Some(cache) = request.cache {
                sleufbase_rust::cadastral_tiles::Client::new(cache)?.request(
                    request.bounds,
                    request.size,
                    &AtomicBool::new(false),
                )?
            } else {
                plan.render_with(&AtomicBool::new(false), |ids| {
                    Ok(ids.iter().map(|&id| tile(id, request.salt)).collect())
                })?
            };
            Ok(
                json!({"urls":plan.tiles().iter().map(|id|id.url()).collect::<Vec<_>>(),"plan":plan,"pixels":image.into_raw()}),
            )
        })();
        serde_json::to_writer(
            out,
            &match result {
                Ok(value) => value,
                Err(error) => json!({"error":error}),
            },
        )
        .map_err(|e| e.to_string())
    })
}
