//! Independent native normal/reverse container and visibility-graph driver.
use sleufbase_rust::{
    dxf_document::Document,
    template_variants::{self, Pair},
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
#[derive(serde::Deserialize)]
struct Case {
    name: String,
    pairs: Vec<Pair>,
    wrappers: Vec<String>,
}
#[derive(serde::Deserialize)]
struct Input {
    cases: Vec<Case>,
}
fn main() -> Result<(), String> {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("fresh output directory required")?,
    );
    if out.exists() {
        return Err("Output already exists".into());
    }
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/template-variants");
    let data: Input = serde_json::from_str(include_str!(
        "../tests/fixtures/template-variants/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    let cancel = AtomicBool::new(false);
    for name in ["image.png", "reverse.png"] {
        std::fs::copy(root.join(name), out.join(name)).map_err(|e| e.to_string())?;
    }
    for c in data.cases {
        let mut doc = Document::load(&root.join("input.dxf"), &cancel)?;
        let names = template_variants::assemble(&mut doc, &c.pairs, &cancel)?;
        if names != c.wrappers {
            return Err(format!("Wrong wrapper names: {}", c.name));
        }
        let mut file = std::fs::File::create(out.join(format!("{}.dxf", c.name)))
            .map_err(|e| e.to_string())?;
        doc.write_to(&mut file, &cancel)?;
    }
    Ok(())
}
