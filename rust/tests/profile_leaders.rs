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
