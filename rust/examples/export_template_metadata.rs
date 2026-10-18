//! Independent title-block audit driver; not the full template export pipeline.
use sleufbase_rust::{
    dxf_document::Document,
    storage,
    template_metadata::{self, Date, Values},
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn main() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let output = PathBuf::from(std::env::args().nth(1).ok_or("output directory required")?);
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/template-metadata/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    let cancel = AtomicBool::new(false);
    for case in cases.as_array().unwrap() {
        let mut doc = Document::load(&root.join(case["path"].as_str().unwrap()), &cancel)?;
        let layouts = case["layouts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().into())
            .collect::<Vec<_>>();
        let date = &case["date"];
        template_metadata::update(
            &mut doc,
            &layouts,
            &Values {
                date: Date::new(
                    date[0].as_u64().unwrap() as u16,
                    date[1].as_u64().unwrap() as u16,
                    date[2].as_u64().unwrap() as u16,
                )?,
                project: case["project"].as_str().map(str::to_owned),
                drawn_by: case["drawn_by"].as_str().unwrap().into(),
            },
            &cancel,
        )?;
        storage::atomic_write(
            &output.join(format!("{}.dxf", case["name"].as_str().unwrap())),
            false,
            |out| doc.write_to(out, &cancel),
        )?;
    }
    // Report Windows local date for the Python audit, avoiding a fixed/UTC clock.
    #[cfg(windows)]
    std::fs::write(output.join("local-date.txt"), Date::today()?.to_string())
        .map_err(|e| e.to_string())?;
    Ok(())
}
