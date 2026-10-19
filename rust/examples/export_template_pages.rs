//! Sequential real/synthetic sheet extension and post-copy metadata audit.
use sleufbase_rust::{
    dxf_document::Document,
    storage, template_layout,
    template_metadata::{self, Date, Values},
    template_pages, Bounds,
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
    required: usize,
    error: bool,
    #[serde(default)]
    added_pages: usize,
}
fn bytes(doc: &Document) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    doc.write_to(&mut result, &AtomicBool::new(false))?;
    Ok(result)
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
        "../tests/fixtures/template-pages/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    for case in cases {
        let mut doc = Document::load(&root.join(case.path), &cancel)?;
        for (i, s) in case.steps.into_iter().enumerate() {
            let before = bytes(&doc)?;
            let result = template_pages::extend(&mut doc, s.required, &cancel);
            if s.error {
                if result.is_ok() || bytes(&doc)? != before {
                    return Err(format!("{}: expected atomic failure", case.name));
                }
                continue;
            }
            let result = result.map_err(|e| format!("{}-{i}: {e}", case.name))?;
            if result.added_pages != s.added_pages {
                return Err(format!("{}: added page mismatch", case.name));
            }
            let names = template_layout::used_layout_names(s.required)?;
            template_metadata::update(
                &mut doc,
                &names,
                &Values {
                    date: Date::new(2026, 10, 19)?,
                    project: Some("Uitbreiding Ø 漢".into()),
                    drawn_by: String::new(),
                },
                &cancel,
            )?;
            let slots = (0..s.required)
                .map(|i| {
                    Some(Bounds {
                        min_x: 120000. + i as f64 * 10.,
                        min_y: 450000. - i as f64,
                        max_x: 120020. + i as f64 * 10.,
                        max_y: 450020. - i as f64,
                    })
                })
                .collect::<Vec<_>>();
            template_layout::configure(&mut doc, &slots, 18., &cancel)?;
            storage::atomic_write(
                &output.join(format!("{}-{i}.dxf", case.name)),
                false,
                |out| doc.write_to(out, &cancel),
            )?;
            storage::atomic_write(
                &output.join(format!("{}-{i}.json", case.name)),
                false,
                |out| serde_json::to_writer(out, &result).map_err(|e| e.to_string()),
            )?;
        }
    }
    Ok(())
}
