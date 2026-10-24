use serde_json::{json, Value};
use sleufbase_rust::{
    export_control::Control,
    export_worker::{Exporter, Inputs, Kind, Request},
    geotiff::GeoTiffInfo,
    raster_points as points, Dataset,
};
use std::{
    path::PathBuf,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
fn reference() -> Value {
    serde_json::from_str(include_str!("fixtures/raster-points/reference.json")).unwrap()
}
fn raster(c: &Value) -> GeoTiffInfo {
    let transform =
        serde_json::from_value::<sleufbase_rust::geo::GeoTransform>(c["transform"].clone())
            .unwrap();
    GeoTiffInfo {
        path: PathBuf::from(format!("source-{}.tiff", c["dataset"]["job_id"])),
        width: 20,
        height: 20,
        transform,
        bounds: transform.bounds(20, 20).unwrap(),
        epsg: Some(28992),
        kickthemap_job_id: c["dataset"]["job_id"].as_u64(),
        kickthemap_job_title: "PS".into(),
        kickthemap_job_sidecar: None,
        metadata: c["metadata"].clone(),
    }
}
fn close(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 2e-8,
            "{a} != {b}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                close(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (k, a) in a {
                close(a, &b[k]);
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
fn original_rotation_setters_candidate_rows_automatic_starts_and_hit_selection_match() {
    let data = reference();
    let cancel = AtomicBool::new(false);
    for c in data["cases"].as_array().unwrap() {
        let mut raster = raster(c);
        let dataset: Dataset = serde_json::from_value(c["dataset"].clone()).unwrap();
        let roads = serde_json::from_value::<Vec<Vec<[f64; 2]>>>(c["roads"].clone()).unwrap();
        let terrain: Vec<Vec<[f64; 2]>> = serde_json::from_value(c["terrain"].clone()).unwrap();
        close(
            &serde_json::to_value(
                points::automatic(
                    &dataset,
                    sleufbase_rust::RULES,
                    &raster.metadata,
                    &roads,
                    &terrain,
                    &cancel,
                )
                .unwrap(),
            )
            .unwrap(),
            &c["automatic"],
        );
        let rows = points::rows(
            &raster,
            &dataset,
            sleufbase_rust::RULES,
            &roads,
            &terrain,
            &cancel,
        )
        .unwrap();
        close(&serde_json::to_value(&rows).unwrap(), &c["rows"]);
        for p in c["picks"].as_array().unwrap() {
            let selected = points::pick(
                &rows,
                serde_json::from_value(p["world"].clone()).unwrap(),
                c["meters_per_pixel"].as_f64().unwrap(),
                c["selection_tolerance"].as_f64().unwrap(),
            );
            assert_eq!(serde_json::to_value(selected).unwrap(), p["expected"]);
        }
        points::set(
            &mut raster,
            points::REFERENCE,
            serde_json::from_value(c["reference_click"].clone()).unwrap(),
            true,
        )
        .unwrap();
        close(&raster.metadata, &c["after_reference"]);
        let display =
            points::display(&raster, points::stored(&raster, points::REFERENCE).unwrap()).unwrap();
        close(&json!(display), &c["reference_display"]);
        points::set(
            &mut raster,
            points::START,
            serde_json::from_value(c["start_click"].clone()).unwrap(),
            false,
        )
        .unwrap();
        close(&raster.metadata, &c["after_start"]);
    }
}
#[test]
fn original_clear_removes_even_invalid_values_but_keeps_other_metadata_and_starts() {
    let data = reference();
    let r = raster(&data["cases"][0]);
    let mut rows = vec![r.clone(), r.clone(), r];
    rows[0].metadata = json!({});
    rows[1].metadata = json!({"template_reference_point":null,"keep":"x"});
    rows[2].metadata = json!({"template_reference_point":{"x":1,"y":2},"template_cross_section_start_point":{"x":4,"y":5}});
    assert_eq!(points::clear_reference(&mut rows), 2);
    assert_eq!(
        json!(rows.iter().map(|r| &r.metadata).collect::<Vec<_>>()),
        data["clear"]["first"]["metadata"]
    );
    assert_eq!(points::clear_reference(&mut rows), 0);
}
fn request() -> Request {
    Request {
        kind: Kind::StartPoints,
        path: PathBuf::new(),
        selection: sleufbase_rust::object_selection::Selection::all(0, 0),
        rules: sleufbase_rust::RULES.to_vec(),
        extra_layers: false,
        options: Default::default(),
        template: None,
        discipline_excel: false,
        maps: None,
        point_target: None,
        point_network: false,
    }
}
#[test]
fn actual_worker_stages_updates_returns_original_allocations_and_isolates_missing_jobs() {
    let data = reference();
    let cases = data["cases"].as_array().unwrap();
    let mut input = Inputs {
        rasters: cases.iter().take(12).map(raster).collect(),
        datasets: cases
            .iter()
            .take(11)
            .map(|c| serde_json::from_value(c["dataset"].clone()).unwrap())
            .collect(),
        ..Default::default()
    };
    let pointer = input.rasters.as_ptr();
    let feature_pointer = input.datasets[0].features.as_ptr();
    let before = serde_json::to_value(&input.rasters).unwrap();
    let mut worker = Exporter::new().unwrap();
    worker.start(input, request()).unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    let ready = loop {
        if let Some(r) = worker.poll() {
            break r;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(2));
    };
    input = ready.inputs;
    assert_eq!(input.rasters.as_ptr(), pointer);
    assert_eq!(input.datasets[0].features.as_ptr(), feature_pointer);
    assert_eq!(serde_json::to_value(&input.rasters).unwrap(), before);
    let output = ready.result.unwrap();
    assert!(!output.point_updates.is_empty());
    assert!(output
        .warnings
        .iter()
        .any(|w| w.contains("Geen gekoppelde objectdata")));
    let changed = points::apply(&mut input.rasters, &output.point_updates).unwrap();
    assert!(changed > 0);
    for update in &output.point_updates {
        assert_eq!(
            points::stored(&input.rasters[update.index], points::START),
            Some(update.point)
        );
    }
    assert_eq!(
        points::apply(&mut input.rasters, &output.point_updates).unwrap(),
        0
    );
}
#[test]
fn project_roundtrip_legacy_defaults_metadata_budget_and_atomic_update_identity_are_checked() {
    let data = reference();
    let mut r = raster(&data["cases"][1]);
    let mut old = serde_json::to_value(&r).unwrap();
    old.as_object_mut().unwrap().remove("metadata");
    let restored: GeoTiffInfo = serde_json::from_value(old).unwrap();
    assert!(restored.metadata.is_null());
    points::set(&mut r, points::REFERENCE, [155000., 463000.], false).unwrap();
    let expected = r.metadata.clone();
    let project = sleufbase_rust::project::NativeProject {
        rasters: vec![r.clone()],
        ..Default::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("points.json");
    sleufbase_rust::project::save_project_new(&path, &project).unwrap();
    let loaded = sleufbase_rust::project::read_project(&path).unwrap();
    assert_eq!(loaded.rasters[0].metadata, expected);
    let before = r.metadata.clone();
    let updates = [
        points::Update {
            index: 0,
            path: r.path.clone(),
            job_id: r.kickthemap_job_id,
            point: [1., 2.],
        },
        points::Update {
            index: 1,
            path: "other.tif".into(),
            job_id: Some(99),
            point: [3., 4.],
        },
    ];
    assert!(points::apply(std::slice::from_mut(&mut r), &updates).is_err());
    assert_eq!(r.metadata, before);
    assert!(points::set(&mut r, points::REFERENCE, [f64::NAN, 0.], false).is_err());
    assert_eq!(r.metadata, before);
    let mut other = r.clone();
    other.path = "other.tif".into();
    other.kickthemap_job_id = Some(99);
    other.epsg = Some(4326);
    let mut batch = vec![r.clone(), other];
    let before_batch = serde_json::to_value(&batch).unwrap();
    assert!(points::apply(&mut batch, &updates).is_err());
    assert_eq!(serde_json::to_value(&batch).unwrap(), before_batch);
    r.metadata["large"] = Value::String("x".repeat(2 * 1024 * 1024));
    assert!(points::validate_metadata(&r.metadata).is_err());
}
#[test]
fn both_template_planners_consume_raster_points_and_slot_metadata_keeps_unrelated_preferences() {
    let mut r = raster(&reference()["cases"][1]);
    r.path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/map-batch/input-0.tiff");
    points::set(&mut r, points::START, [155003., 463004.], false).unwrap();
    points::set(&mut r, points::REFERENCE, [155001., 463002.], false).unwrap();
    let merged = points::effective(
        &r,
        &json!({"template_reference_point":{"x":0,"y":0},"maaiveld":"custom"}),
    );
    assert_eq!(points::start_in(&merged), Some([155003., 463004.]));
    assert_eq!(merged[points::REFERENCE], r.metadata[points::REFERENCE]);
    assert_eq!(merged["maaiveld"], "custom");
    let data = reference();
    let dataset: Dataset = serde_json::from_value(data["cases"][1]["dataset"].clone()).unwrap();
    let mut state = sleufbase_rust::template_order::State {
        profiles: false,
        auto_bgt: false,
        ..Default::default()
    };
    state.sync(std::slice::from_ref(&r)).unwrap();
    state.rows[0].dataset = Some(dataset.source_path.clone());
    let inputs = Inputs {
        rasters: vec![r.clone()],
        datasets: vec![dataset.clone()],
        ..Default::default()
    };
    let req = sleufbase_rust::template_workflow::request(
        &state,
        &inputs,
        sleufbase_rust::RULES,
        &Control::default(),
    )
    .unwrap();
    let profile = req.slots[0].as_ref().unwrap().profile.as_ref().unwrap();
    assert_eq!(profile.forced_start, Some([155003., 463004.]));
    assert_eq!(
        profile.metadata[points::REFERENCE],
        r.metadata[points::REFERENCE]
    );
    let template =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/cadastral_template.dxf");
    let mut options = sleufbase_rust::template_project::Options {
        template,
        ..Default::default()
    };
    options.choices = sleufbase_rust::template_project::suggest(
        std::slice::from_ref(&dataset),
        std::slice::from_ref(&r),
        &[],
    );
    let req = sleufbase_rust::template_project::build(
        &options,
        &[dataset],
        &[r],
        &[],
        &[],
        None,
        sleufbase_rust::RULES,
        &Control::default(),
    )
    .unwrap();
    let profile = req.slots[0].as_ref().unwrap().profile.as_ref().unwrap();
    assert_eq!(profile.forced_start, Some([155003., 463004.]));
    assert_eq!(
        profile.metadata[points::REFERENCE],
        merged[points::REFERENCE]
    );
}
