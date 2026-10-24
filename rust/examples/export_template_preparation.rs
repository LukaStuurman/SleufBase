//! Independent repeated preparation/resource-composition audit driver.
use sleufbase_rust::{
    dxf_document::Document,
    storage,
    template_geometry::{self, Entity, Layer},
    template_prepare::{self, LayerColor},
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
#[derive(serde::Deserialize)]
struct Case {
    name: String,
    path: String,
    steps: Vec<Step>,
}
#[derive(serde::Deserialize)]
struct Step {
    kind: String,
    points: Option<Vec<LayerColor>>,
    layers: Option<Vec<Layer>>,
    entities: Option<Vec<Entity>>,
}
fn main() -> Result<(), String> {
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("fresh output directory required")?,
    );
    if output.exists() {
        return Err("Output directory already exists".into());
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let cancel = AtomicBool::new(false);
    let cases: Vec<Case> = serde_json::from_str(include_str!(
        "../tests/fixtures/template-preparation/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    for case in cases {
        let mut doc = Document::load(&root.join(case.path), &cancel)?;
        for (index, step) in case.steps.into_iter().enumerate() {
            match step.kind.as_str() {
                "prepare" => {
                    let points = step.points.ok_or("points required")?;
                    template_prepare::profile(
                        &mut doc,
                        points.iter().map(|p| (p.name.as_str(), p.color)),
                        &cancel,
                    )?;
                }
                "geometry" => {
                    let layers = step.layers.ok_or("layers required")?;
                    let entities = step.entities.ok_or("entities required")?;
                    template_geometry::add(&mut doc, &layers, &entities, &cancel)?;
                }
                _ => return Err("Unknown preparation step".into()),
            }
            storage::atomic_write(
                &output.join(format!("{}-{index}.dxf", case.name)),
                false,
                |out| doc.write_to(out, &cancel),
            )?;
        }
    }
    Ok(())
}
