//! Pixel audit driver for original TIFF image preparation.
use sleufbase_rust::{
    geo::GeoTransform,
    profile::XY,
    storage,
    template_raster::{self, Options, Source},
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
#[derive(serde::Deserialize)]
struct Case {
    name: String,
    input: String,
    kind: String,
    transform: GeoTransform,
    axis: Option<[XY; 2]>,
    roads: Vec<Vec<XY>>,
    terrain: Vec<Vec<XY>>,
    reverse: bool,
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
    std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/template-raster");
    let cases: Vec<Case> = serde_json::from_str(include_str!(
        "../tests/fixtures/template-raster/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    let cancel = AtomicBool::new(false);
    for c in cases {
        let path = root.join(c.input);
        let p = if c.kind == "tiff" {
            template_raster::from_tiff(
                &Source {
                    path,
                    transform: c.transform,
                    axis: c.axis,
                    reverse_orientation: c.reverse,
                },
                &c.roads,
                &c.terrain,
                None,
                &cancel,
            )?
        } else {
            let image = image::open(&path).map_err(|e| e.to_string())?.into_rgba8();
            template_raster::prepare(
                image,
                &Options {
                    transform: c.transform,
                    roads: &c.roads,
                    terrain: &c.terrain,
                    axis: c.axis,
                    reverse: c.reverse,
                },
                &cancel,
            )?
        };
        template_raster::save(&p, &output.join(format!("{}.png", c.name)), &cancel)?;
        storage::atomic_write(&output.join(format!("{}.json", c.name)), false, |out| {
            serde_json::to_writer(out, &p.details).map_err(|e| e.to_string())
        })?;
    }
    Ok(())
}
