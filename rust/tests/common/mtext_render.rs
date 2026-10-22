use serde_json::Value;
use sleufbase_rust::{dxf_fonts::Catalog, mtext_render::Output};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mtext-render")
}
pub fn catalog(root: &Path, cancel: &AtomicBool) -> Result<Catalog, String> {
    let mut dirs = std::fs::read_dir(root.join("fonts"))
        .map_err(|e| e.to_string())?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    dirs.sort();
    Catalog::from_dirs(&dirs, cancel)
}
pub fn summary(output: Output) -> Value {
    let mut value = serde_json::to_value(output).unwrap();
    // Every cell/line is already independently audited by mtext_layout. Here
    // compare the actual composed adapter's columns and renderer callbacks.
    for c in value["layout"]["columns"].as_array_mut().unwrap() {
        c.as_object_mut().unwrap().remove("paragraphs");
    }
    value
}
