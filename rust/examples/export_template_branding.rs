//! Readable-original paper logos/contact through the actual native image batch.
use sleufbase_rust::{
    dxf_document::Document, export_control::Control, storage, template_branding, template_export,
    template_images,
};
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
    std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let output = output.canonicalize().map_err(|e| e.to_string())?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/template-branding");
    let control = Control::default();
    let cancel = AtomicBool::new(false);
    let c: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/template-branding/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    let layouts: Vec<String> =
        serde_json::from_value(c["layouts"].clone()).map_err(|e| e.to_string())?;
    let mut doc = Document::load(&root.join("source.dxf"), &cancel)?;
    template_branding::restore_contact(&mut doc, &cancel)?;
    let tech = output.join("tech.png");
    let client = output.join("client.png");
    template_branding::prepare_logo(&root.join("tech.png"), &tech, &control)?;
    template_branding::prepare_logo(&root.join("client.jpg"), &client, &control)?;
    let images =
        template_branding::images(&doc, &layouts, Some(&tech), Some(&client), &cancel)?;
    template_images::add_images(&mut doc, &images, &cancel)?;
    storage::atomic_write(&output.join("branding.dxf"), false, |f| {
        doc.write_to(f, &cancel)
    })?;
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let base: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/template-export/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    let mut slot = base[0]["request"]["slots"][0].clone();
    slot["profile"] = serde_json::Value::Null;
    slot["tiff_png"] = serde_json::json!(fixture_root.join("template-export/tiff.png"));
    slot["map_png"] = serde_json::json!(fixture_root.join("template-export/map.png"));
    let request:template_export::Request=serde_json::from_value(serde_json::json!({
        "template":PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/cadastral_template.dxf"),
        "slots":vec![slot;9],"rules":[],"settings":{"project":"Logo's Ø","drawn_by":"Technicus","export_date":[2026,10,23],
        "logos":{"techbase":root.join("tech.png"),"client":root.join("client.jpg")}}
    })).map_err(|e|e.to_string())?;
    template_export::export(&output.join("complete.dxf"), &request, &control)?;
    Ok(())
}
