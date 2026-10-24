use serde::Deserialize;
use serde_json::{json, Value};
use sleufbase_rust::{dxf_fonts::Catalog, dxf_text_metrics::Text};
use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    sync::atomic::AtomicBool,
};
#[derive(Deserialize)]
struct Fixture {
    sources: HashMap<String, PathBuf>,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    files: Vec<Input>,
    queries: Vec<Query>,
}
#[derive(Deserialize)]
struct Input {
    name: String,
    source: Option<String>,
    text: Option<String>,
}
#[derive(Deserialize)]
struct Query {
    name: String,
    text: Text,
    remove: Vec<String>,
}
fn owned_file(root: &Path, name: &str) -> Result<PathBuf, String> {
    if name.is_empty()
        || !Path::new(name)
            .components()
            .all(|p| matches!(p, Component::Normal(_)))
        || Path::new(name).components().count() != 1
    {
        return Err("Fixture filenames must be a single relative component".into());
    }
    Ok(root.join(name))
}
pub fn run() -> Result<Value, String> {
    let fixture: Fixture =
        serde_json::from_str(include_str!("../fixtures/font-resolution/reference.json"))
            .map_err(|e| e.to_string())?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/font-resolution");
    let cancel = AtomicBool::new(false);
    let mut cases = Vec::new();
    for case in fixture.cases {
        let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
        for file in case.files {
            let dest = owned_file(temp.path(), &file.name)?;
            if let Some(key) = file.source {
                std::fs::copy(
                    root.join(fixture.sources.get(&key).ok_or("Unknown fixture asset")?),
                    dest,
                )
                .map_err(|e| e.to_string())?;
            } else {
                std::fs::write(dest, file.text.unwrap_or_default()).map_err(|e| e.to_string())?;
            }
        }
        let mut catalog = Catalog::from_dirs(&[temp.path().to_owned()], &cancel)?;
        let mut results = Vec::new();
        for query in case.queries {
            for name in query.remove {
                let path = owned_file(temp.path(), &name)?;
                match std::fs::remove_file(path) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
                    _ => {}
                }
            }
            let result=catalog.resolve(&query.name,&cancel).and_then(|mut f|{
                let extent=f.extent(&query.text,&cancel)?;
                Ok(json!({"status":"ok","kind":f.kind(),"selected":catalog.selected_path(&query.name).and_then(|p|p.file_name()).map(|s|s.to_string_lossy()),"extent":extent}))
            });
            results.push(match result {
                Ok(value) => value,
                Err(error) => json!({"status":"error","message":error}),
            });
        }
        cases.push(json!({"name":case.name,"actual":results}));
    }
    Ok(json!(cases))
}
