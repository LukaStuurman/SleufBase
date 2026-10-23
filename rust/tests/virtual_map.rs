use serde_json::{json, Value};
use sleufbase_rust::{virtual_map as map, virtual_trench::VirtualTrench};
fn data() -> Value {
    serde_json::from_str(include_str!("fixtures/virtual-map/reference.json")).unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 2e-8, "{a} != {b}");
}
#[test]
fn original_move_reference_creation_and_reversed_nearest_hit_contracts_match() {
    let data = data();
    for c in data["cases"].as_array().unwrap() {
        assert_eq!(
            map::next_name(
                c["names"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| n.as_str().unwrap())
            ),
            c["next_name"].as_str().unwrap()
        );
        let mut t: VirtualTrench = serde_json::from_value(c["source"].clone()).unwrap();
        let picks = c["picks"].as_array().unwrap();
        let layers = [t.clone(), t.clone()];
        for (p, expected) in picks.iter().zip(c["expected_picks"].as_array().unwrap()) {
            let p = serde_json::from_value(p.clone()).unwrap();
            assert_eq!(
                map::pick(
                    &layers,
                    p,
                    c["mpp"].as_f64().unwrap(),
                    c["selection_tolerance"].as_f64().unwrap()
                ),
                expected.as_u64().map(|n| n as usize)
            );
        }
        let world = serde_json::from_value(c["reference_world"].clone()).unwrap();
        assert!(map::set_reference(&mut t, world).unwrap());
        for key in ["x", "y"] {
            near(
                t.metadata["template_reference_point"][key]
                    .as_f64()
                    .unwrap(),
                c["reference_raw"][key].as_f64().unwrap(),
            );
        }
        let p = map::reference(&t).unwrap().unwrap();
        for i in 0..2 {
            near(p[i], c["reference_display"][i].as_f64().unwrap());
        }
        let dx = c["target"][0].as_f64().unwrap() - c["anchor"][0].as_f64().unwrap();
        let dy = c["target"][1].as_f64().unwrap() - c["anchor"][1].as_f64().unwrap();
        let objects = t.objects.as_ptr();
        let boundary = t.boundary_3d.as_ptr();
        let source = t.source_path.clone();
        let metadata = t.metadata.clone();
        assert!(map::translate(&mut t, dx, dy).unwrap());
        assert_eq!(t.objects.as_ptr(), objects);
        assert_eq!(t.boundary_3d.as_ptr(), boundary);
        assert_eq!(t.source_path, source);
        assert_eq!(t.metadata, metadata);
        let expected: VirtualTrench = serde_json::from_value(c["expected"].clone()).unwrap();
        for (a, b) in [(&t.start, &expected.start), (&t.end, &expected.end)]
            .into_iter()
            .chain(
                t.objects
                    .iter()
                    .zip(&expected.objects)
                    .map(|(a, b)| (&a.position, &b.position)),
            )
        {
            near(a.x, b.x);
            near(a.y, b.y);
            assert_eq!(a.z, b.z);
        }
        let corrected: Vec<sleufbase_rust::Vertex> =
            serde_json::from_value(c["corrected_boundary"].clone()).unwrap();
        for (a, b) in t.boundary_3d.iter().zip(corrected) {
            near(a.x, b.x);
            near(a.y, b.y);
            assert_eq!(a.z, b.z);
        }
        let start = serde_json::from_value(c["creation"]["start"].clone()).unwrap();
        let close = serde_json::from_value(c["creation"]["too_close"].clone()).unwrap();
        let end = serde_json::from_value(c["creation"]["end"].clone()).unwrap();
        assert_eq!(c["creation"]["short_mode"], "placing_end");
        assert!(map::create("Test".into(), start, close, 0.6).is_err());
        let created = map::create("Test".into(), start, end, 0.6).unwrap();
        let original = &c["creation"]["created"][0];
        near(created.start.x, original[0][0].as_f64().unwrap());
        near(created.end.y, original[1][1].as_f64().unwrap());
    }
}
#[test]
fn atomic_invalid_moves_metadata_budgets_native_roundtrip_and_alignment_reset_remain_correct() {
    let mut t: VirtualTrench =
        serde_json::from_value(data()["cases"][0]["source"].clone()).unwrap();
    t.alignment = Some(sleufbase_rust::marxact_alignment::MarXactAlignment {
        auto_start: t.start,
        auto_end: t.end,
        auto_width: t.width_meters,
        rotation_degrees: 0.,
    });
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("measured.dxf");
    std::fs::write(&source, b"original source bytes").unwrap();
    t.source_path = Some(source.clone());
    let before = serde_json::to_vec(&t).unwrap();
    assert!(map::translate(&mut t, f64::NAN, 1.).is_err());
    assert_eq!(serde_json::to_vec(&t).unwrap(), before);
    assert!(!map::translate(&mut t, 0., 0.).unwrap());
    let auto = t.alignment.as_ref().unwrap().auto_start;
    assert!(map::translate(&mut t, 12., -7.).unwrap());
    assert_eq!(t.alignment.as_ref().unwrap().auto_start.x, auto.x + 12.);
    assert_eq!(t.alignment.as_ref().unwrap().auto_start.z, auto.z);
    assert_eq!(std::fs::read(source).unwrap(), b"original source bytes");
    assert!(map::set_reference(&mut t, [155000., 463000.]).unwrap());
    let metadata = t.metadata.clone();
    assert!(map::set_reference(&mut t, [f64::INFINITY, 0.]).is_err());
    assert_eq!(metadata, t.metadata);
    let project = sleufbase_rust::project::NativeProject {
        virtual_trenches: vec![t.clone()],
        ..Default::default()
    };
    let path = dir.path().join("native.sleufbase");
    sleufbase_rust::project::save_project(&path, &project).unwrap();
    let restored = sleufbase_rust::project::read_project(&path).unwrap();
    assert_eq!(restored.virtual_trenches[0].metadata, t.metadata);
    let text = "x".repeat(2 * 1024 * 1024);
    t.metadata = json!({"large":text});
    let before = t.metadata.clone();
    assert!(map::set_reference(&mut t, [1., 2.]).is_err());
    assert_eq!(t.metadata, before);
    let base: VirtualTrench = serde_json::from_value(data()["cases"][1]["source"].clone()).unwrap();
    let mut many = Vec::new();
    for _ in 0..9 {
        let mut t = base.clone();
        t.metadata = json!({"large":"x".repeat(1900000)});
        many.push(t);
    }
    assert!(sleufbase_rust::project::NativeProject {
        virtual_trenches: many,
        ..Default::default()
    }
    .validate()
    .is_err());
}
#[test]
fn selected_virtual_reference_and_notes_reach_the_real_template_planner_and_clear_actions() {
    use sleufbase_rust::{
        export_control::Control,
        export_worker::Inputs,
        template_project::{self, Choice, Options, Source},
    };
    let mut t: VirtualTrench =
        serde_json::from_value(data()["cases"][1]["source"].clone()).unwrap();
    let p = [t.start.x + 2., t.start.y + 1.];
    map::set_reference(&mut t, p).unwrap();
    let display = map::reference(&t).unwrap();
    let metadata = t.metadata.clone();
    let copy = sleufbase_rust::virtual_trench_editor::manual_copy(&t, "kopie").unwrap();
    assert_eq!(map::reference(&copy).unwrap(), display);
    assert_eq!(copy.metadata["keep"], metadata["keep"]);
    assert!(copy.metadata.get("rotation_degrees").is_none());
    let mut inputs = Inputs {
        trenches: vec![t],
        ..Default::default()
    };
    let opts = Options {
        template: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/cadastral_template.dxf"),
        paired: false,
        include_bgt: false,
        include_cadastral: false,
        include_cad: false,
        choices: vec![Choice {
            source: Source::Virtual(0),
            raster: None,
            include: true,
            title: "Virtueel".into(),
            profile: true,
            reverse: false,
            comments: String::new(),
            photos: Vec::new(),
        }],
        ..Default::default()
    };
    let request =
        template_project::build_from_inputs(&opts, &inputs, &[], &Control::default()).unwrap();
    let slot = request.slots[0].as_ref().unwrap();
    assert_eq!(slot.profile.as_ref().unwrap().metadata, metadata);
    assert_eq!(slot.map_source.as_ref().unwrap().reference, display);
    assert!(slot.comments.contains("Bronnotitie"));
    let mut hidden = opts;
    hidden.choices[0].profile = false;
    let request =
        template_project::build_from_inputs(&hidden, &inputs, &[], &Control::default()).unwrap();
    assert_eq!(
        request.slots[0]
            .as_ref()
            .unwrap()
            .map_source
            .as_ref()
            .unwrap()
            .reference,
        display
    );
    assert_eq!(
        sleufbase_rust::map_comments::clear_all(
            &mut inputs.comments,
            &mut inputs.rasters,
            &mut inputs.trenches
        )
        .unwrap(),
        1
    );
    assert_eq!(map::reference(&inputs.trenches[0]).unwrap(), display);
    assert!(map::clear_reference(&mut inputs.trenches[0]));
    assert!(!map::clear_reference(&mut inputs.trenches[0]));
}
