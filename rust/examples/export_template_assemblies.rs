//! Audit the production prepared-input exporter, including permanent bundles.
use sleufbase_rust::{
    export_control::Control,
    storage,
    template_export::{self, Request},
};
use std::path::PathBuf;
#[derive(serde::Deserialize)]
struct Case {
    name: String,
    request: Request,
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
    let cases: Vec<Case> = serde_json::from_str(include_str!(
        "../tests/fixtures/template-export/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    for mut case in cases {
        case.request.template = root.join(&case.request.template);
        for slot in case.request.slots.iter_mut().flatten() {
            if !slot.tiff_png.as_os_str().is_empty() {
                slot.tiff_png = root.join(&slot.tiff_png);
            }
            if let Some(source) = &mut slot.tiff_source {
                source.path = root.join(&source.path);
            }
            slot.map_png = root.join(&slot.map_png);
        }
        let result = template_export::export(
            &output.join(format!("{}.dxf", case.name)),
            &case.request,
            &Control::default(),
        )
        .map_err(|e| format!("{}: {e}", case.name))?;
        storage::atomic_write(&output.join(format!("{}.json", case.name)), false, |out| {
            serde_json::to_writer(out,&serde_json::json!({"filled":result.filled_slots,"profiles":result.profiles,"added_pages":result.added_pages,"assets":result.published.assets})).map_err(|e|e.to_string())
        })?;
    }
    Ok(())
}
