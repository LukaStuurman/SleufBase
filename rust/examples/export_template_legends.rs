use sleufbase_rust::{
    dxf_document::Document,
    storage, template_legends,
    template_metadata::{self, Date, Values},
    template_pages,
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
#[derive(serde::Deserialize)]
struct Case {
    name: String,
    path: String,
    names: Vec<String>,
    required: Option<usize>,
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
        "../tests/fixtures/template-legends/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    for c in cases {
        let mut doc = Document::load(&root.join(c.path), &cancel)?;
        if let Some(required) = c.required {
            template_pages::extend(&mut doc, required, &cancel)?;
        }
        template_legends::restore(&mut doc, &c.names, &cancel)?;
        template_metadata::update(
            &mut doc,
            &c.names,
            &Values {
                date: Date::new(2026, 10, 19)?,
                project: Some("Legenda Ø 漢".into()),
                drawn_by: String::new(),
            },
            &cancel,
        )?;
        storage::atomic_write(&output.join(format!("{}.dxf", c.name)), false, |out| {
            doc.write_to(out, &cancel)
        })?;
        if template_legends::restore(&mut doc, &c.names, &cancel)? != 0 {
            return Err("Legend restoration is not idempotent".into());
        }
    }
    Ok(())
}
