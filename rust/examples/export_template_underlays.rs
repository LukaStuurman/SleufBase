//! Original-supported paper-layout underlay copies and shared definitions.
use sleufbase_rust::{dxf_document::Document, storage, template_pages};
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn main() -> Result<(), String> {
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("fresh output directory required")?,
    );
    if output.exists() {
        return Err("Output already exists".into());
    }
    let cancel = AtomicBool::new(false);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/template-underlays");
    let mut doc = Document::load(&root.join("source.dxf"), &cancel)?;
    for required in [9, 17, 33] {
        template_pages::extend(&mut doc, required, &cancel)?;
        storage::atomic_write(
            &output.join(format!("underlay-{required}.dxf")),
            false,
            |out| doc.write_to(out, &cancel),
        )?;
    }
    Ok(())
}
