//! Profile graphics audit: two geometry batches with raster insertion between.
use sleufbase_rust::{
    dxf_document::Document,
    export_control::Control,
    template_geometry::{self, Entity, Layer},
    template_images,
    template_transaction::Transaction,
    Bounds,
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
#[derive(serde::Deserialize)]
struct Case {
    name: String,
    path: String,
    layers: Vec<Layer>,
    requests: Vec<Entity>,
}
fn main() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let output = PathBuf::from(std::env::args().nth(1).ok_or("output directory required")?);
    if output.exists()
        && std::fs::read_dir(&output)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
    {
        return Err("Audit vereist een lege uitvoermap".into());
    }
    let cases: Vec<Case> = serde_json::from_str(include_str!(
        "../tests/fixtures/template-geometry/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    let cancel = AtomicBool::new(false);
    for case in cases {
        let control = Control::default();
        let transaction = Transaction::new(&output.join(format!("{}.dxf", case.name)), &control)?;
        let mut doc = Document::load(&root.join(case.path), &cancel)?;
        let mid = case.requests.len() / 2;
        if case.name != "blocks" {
            template_geometry::add(&mut doc, &case.layers, &case.requests[..mid], &cancel)?;
        }
        let raster = transaction.asset_path("shared.png")?;
        image::RgbaImage::from_pixel(12, 8, image::Rgba([4, 5, 6, 255]))
            .save(&raster)
            .map_err(|e| e.to_string())?;
        template_images::add_images(
            &mut doc,
            &[template_images::Image {
                path: raster,
                bounds: Bounds {
                    min_x: 0.,
                    min_y: 0.,
                    max_x: 2.,
                    max_y: 1.,
                },
                definition_name: "mixed".into(),
                inset: 0.,
                preserve_aspect: true,
                layout: None,
            }],
            &cancel,
        )?;
        if case.name == "blocks" {
            template_geometry::add(&mut doc, &case.layers, &case.requests[..mid], &cancel)?;
        }
        template_geometry::add(&mut doc, &case.layers, &case.requests[mid..], &cancel)?;
        let mut file =
            std::fs::File::create(transaction.staged_drawing()).map_err(|e| e.to_string())?;
        doc.write_to(&mut file, &cancel)?;
        drop(file);
        transaction.publish(&control)?;
    }
    Ok(())
}
