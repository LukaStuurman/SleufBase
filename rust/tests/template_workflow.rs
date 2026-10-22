use sleufbase_rust::{
    export_control::Control,
    export_worker::{Exporter, Inputs, Kind, Request},
    geotiff::GeoTiffInfo,
    template_export,
    template_order::{Row, State},
    template_workflow,
};
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};
fn inputs() -> Inputs {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/template-export/reference.json")).unwrap();
    let c = f
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "native-tiff")
        .unwrap();
    let request: template_export::Request = serde_json::from_value(c["request"].clone()).unwrap();
    let mut source = request.slots[0]
        .as_ref()
        .unwrap()
        .tiff_source
        .as_ref()
        .unwrap()
        .clone();
    source.path = root.join("..").join(source.path);
    let decoded = sleufbase_rust::raster::decode_full_with_cancel(
        &source.path,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    Inputs {
        datasets: request.datasets,
        rasters: vec![GeoTiffInfo {
            path: source.path,
            width: decoded.width,
            height: decoded.height,
            transform: source.transform,
            bounds: source
                .transform
                .bounds(decoded.width, decoded.height)
                .unwrap(),
            epsg: Some(28992),
            kickthemap_job_id: Some(1),
            kickthemap_job_title: "PS1".into(),
            kickthemap_job_sidecar: None,
        }],
        ..Default::default()
    }
}
fn state(inputs: &Inputs) -> State {
    let mut state = State {
        paired: false,
        project: "Werkproject Ø".into(),
        aerial: false,
        auto_bgt: false,
        ..Default::default()
    };
    state.sync(&inputs.rasters).unwrap();
    state.rows[0].number = "12".into();
    state
}
fn request(path: PathBuf, state: State) -> Request {
    Request {
        kind: Kind::Template,
        path,
        selection: sleufbase_rust::object_selection::Selection::all(0, 0),
        rules: sleufbase_rust::RULES.to_vec(),
        extra_layers: false,
        options: Default::default(),
        template: Some(Box::new(state)),
        maps: None,
        discipline_excel: false,
    }
}
fn wait(worker: &mut Exporter) -> sleufbase_rust::export_worker::Ready {
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        if let Some(r) = worker.poll() {
            return r;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn project_planning_preserves_empty_slots_excludes_rows_and_borrows_datasets_and_shared_maps() {
    let inputs = inputs();
    let mut state = state(&inputs);
    let mut second = state.rows[0].clone();
    second.variant = "AUTO".into();
    let mut excluded = second.clone();
    excluded.included = false;
    state.rows.extend([Row::default(), second, excluded]);
    let r = template_workflow::request(&state, &inputs, sleufbase_rust::RULES, &Control::default())
        .unwrap();
    assert_eq!(r.slots.len(), 3);
    assert!(r.slots[1].is_none());
    assert!(r.datasets.is_empty());
    let a = r.slots[0].as_ref().unwrap();
    let b = r.slots[2].as_ref().unwrap();
    assert_eq!((a.label.as_str(), b.label.as_str()), ("PS12", "PS12B"));
    assert_eq!(a.profile.as_ref().unwrap().dataset, 0);
    assert_eq!(a.report_dataset, Some(0));
    assert!(std::sync::Arc::ptr_eq(
        &a.map_source.as_ref().unwrap().overlays,
        &b.map_source.as_ref().unwrap().overlays
    ));
    let original = inputs.datasets[0].features.as_ptr();
    drop(r);
    assert_eq!(inputs.datasets[0].features.as_ptr(), original);
    state.rows[0].without_profile = true;
    let no_drawing =
        template_workflow::request(&state, &inputs, sleufbase_rust::RULES, &Control::default())
            .unwrap();
    let first = no_drawing.slots[0].as_ref().unwrap();
    assert!(first.profile.is_none());
    assert_eq!(
        first.report_dataset,
        Some(0),
        "Discipline counting must not follow the drawing switch"
    );
}
#[test]
fn real_native_worker_exports_the_bundled_template_and_returns_all_source_allocations() {
    let dir = tempfile::tempdir().unwrap();
    let mut inputs = inputs();
    inputs.comments.push(sleufbase_rust::project::Comment {
        x: 2.,
        y: 3.,
        text: "Grondwater\n grondwater\nStraße\nSTRASSE".into(),
    });
    let state = state(&inputs);
    let pointers = (
        inputs.datasets[0].features.as_ptr(),
        inputs.rasters.as_ptr(),
        inputs.comments.as_ptr(),
    );
    let output = dir.path().join("template.dxf");
    let mut worker = Exporter::new().unwrap();
    worker
        .start(inputs, request(output.clone(), state.clone()))
        .unwrap();
    let ready = wait(&mut worker);
    ready.result.unwrap();
    assert_eq!(
        (
            ready.inputs.datasets[0].features.as_ptr(),
            ready.inputs.rasters.as_ptr(),
            ready.inputs.comments.as_ptr()
        ),
        pointers
    );
    let doc = sleufbase_rust::dxf_document::Document::load(
        &output,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert!(doc.records().any(|r| r.kind() == "ATTRIB"
        && r.first(2) == Some("GEM-PROJECT")
        && r.first(1) == Some("Werkproject Ø")));
    let today = sleufbase_rust::template_metadata::Date::today()
        .unwrap()
        .to_string();
    assert!(doc.records().any(|r| r.kind() == "ATTRIB"
        && r.first(2) == Some("DATUM1")
        && r.first(1) == Some(today.as_str())));
    assert!(doc
        .records()
        .any(|r| r.first(1) == Some("Opmerkingen:\\PGrondwater\\PStraße")));
    let paths = doc
        .records()
        .filter(|r| r.kind() == "IMAGEDEF")
        .filter_map(|r| r.first(1))
        .filter(|p| p.contains("slot00001"))
        .collect::<Vec<_>>();
    assert_eq!(paths.len(), 2);
    for p in paths {
        assert!(std::path::Path::new(p).exists());
    }
    // Failed replacement after a previously completed export leaves its bytes.
    let previous = std::fs::read(&output).unwrap();
    let mut bad = state;
    bad.template = Some(dir.path().join("missing.dxf"));
    worker
        .start(ready.inputs, request(output.clone(), bad))
        .unwrap();
    let ready = wait(&mut worker);
    assert!(ready.result.is_err());
    assert_eq!(std::fs::read(&output).unwrap(), previous);
    assert_eq!(ready.inputs.rasters.as_ptr(), pointers.1);
}
#[test]
fn cancellation_and_panics_keep_owned_sources_and_never_publish_empty_projects() {
    for panic in [false, true] {
        let inputs = inputs();
        let state = state(&inputs);
        let original = inputs.rasters.as_ptr();
        let (tx, rx) = mpsc::sync_channel(1);
        let (gate, waiting) = mpsc::sync_channel(1);
        let mut worker = Exporter::with_processor(move |inputs, request, control| {
            tx.send(()).unwrap();
            waiting.recv_timeout(Duration::from_secs(10)).unwrap();
            assert!(!panic, "injected worker panic");
            sleufbase_rust::export_worker::process(inputs, request, control)
        })
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cancelled.dxf");
        worker.start(inputs, request(path.clone(), state)).unwrap();
        rx.recv_timeout(Duration::from_secs(10)).unwrap();
        if !panic {
            assert!(worker.cancel());
        }
        gate.send(()).unwrap();
        let ready = wait(&mut worker);
        assert!(ready.result.is_err());
        assert_eq!(ready.inputs.rasters.as_ptr(), original);
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
#[test]
fn automatic_bgt_is_shared_for_duplicate_rasters_and_preserves_its_constructed_geometry_contract() {
    let inputs = inputs();
    let mut state = state(&inputs);
    state.auto_bgt = true;
    state.rows.push(state.rows[0].clone());
    let mut calls = 0;
    let r = template_workflow::request_with_bgt(
        &state,
        &inputs,
        sleufbase_rust::RULES,
        &Control::default(),
        |_| {
            calls += 1;
            Ok(vec![sleufbase_rust::bgt_surface::Feature {
                physical_appearance: "Asfalt".into(),
                geometry: sleufbase_rust::bgt_surface::Geometry::Polygon(vec![vec![
                    [0., 0.],
                    [11., 0.],
                    [11., 6.],
                    [0., 6.],
                    [0., 0.],
                ]]),
            }])
        },
    )
    .unwrap();
    assert_eq!(calls, 1);
    let a = r.slots[0].as_ref().unwrap();
    let b = r.slots[1].as_ref().unwrap();
    assert!(a.surfaces_constructed && b.surfaces_constructed);
    assert!(std::sync::Arc::ptr_eq(
        a.surfaces.as_ref().unwrap(),
        b.surfaces.as_ref().unwrap()
    ));
    state.profiles = false;
    let r = template_workflow::request_with_bgt(
        &state,
        &inputs,
        sleufbase_rust::RULES,
        &Control::default(),
        |_| panic!("no BGT needed without profile drawing"),
    )
    .unwrap();
    assert!(!r.slots[0].as_ref().unwrap().profile.as_ref().unwrap().draw);
    assert!(r.slots[0].as_ref().unwrap().profile.is_some());
    state.profiles = true;
    assert!(template_workflow::request_with_bgt(
        &state,
        &inputs,
        sleufbase_rust::RULES,
        &Control::default(),
        |_| Err("injected BGT failure".into())
    )
    .err()
    .unwrap()
    .contains("BGT failure"));
}
#[test]
fn replacement_cannot_overwrite_measurements_cad_or_project_containers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.dxf");
    std::fs::write(&path, b"preserve original source").unwrap();
    let mut inputs = inputs();
    let state = state(&inputs);
    for kind in 0..3 {
        inputs.datasets[0].source_path = if kind == 0 {
            path.clone()
        } else {
            "unrelated.json".into()
        };
        inputs.protected_paths = if kind == 1 {
            vec![path.clone()]
        } else {
            vec![]
        };
        inputs.overlays = if kind == 2 {
            vec![sleufbase_rust::dxf_read::DxfOverlay {
                path: path.clone(),
                ..Default::default()
            }]
        } else {
            vec![]
        };
        assert!(template_workflow::export(
            &path,
            &state,
            &inputs,
            sleufbase_rust::RULES,
            &Control::default()
        )
        .unwrap_err()
        .contains("projectbronnen"));
        assert_eq!(std::fs::read(&path).unwrap(), b"preserve original source");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
