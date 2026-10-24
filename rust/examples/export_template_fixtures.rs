//! Source-preservation and text-edit audits; not a full template export command.
use sleufbase_rust::{dxf_document::Document, storage, template_slots};
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn main() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let output = PathBuf::from(std::env::args().nth(1).ok_or("output directory required")?);
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/templates/reference.json"))
            .map_err(|e| e.to_string())?;
    let cancel = AtomicBool::new(false);
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut document = Document::load(&root.join(case["path"].as_str().unwrap()), &cancel)?;
        storage::atomic_write(
            &output.join(format!("{name}-unchanged.dxf")),
            false,
            |out| document.write_to(out, &cancel),
        )?;
        if case["error"] == true {
            continue;
        }
        for (i, slot) in template_slots::detect(&document, &cancel)?
            .iter()
            .enumerate()
        {
            document.replace_text(&slot.label_handle, &format!("Proefsleuf {}", i + 1))?;
            if let Some(id) = &slot.address_handle {
                document.replace_text(
                    id,
                    &format!("Adres\\PNieuw Ø {} {}", i + 1, "漢😀".repeat(150)),
                )?;
            }
            if let Some(id) = &slot.comments_handle {
                document.replace_text(id, &format!("Opmerkingen\\PMeting {}", i + 1))?;
            }
        }
        storage::atomic_write(&output.join(format!("{name}-edited.dxf")), false, |out| {
            document.write_to(out, &cancel)
        })?;
    }
    Ok(())
}
