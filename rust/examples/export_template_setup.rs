use sleufbase_rust::{
    dxf_document::Document,
    storage,
    template_metadata::Date,
    template_prepare::LayerColor,
    template_setup::{self, Options},
    Bounds,
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
    slots: Vec<Option<Bounds>>,
    project: Option<String>,
    drawn_by: String,
    label_gap: f64,
    profile_layers: Option<Vec<LayerColor>>,
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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let cancel = AtomicBool::new(false);
    let cases: Vec<Case> = serde_json::from_str(include_str!(
        "../tests/fixtures/template-setup/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    for c in cases {
        let mut doc = Document::load(&root.join(c.path), &cancel)?;
        for (i, s) in c.steps.into_iter().enumerate() {
            let prepared = template_setup::prepare(
                &mut doc,
                &s.slots,
                &Options {
                    project: s.project,
                    drawn_by: s.drawn_by,
                    export_date: Some(Date::new(2026, 10, 19)?),
                    label_gap: s.label_gap,
                    profile_layers: s.profile_layers,
                },
                &cancel,
            )
            .map_err(|e| format!("{}-{i}: {e}", c.name))?;
            storage::atomic_write(&output.join(format!("{}-{i}.dxf", c.name)), false, |out| {
                doc.write_to(out, &cancel)
            })?;
            storage::atomic_write(&output.join(format!("{}-{i}.json", c.name)), false, |out| {
                serde_json::to_writer(out, &prepared).map_err(|e| e.to_string())
            })?;
        }
    }
    Ok(())
}
