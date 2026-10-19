//! Native frame/band integration audit. Not a full template-export CLI.
use sleufbase_rust::{
    dxf_document::Document,
    profile::{self, Options as ProfileOptions, XY},
    profile_drawing::{self, Options, Templates},
    profile_leaders, profile_reference, profile_surface, storage, template_geometry, Bounds,
    Dataset, Feature, Rule,
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
    reference_cases: Vec<ReferenceCase>,
}
#[derive(serde::Deserialize)]
struct ReferenceCase {
    profile_case: String,
    layout: profile_drawing::Layout,
    right_edge: f64,
    metadata: serde_json::Value,
    center: [f64; 2],
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
    metadata: serde_json::Value,
    automatic: serde_json::Value,
    use_custom: bool,
    clip_markers: bool,
    show_reference: bool,
    diameter_prefix: String,
}
fn main() -> Result<(), String> {
    let leader_mode =
        std::env::args().any(|a| matches!(a.as_str(), "--leaders" | "--leaders-collisions"));
    let collision_mode = std::env::args().any(|a| a == "--leaders-collisions");
    let reference_mode = leader_mode || std::env::args().any(|a| a == "--reference");
    let reference_cases = std::env::args().any(|a| a == "--reference-cases");
    let body_mode = reference_mode || std::env::args().any(|a| a == "--body");
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
    if reference_cases {
        for (index, case) in drawings.reference_cases.into_iter().enumerate() {
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
            let profile = profile::build(
                &dataset,
                p.rules.as_deref().unwrap_or(&profiles.rules),
                ProfileOptions {
                    roads: &p.roads,
                    terrain_boundaries: &p.terrain,
                    forced_start: p.forced_start,
                    reverse: p.reverse,
                    marker_scale: p.marker_scale,
                },
            )?
            .ok_or("expected profile")?;
            let reference = profile_reference::resolve(&profile, &case.metadata)?;
            let display = reference.display(&case.metadata, case.center)?;
            let indicator = profile_reference::draw(
                &profile,
                case.layout,
                case.right_edge,
                &reference,
                &cancel,
            )?;
            let mut doc = Document::from_bytes(source.to_vec(), &cancel)?;
            template_geometry::add(&mut doc, &[], &indicator.entities, &cancel)?;
            storage::atomic_write(&output.join(format!("ref-{index}.dxf")), false, |out| {
                doc.write_to(out, &cancel)
            })?;
            std::fs::write(output.join(format!("ref-{index}.json")),serde_json::to_vec(&serde_json::json!({"reference":reference,"display":display,"segments":indicator.segments,"boxes":indicator.boxes})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        }
        return Ok(());
    }
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
        let reference = if reference_mode && case.show_reference {
            Some(profile_reference::resolve(&profile, &case.metadata)?)
        } else {
            None
        };
        let mut drawing = profile_drawing::frame_and_bands(
            &profile,
            &dataset,
            [case.row, case.tiff],
            &templates,
            Options {
                label: &case.label,
                manual_scale: case.manual_scale,
                marker_scale: p.marker_scale,
                reference_chainage: reference
                    .as_ref()
                    .map_or(case.reference_chainage, |r| r.chainage),
                distance_description: reference
                    .as_ref()
                    .map_or(case.distance_description.as_str(), |r| {
                        r.description.as_str()
                    }),
            },
            &cancel,
        )?
        .ok_or("expected drawing")?;
        let mut doc = Document::from_bytes(source.to_vec(), &cancel)?;
        let mut obstacles = None;
        let mut fills = None;
        let mut layers = Vec::new();
        let mut leader_plan = None;
        if body_mode {
            let resolved =
                profile_surface::resolve(&case.metadata, &case.automatic, case.use_custom)?;
            let mut body = profile_surface::build(
                &profile,
                &dataset,
                &drawing,
                case.tiff,
                &resolved,
                profile_surface::Options {
                    clip_markers: case.clip_markers,
                    marker_in_leader: false,
                },
                &cancel,
            )?;
            if let Some(reference) = &reference {
                profile_reference::draw(
                    &profile,
                    drawing.layout,
                    case.tiff.min_x,
                    reference,
                    &cancel,
                )?
                .append(&mut body);
            }
            let (required, context) = profile_surface::compose(&mut drawing, body)?;
            layers = required;
            obstacles = Some(context);
            fills = Some(resolved);
            if drawing.entities.iter().any(|e|matches!(&e.shape,template_geometry::Shape::Block{name,..} if name==profile_surface::MARKER)){profile_surface::prepare_marker(&mut doc,&cancel)?;}
        }
        if leader_mode {
            let context = obstacles
                .as_ref()
                .ok_or("Leader stage requires profile body")?;
            let plan = profile_leaders::compose(
                &profile,
                &dataset,
                &mut drawing,
                context,
                profile_leaders::ProfileOptions {
                    marker_scale: p.marker_scale,
                    diameter_prefix: &case.diameter_prefix,
                    avoid_collisions: collision_mode,
                },
                &cancel,
            )?;
            if !plan.entries.is_empty() {
                profile_leaders::prepare(&mut doc, &cancel)?;
                layers.push(template_geometry::Layer {
                    name: profile_leaders::LAYER.into(),
                    color: 7,
                    true_color: None,
                    lineweight: Some(0),
                });
            }
            leader_plan = Some(plan);
        }
        template_geometry::add(&mut doc, &layers, &drawing.entities, &cancel)?;
        storage::atomic_write(&output.join(format!("{}.dxf", case.name)), false, |out| {
            doc.write_to(out, &cancel)
        })?;
        let metadata =
            serde_json::to_vec(&serde_json::json!({"layout":drawing.layout,"bands":drawing.bands,"obstacles":obstacles,"fills":fills,"leader_entries":leader_plan.as_ref().map(|p|&p.entries),"leader_placements":leader_plan.as_ref().map(|p|&p.placed)}))
                .map_err(|e| e.to_string())?;
        storage::atomic_write(&output.join(format!("{}.json", case.name)), false, |out| {
            use std::io::Write;
            out.write_all(&metadata).map_err(|e| e.to_string())
        })?;
    }
    Ok(())
}
