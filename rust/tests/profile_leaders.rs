use sleufbase_rust::{
    dxf_document::Document,
    profile_leaders::{self, Entry, Options},
    template_geometry,
};
use std::sync::atomic::AtomicBool;
fn close(a: &serde_json::Value, b: &serde_json::Value) {
    match (a, b) {
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => assert!(
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-9,
            "{a} != {b}"
        ),
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                close(a, b);
            }
        }
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (k, v) in a {
                close(v, &b[k]);
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
fn all_original_active_label_layouts_match_including_crowding_and_81_entry_fast_path() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/profile-leaders/layout.json")).unwrap();
    let cancel = AtomicBool::new(false);
    for (index, case) in cases.as_array().unwrap().iter().enumerate() {
        let entries: Vec<Entry> = serde_json::from_value(case["entries"].clone()).unwrap();
        let opts = &case["options"];
        let hard: Vec<[f64; 4]> =
            serde_json::from_value(opts["hard_static_line_segments"].clone()).unwrap();
        let soft: Vec<[f64; 4]> =
            serde_json::from_value(opts["soft_static_line_segments"].clone()).unwrap();
        let text: Vec<[f64; 4]> =
            serde_json::from_value(opts["static_text_boxes"].clone()).unwrap();
        let placed = profile_leaders::solve(
            &entries,
            case["marker_scale"].as_f64().unwrap_or(0.02),
            Options {
                hard: &hard,
                soft: &soft,
                text: &text,
                min_x: opts["min_text_x"].as_f64(),
                max_x: opts["max_text_x"].as_f64(),
                avoid_collisions: opts["avoid_collisions"].as_bool().unwrap(),
            },
            &cancel,
        )
        .unwrap();
        let leaders=placed.iter().map(|p|{let e=&entries[p.entry_index];serde_json::json!({"description":e.description,"depth_text":e.depth_text,"leader_start":[e.anchor_x,e.leader_block_start_y],"text_insert":p.text_insert,"marker_scale":p.marker_scale,"color":e.leader_color})}).collect::<Vec<_>>();
        let lines=placed.iter().map(|p|{let c=profile_leaders::connector(&entries[p.entry_index]);serde_json::json!([[[c[0],c[1]],[c[2],c[3]]],{"dxfattribs":{"layer":"X-XX-AL-PROFIELBALK-SD","color":8,"lineweight":0}}])}).collect::<Vec<_>>();
        eprintln!("case {index}");
        close(
            &serde_json::json!({"leaders":leaders,"lines":lines}),
            &case["expected"],
        );
    }
}
#[test]
fn annotation_block_and_leader_links_work_without_reparse_and_reject_invalid_batches_atomically() {
    let cancel = AtomicBool::new(false);
    let source = include_bytes!("fixtures/profile-leaders/empty.dxf");
    let mut d = Document::from_bytes(source.to_vec(), &cancel).unwrap();
    profile_leaders::prepare(&mut d, &cancel).unwrap();
    profile_leaders::prepare(&mut d, &cancel).unwrap();
    let case: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/profile-leaders/layout.json")).unwrap();
    let entries: Vec<Entry> = serde_json::from_value(case[1]["entries"].clone()).unwrap();
    let placed = profile_leaders::solve(
        &entries,
        0.02,
        Options {
            avoid_collisions: false,
            ..Default::default()
        },
        &cancel,
    )
    .unwrap();
    let mut entities = Vec::new();
    profile_leaders::append(&mut entities, &entries, &placed, &cancel).unwrap();
    let handles = template_geometry::add(&mut d, &[], &entities, &cancel).unwrap();
    let mut out = Vec::new();
    d.write_to(&mut out, &cancel).unwrap();
    let written = Document::from_bytes(out.clone(), &cancel).unwrap();
    let leaders = written
        .records()
        .filter(|r| r.kind() == "LEADER")
        .collect::<Vec<_>>();
    assert_eq!(leaders.len(), 4);
    for (i, r) in leaders.iter().enumerate() {
        assert_eq!(r.first(340), Some(handles[i * 3].as_str()));
    }
    let invalid = entities.len() + 1;
    for invalid in [invalid, 2] {
        if let template_geometry::Shape::Leader { annotation, .. } = &mut entities[1].shape {
            *annotation = invalid;
        }
        assert!(template_geometry::add(&mut d, &[], &entities, &cancel).is_err());
        let mut after = Vec::new();
        d.write_to(&mut after, &cancel).unwrap();
        assert_eq!(out, after);
    }
}
#[test]
fn cancellation_nonfinite_and_oversized_layouts_publish_no_results() {
    let cancel = AtomicBool::new(false);
    let v: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/profile-leaders/layout.json")).unwrap();
    let mut entries: Vec<Entry> = serde_json::from_value(v[1]["entries"].clone()).unwrap();
    assert!(
        profile_leaders::solve(&entries, 0.02, Options::default(), &AtomicBool::new(true)).is_err()
    );
    entries[0].anchor_x = f64::NAN;
    assert!(profile_leaders::solve(&entries, 0.02, Options::default(), &cancel).is_err());
    assert!(profile_leaders::solve(&[], f64::INFINITY, Options::default(), &cancel).is_err());
    let excessive = vec![entries[1].clone(); 25_001];
    assert!(profile_leaders::solve(&excessive, 0.02, Options::default(), &cancel).is_err());
    let mut output = Vec::new();
    assert!(profile_leaders::append(&mut output, &entries, &[], &AtomicBool::new(true)).is_err());
    assert!(output.is_empty());
}
#[test]
fn all_original_sleeve_bundle_diameter_material_and_prefix_descriptions_match() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/profile-drawing/reference.json")).unwrap();
    for case in fixture["description_cases"].as_array().unwrap() {
        let f: sleufbase_rust::Feature = serde_json::from_value(case["feature"].clone()).unwrap();
        assert_eq!(
            profile_leaders::description(
                &f,
                case["base"].as_str().unwrap(),
                case["prefix"].as_str().unwrap()
            )
            .unwrap(),
            case["expected"].as_str().unwrap()
        );
    }
    let mut f: sleufbase_rust::Feature =
        serde_json::from_value(fixture["description_cases"][0]["feature"].clone()).unwrap();
    f.attributes[2] = "9".repeat(4096);
    assert!(profile_leaders::description(&f, "Water", "Ø").is_err());
}
fn measured_case() -> (
    sleufbase_rust::Dataset,
    sleufbase_rust::profile::Profile<'static>,
    sleufbase_rust::profile_drawing::Drawing,
) {
    use sleufbase_rust::{
        profile::{Profile, ProfilePoint},
        profile_drawing::{Band, Drawing, Layout},
        Dataset, Feature, Geometry, Vertex,
    };
    let position = Vertex {
        x: 3.,
        y: 0.,
        z: Some(8.),
    };
    let data = Dataset {
        job_id: 1,
        title: "PS1".into(),
        source_path: "fixture".into(),
        features: vec![Feature {
            object_name: "water".into(),
            source_name: "water".into(),
            geometry: Geometry::Point(position),
            attributes: ["PVC".into(), "110".into(), "dubbel mantelbuis 2,5".into()],
        }],
    };
    let profile = Profile {
        start: Vertex {
            x: 0.,
            y: 0.,
            z: Some(10.),
        },
        end: Vertex {
            x: 10.,
            y: 0.,
            z: Some(11.),
        },
        axis_dx: 10.,
        axis_dy: 0.,
        axis_length: 10.,
        reference_level: 8.,
        points: vec![ProfilePoint {
            feature_index: 0,
            position,
            chainage: 3.,
            point_z: 8.,
            layer_name: "Water".into(),
            color: 3,
            description: "Waterleiding".into(),
            is_endpoint: false,
        }],
    };
    let drawing = Drawing {
        layout: Layout {
            start_x: 10.,
            end_x: 20.,
            reference_y: 5.,
            title_y: 4.5,
            top_band_y: 4.,
            bottom_band_y: 3.,
            bottom_y: 2.,
            denominator: 20,
            scale: 1.,
        },
        entities: Vec::new(),
        bands: [(12.9362, 13.1), (13.0638, 13.2)]
            .into_iter()
            .map(|(x, placed_x)| Band {
                point_index: 0,
                feature_index: 0,
                anchor_x: x,
                placed_x,
                chainage: 3.,
                point_z: 8.,
                pinned: false,
                marker_scale: 0.11,
            })
            .collect(),
    };
    (data, profile, drawing)
}
#[test]
fn native_entry_generation_distinguishes_pipe_size_from_leader_scale_and_uses_shifted_bands() {
    let (data, mut profile, drawing) = measured_case();
    let cancel = AtomicBool::new(false);
    let entries = profile_leaders::entries(&profile, &data, &drawing, 0.02, "DN", &cancel).unwrap();
    assert_eq!(entries.len(), 2);
    for (e, b) in entries.iter().zip(&drawing.bands) {
        assert_eq!(e.anchor_x, b.anchor_x);
        assert_eq!(e.band_x, Some(b.placed_x));
        assert_eq!(e.leader_scale, Some(0.02));
        assert_eq!(e.leader_line_start_y, 5.);
        assert!((e.leader_block_start_y - 5.02).abs() < 1e-12);
        assert!((e.leader_top_y - 5.2).abs() < 1e-12);
        assert_eq!(e.depth_text, "Diepte: 2.00");
        assert_eq!(
            e.description,
            "Waterleiding mantelbuis Bundel: 2,5X DN110 PVC"
        );
    }
    profile.points[0].is_endpoint = true;
    assert!(
        profile_leaders::entries(&profile, &data, &drawing, 0.02, "DN", &cancel)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn composition_cancellation_and_stale_measurement_indices_leave_the_previous_drawing_intact() {
    let (data, profile, mut drawing) = measured_case();
    drawing.entities.push(template_geometry::Entity {
        layer: "0".into(),
        layout: None,
        paint: Default::default(),
        shape: template_geometry::Shape::Circle {
            center: [1., 2.],
            radius: 0.1,
        },
    });
    let obstacles = sleufbase_rust::profile_surface::Obstacles {
        hard: Vec::new(),
        soft: Vec::new(),
        text: Vec::new(),
    };
    let options = || profile_leaders::ProfileOptions {
        marker_scale: 0.02,
        diameter_prefix: "Ø",
        avoid_collisions: true,
    };
    let before = format!("{:?}", drawing.entities);
    assert!(profile_leaders::compose(
        &profile,
        &data,
        &mut drawing,
        &obstacles,
        options(),
        &AtomicBool::new(true)
    )
    .is_err());
    assert_eq!(before, format!("{:?}", drawing.entities));
    drawing.bands[1].feature_index = 999;
    assert!(profile_leaders::compose(
        &profile,
        &data,
        &mut drawing,
        &obstacles,
        options(),
        &AtomicBool::new(false)
    )
    .is_err());
    assert_eq!(before, format!("{:?}", drawing.entities));
    drawing.bands[1].feature_index = 0;
    drawing.bands[1].point_z += 1.;
    assert!(profile_leaders::compose(
        &profile,
        &data,
        &mut drawing,
        &obstacles,
        options(),
        &AtomicBool::new(false)
    )
    .is_err());
    assert_eq!(before, format!("{:?}", drawing.entities));
}
