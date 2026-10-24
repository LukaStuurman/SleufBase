//! Active production LEADER placement/export audit, not the complete UI.
use sleufbase_rust::{
    dxf_document::Document,
    profile_leaders::{self, Entry, Options},
    storage, template_geometry,
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
#[derive(serde::Deserialize)]
struct Case {
    entries: Vec<Entry>,
    options: Settings,
    #[serde(default = "scale")]
    marker_scale: f64,
}
fn scale() -> f64 {
    0.02
}
#[derive(serde::Deserialize)]
struct Settings {
    hard_static_line_segments: Vec<[f64; 4]>,
    soft_static_line_segments: Vec<[f64; 4]>,
    static_text_boxes: Vec<[f64; 4]>,
    min_text_x: Option<f64>,
    max_text_x: Option<f64>,
    avoid_collisions: bool,
}
fn main() -> Result<(), String> {
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("fresh output directory required")?,
    );
    if output.exists() {
        return Err("audit output directory already exists".into());
    }
    let cases: Vec<Case> = serde_json::from_str(include_str!(
        "../tests/fixtures/profile-leaders/layout.json"
    ))
    .map_err(|e| e.to_string())?;
    let cancel = AtomicBool::new(false);
    for (index, case) in cases.into_iter().enumerate() {
        let mut doc = Document::from_bytes(
            include_bytes!("../tests/fixtures/profile-leaders/empty.dxf").to_vec(),
            &cancel,
        )?;
        let s = &case.options;
        let placed = profile_leaders::solve(
            &case.entries,
            case.marker_scale,
            Options {
                hard: &s.hard_static_line_segments,
                soft: &s.soft_static_line_segments,
                text: &s.static_text_boxes,
                min_x: s.min_text_x,
                max_x: s.max_text_x,
                avoid_collisions: s.avoid_collisions,
            },
            &cancel,
        )?;
        let mut entities = Vec::new();
        if !placed.is_empty() {
            profile_leaders::prepare(&mut doc, &cancel)?;
            profile_leaders::append(&mut entities, &case.entries, &placed, &cancel)?;
        }
        template_geometry::add(&mut doc, &[], &entities, &cancel)?;
        storage::atomic_write(&output.join(format!("{index}.dxf")), false, |out| {
            doc.write_to(out, &cancel)
        })?;
    }
    Ok(())
}
