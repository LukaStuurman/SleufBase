//! End-to-end nested DXF labels, measured boundaries and Python parse contracts.
use serde::Deserialize;
use sleufbase_rust::{
    dxf_read::{load_dxf, parse_dxf},
    marxact::{parse_leader_content, parse_overlay},
};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Leader {
    x: f64,
    y: f64,
    name: String,
    height: Option<f64>,
    raw_content: String,
}
#[derive(Deserialize)]
struct Object {
    x: f64,
    y: f64,
    z: Option<f64>,
    layer_name: String,
    name: String,
    block_name: String,
}
#[derive(Deserialize)]
struct Trench {
    name: String,
    polygon: Vec<[f64; 3]>,
    objects: Vec<Object>,
}
#[derive(Deserialize)]
struct Text {
    content: String,
    name: String,
    height: Option<f64>,
}
#[derive(Deserialize)]
struct Expected {
    leader_count: usize,
    insert_count: usize,
    assigned_insert_count: usize,
    leaders: Vec<Leader>,
    trenches: Vec<Trench>,
    text_cases: Vec<Text>,
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/marxact-cad")
}
fn expected() -> Expected {
    serde_json::from_str(include_str!("fixtures/marxact-cad/expected.json")).unwrap()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}

#[test]
fn nested_mleader_arrow_content_and_measurements_match_original_python() {
    let oracle = expected();
    let overlay = load_dxf(&root().join("nested-leaders.dxf")).unwrap();
    let leaders: Vec<_> = overlay
        .features
        .iter()
        .filter(|f| f.entity_type == "MULTILEADER")
        .collect();
    assert_eq!(leaders.len(), oracle.leaders.len());
    for (actual, want) in leaders.iter().zip(&oracle.leaders) {
        let tip = actual.points.first().unwrap();
        close(tip.x, want.x);
        close(tip.y, want.y);
        assert_eq!(actual.raw_text, want.raw_content);
        assert!(!actual.raw_text.contains("LEADER_LINE{"));
        assert_eq!(
            parse_leader_content(&actual.raw_text),
            (want.name.clone(), want.height)
        );
    }
    let result = parse_overlay(&overlay, &Default::default()).unwrap();
    assert_eq!(result.leader_count, oracle.leader_count);
    assert_eq!(result.insert_count, oracle.insert_count);
    assert_eq!(result.assigned_insert_count, oracle.assigned_insert_count);
    assert_eq!(result.trenches.len(), oracle.trenches.len());
    assert_eq!(
        result.skipped_boundary_count, 1,
        "open POLYLINE is never a trench boundary"
    );
    for (actual, want) in result.trenches.iter().zip(&oracle.trenches) {
        assert_eq!(actual.label, want.name);
        assert_eq!(actual.boundary_3d.len(), want.polygon.len());
        for (point, want) in actual.boundary_3d.iter().zip(&want.polygon) {
            close(point.x, want[0]);
            close(point.y, want[1]);
            close(point.z.unwrap(), want[2]);
        }
        assert_eq!(actual.objects.len(), want.objects.len());
        for (object, want) in actual.objects.iter().zip(&want.objects) {
            close(object.position.x, want.x);
            close(object.position.y, want.y);
            assert_eq!(object.position.z, want.z);
            assert_eq!(object.object_name, want.name);
            assert_eq!(object.attributes[1], want.layer_name);
            assert_eq!(object.attributes[2], want.block_name);
        }
    }
}

#[test]
fn label_text_uses_first_match_numeric_prefix_and_original_separators() {
    for case in expected().text_cases {
        assert_eq!(
            parse_leader_content(&case.content),
            (case.name, case.height),
            "{}",
            case.content
        );
    }
}

#[test]
fn missing_arrow_never_falls_back_to_text_location_and_malformed_scopes_fail() {
    let wrap = |groups: &str| {
        format!("0\nSECTION\n2\nENTITIES\n0\nMULTILEADER\n{groups}0\nENDSEC\n0\nEOF\n")
    };
    let without_arrow = "300\nCONTEXT_DATA{\n10\n123\n20\n456\n304\nName=PS1\n301\n}\n";
    assert!(parse_dxf(&wrap(without_arrow), Path::new("no-arrow.dxf"))
        .unwrap()
        .features
        .is_empty());
    for groups in [
        "300\nCONTEXT_DATA{\n302\nLEADER{\n301\n}\n",
        "300\nCONTEXT_DATA{\n304\nLEADER_LINE{\n301\n}\n",
        "300\nCONTEXT_DATA{\n302\nLEADER{\n304\nLEADER_LINE{\n10\n1\n305\n}\n303\n}\n301\n}\n",
        "300\nCONTEXT_DATA{\n304\nName=PS1\n",
    ] {
        assert!(
            parse_dxf(&wrap(groups), Path::new("malformed.dxf")).is_err(),
            "{groups}"
        );
    }
}

#[test]
fn older_saved_overlays_without_closed_flag_still_preserve_closed_boundaries() {
    let overlay = load_dxf(&root().join("nested-leaders.dxf")).unwrap();
    let mut value = serde_json::to_value(overlay).unwrap();
    for feature in value["features"].as_array_mut().unwrap() {
        feature.as_object_mut().unwrap().remove("closed");
    }
    // Old snapshots used a repeated first vertex only on header-closed polylines.
    let old = serde_json::from_value(value).unwrap();
    let result = parse_overlay(&old, &Default::default()).unwrap();
    assert_eq!(result.trenches.len(), 1);
}
