//! Native frame/band integration audit. Not a full template-export CLI.
use sleufbase_rust::{
    dxf_document::Document,
    profile::{self, Options as ProfileOptions, XY},
    profile_drawing::{self, Options, Templates},
    storage, template_geometry, Bounds, Dataset, Feature, Rule,
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
#[derive(serde::Deserialize)]
struct Profiles {
    rules: Vec<Rule>,
    cases: Vec<ProfileCase>,
}
#[derive(serde::Deserialize)]
struct ProfileCase {
    name: String,
    features: Vec<Feature>,
    roads: Vec<Vec<XY>>,
    terrain: Vec<Vec<XY>>,
    forced_start: Option<XY>,
    marker_scale: f64,
    reverse: bool,
    rules: Option<Vec<Rule>>,
}
#[derive(serde::Deserialize)]
struct Drawings {
    cases: Vec<Case>,
}
#[derive(serde::Deserialize)]
struct Case {
    name: String,
    profile_case: String,
    row: Bounds,
    tiff: Bounds,
    manual_scale: Option<i64>,
    label: String,
    reference_chainage: f64,
    distance_description: String,
}
fn main() -> Result<(), String> {
    let output = PathBuf::from(std::env::args().nth(1).ok_or("output directory required")?);
    if output.exists()
        && std::fs::read_dir(&output)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
    {
        return Err("Audit vereist een lege uitvoermap".into());
    }
    let profiles: Profiles =
        serde_json::from_str(include_str!("../tests/fixtures/profiles/reference.json"))
            .map_err(|e| e.to_string())?;
    let drawings: Drawings = serde_json::from_str(include_str!(
        "../tests/fixtures/profile-drawing/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    let source = include_bytes!("../tests/fixtures/profile-drawing/blocks.dxf");
    let cancel = AtomicBool::new(false);
    let template = Document::from_bytes(source.to_vec(), &cancel)?;
    let templates = Templates::read(&template, &cancel)?;
    for case in drawings.cases {
        let p = profiles
            .cases
            .iter()
            .find(|p| p.name == case.profile_case)
            .ok_or("missing profile case")?;
        let dataset = Dataset {
            job_id: 1,
            title: p.name.clone(),
            source_path: "fixture.json".into(),
            features: p.features.clone(),
        };
        let rules = p.rules.as_deref().unwrap_or(&profiles.rules);
        let profile = profile::build_with_cancel(
            &dataset,
            rules,
            ProfileOptions {
                roads: &p.roads,
                terrain_boundaries: &p.terrain,
                forced_start: p.forced_start,
                reverse: p.reverse,
                marker_scale: p.marker_scale,
            },
            &cancel,
        )?
        .ok_or("expected profile")?;
        let drawing = profile_drawing::frame_and_bands(
            &profile,
            &dataset,
            [case.row, case.tiff],
            &templates,
            Options {
                label: &case.label,
                manual_scale: case.manual_scale,
                marker_scale: p.marker_scale,
                reference_chainage: case.reference_chainage,
                distance_description: &case.distance_description,
            },
            &cancel,
        )?
        .ok_or("expected drawing")?;
        let mut doc = Document::from_bytes(source.to_vec(), &cancel)?;
        template_geometry::add(&mut doc, &[], &drawing.entities, &cancel)?;
        storage::atomic_write(&output.join(format!("{}.dxf", case.name)), false, |out| {
            doc.write_to(out, &cancel)
        })?;
        let metadata =
            serde_json::to_vec(&serde_json::json!({"layout":drawing.layout,"bands":drawing.bands}))
                .map_err(|e| e.to_string())?;
        storage::atomic_write(&output.join(format!("{}.json", case.name)), false, |out| {
            use std::io::Write;
            out.write_all(&metadata).map_err(|e| e.to_string())
        })?;
    }
    Ok(())
}
