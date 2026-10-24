//! Native reconstruction audit; reference input and geometry checks use GEOS.
use sleufbase_rust::{
    bgt_features,
    bgt_tiles::{Tile, TileFeature},
};
use sleufbase_rust::{bgt_geometry, bgt_surface::Geometry, Bounds};
use std::sync::atomic::AtomicBool;
#[derive(serde::Deserialize)]
struct Case {
    name: String,
    parts: Vec<Geometry>,
    bounds: Bounds,
}
#[derive(serde::Deserialize)]
struct Probe {
    start: [f64; 2],
    end: [f64; 2],
}
#[derive(serde::Deserialize)]
struct Group {
    name: String,
    bounds: Bounds,
    tiles: Vec<Vec<TileFeature>>,
    probes: Vec<Probe>,
}
fn main() -> Result<(), String> {
    let path = std::env::args().nth(1).ok_or("input JSON required")?;
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    if std::env::args().any(|a| a == "--groups") {
        let groups: Vec<Group> = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
        let cancel = AtomicBool::new(false);
        let mut results = Vec::new();
        for g in groups {
            let tiles = g
                .tiles
                .into_iter()
                .map(|features| Tile {
                    features,
                    memory_bytes: 0,
                })
                .collect::<Vec<_>>();
            let surfaces = bgt_features::surfaces(&tiles, g.bounds, &cancel)?;
            let paths = bgt_features::paths(&tiles, g.bounds, &cancel)?;
            let index = bgt_features::surface_index(&surfaces, &cancel)?;
            let votes = g
                .probes
                .iter()
                .map(|p| index.dominant(p.start, p.end, &cancel))
                .collect::<Result<Vec<_>, _>>()?;
            results.push(
                serde_json::json!({"name":g.name,"surfaces":surfaces,"paths":paths,"votes":votes}),
            );
        }
        println!(
            "{}",
            serde_json::to_string(&results).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    let cases: Vec<Case> = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    let cancel = AtomicBool::new(false);
    let results = cases
        .into_iter()
        .map(|c| {
            let refs = c.parts.iter().collect::<Vec<_>>();
            bgt_geometry::normalize(&refs, c.bounds, &cancel)
                .map(|geometry| serde_json::json!({"name":c.name,"geometry":geometry}))
        })
        .collect::<Result<Vec<_>, _>>()?;
    println!(
        "{}",
        serde_json::to_string(&results).map_err(|e| e.to_string())?
    );
    Ok(())
}
