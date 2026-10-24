//! Real template IMAGE objects + permanent PNG assets, published twice per case.
//! Audit driver, not a full user-facing template export command.
use serde::{Deserialize, Serialize};
use sleufbase_rust::{
    dxf_document::Document,
    export_control::Control,
    storage,
    template_images::{self, Image},
    template_transaction::Transaction,
    Bounds,
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
#[derive(Deserialize)]
struct Request {
    filename: String,
    width: u32,
    height: u32,
    index: u32,
    bounds: Bounds,
    definition_name: String,
    inset: f64,
    preserve_aspect: bool,
    layout: Option<String>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    path: String,
    requests: Vec<Request>,
}
#[derive(Serialize)]
struct Manifest {
    drawing: PathBuf,
    assets: PathBuf,
}
fn main() -> Result<(), String> {
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("fresh output directory required")?,
    );
    if output.exists()
        && std::fs::read_dir(&output)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
    {
        return Err("Audit vereist een lege uitvoermap".into());
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let cases: Vec<Case> = serde_json::from_str(include_str!(
        "../tests/fixtures/template-images/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    let cancel = AtomicBool::new(false);
    for case in cases {
        let mut manifests = Vec::new();
        for round in 0..2 {
            let control = Control::default();
            let tx = Transaction::new(&output.join(format!("{}.dxf", case.name)), &control)?;
            let mut specs = Vec::new();
            for r in &case.requests {
                let path = tx.asset_path(&r.filename)?;
                let image = image::RgbaImage::from_fn(r.width, r.height, |x, y| {
                    image::Rgba([
                        ((x * 7 + r.index) % 256) as u8,
                        ((y * 11 + r.index * 3) % 256) as u8,
                        (((x + y) * 13) % 256) as u8,
                        (80 + (x * 3 + y * 5) % 176) as u8,
                    ])
                });
                image.save(&path).map_err(|e| e.to_string())?;
                specs.push(Image {
                    path,
                    bounds: r.bounds,
                    definition_name: r.definition_name.clone(),
                    inset: r.inset,
                    preserve_aspect: r.preserve_aspect,
                    layout: r.layout.clone(),
                });
            }
            let mut document = Document::load(&root.join(&case.path), &cancel)?;
            template_images::add_images(&mut document, &specs, &cancel)?;
            storage::atomic_write(tx.staged_drawing(), false, |out| {
                document.write_to(out, &cancel)
            })?;
            let published = tx.publish(&control)?;
            let drawing = if round == 0 {
                let copy = output.join(format!("{}-saved-copy.dxf", case.name));
                std::fs::copy(&published.drawing, &copy).map_err(|e| e.to_string())?;
                copy
            } else {
                published.drawing
            };
            manifests.push(Manifest {
                drawing,
                assets: published.assets,
            });
        }
        storage::atomic_write(
            &output.join(format!("{}-manifest.json", case.name)),
            false,
            |out| serde_json::to_writer_pretty(out, &manifests).map_err(|e| e.to_string()),
        )?;
    }
    Ok(())
}
