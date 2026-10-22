//! Run the production standalone batch on explicit inputs without app settings.
use serde::Deserialize;
use sleufbase_rust::{
    dxf_read::DxfOverlay, export_control::Control, export_worker::Inputs, geotiff::GeoTiffInfo,
    map_batch, map_text::Paths,
};
use std::{io::Read, path::PathBuf};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    rasters: Vec<GeoTiffInfo>,
    settings: map_batch::Settings,
    fonts: Option<Paths>,
    #[serde(default)]
    overlays: Vec<DxfOverlay>,
}
fn main() -> Result<(), String> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err("Gebruik export_map_batch invoer.json uitvoermap".into());
    }
    let input = std::fs::canonicalize(PathBuf::from(&args[0])).map_err(|e| e.to_string())?;
    let parent = input.parent().unwrap();
    let mut body = Vec::new();
    std::fs::File::open(&input)
        .map_err(|e| e.to_string())?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    if body.len() > 64 * 1024 * 1024 {
        return Err("Kaartinvoer overschrijdt 64 MiB".into());
    }
    let mut request: Request = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    let resolve = |p: &mut PathBuf| {
        if p.is_relative() {
            *p = parent.join(&*p);
        }
    };
    for raster in &mut request.rasters {
        resolve(&mut raster.path);
        if let Some(p) = &mut raster.kickthemap_job_sidecar {
            resolve(p);
        }
    }
    if let Some(fonts) = &mut request.fonts {
        resolve(&mut fonts.regular);
        resolve(&mut fonts.bold);
    }
    let inputs = Inputs {
        rasters: request.rasters,
        overlays: request.overlays,
        protected_paths: vec![input],
        ..Default::default()
    };
    let paths = map_batch::export_with(
        &PathBuf::from(&args[1]),
        &request.settings,
        &inputs,
        request.fonts,
        &Control::default(),
    )?;
    println!(
        "{}",
        serde_json::to_string(&paths).map_err(|e| e.to_string())?
    );
    Ok(())
}
