use sleufbase_rust::{dxf_document::Document, dxf_text_metrics::Font, template_extents::Index};
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn close(a: f64, b: f64) {
    assert!(
        (a - b).abs() <= 1e-12 + 32. * f64::EPSILON * a.abs().max(b.abs()),
        "{a} != {b}"
    );
}
#[test]
fn original_profile_geometry_and_production_selector_positions_match() {
    let cancel = AtomicBool::new(false);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let doc = Document::from_bytes(
        include_bytes!("fixtures/template-extents/input.dxf").to_vec(),
        &cancel,
    )
    .unwrap();
    let index = Index::new(&doc, &cancel).unwrap();
    let ttf = Font::load(
        &root.join("tests/fixtures/dxf-text-metrics/SleufBaseFixture.ttf"),
        &cancel,
    )
    .unwrap();
    let mut resolve = |name: &str| {
        if name == "*monospace" {
            Ok(Font::monospace())
        } else if name == "SleufBaseFixture.ttf" {
            Ok(ttf.clone())
        } else {
            Err(format!("Unexpected font {name}"))
        }
    };
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/template-extents/reference.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let ids: Vec<u64> = serde_json::from_value(case["handles"].clone()).unwrap();
        let bounds = index
            .bounds(&ids, &mut resolve, &cancel)
            .unwrap_or_else(|e| panic!("{}: {e}", case["name"]));
        let actual = serde_json::to_value(bounds).unwrap();
        if case["bounds"].is_null() {
            assert!(actual.is_null(), "{}", case["name"]);
        } else {
            for k in ["min", "max"] {
                for i in 0..3 {
                    close(
                        actual[k][i].as_f64().unwrap(),
                        case["bounds"][k][i].as_f64().unwrap(),
                    );
                }
            }
        }
        let selector = index
            .selector(&ids, &mut resolve, &cancel)
            .unwrap_or_else(|e| panic!("{}: {e}", case["name"]));
        match (selector, case["selector"].as_array()) {
            (Some(a), Some(b)) => {
                for i in 0..3 {
                    close(a[i], b[i].as_f64().unwrap());
                }
            }
            (None, None) => {}
            _ => panic!("{}: selector mismatch", case["name"]),
        }
    }
}
#[test]
fn unsupported_or_missing_geometry_and_cancelled_bounds_never_invent_a_selector() {
    let cancel = AtomicBool::new(false);
    let input = include_str!("fixtures/template-extents/input.dxf");
    let doc = Document::from_bytes(input.as_bytes().to_vec(), &cancel).unwrap();
    let index = Index::new(&doc, &cancel).unwrap();
    assert!(index
        .bounds(&[u64::MAX], &mut |_| Ok(Font::monospace()), &cancel)
        .is_err());
    let mut staged = Document::from_bytes(input.as_bytes().to_vec(), &cancel).unwrap();
    let handle = staged
        .records()
        .find(|r| r.kind() == "TEXT")
        .unwrap()
        .handle()
        .unwrap();
    staged
        .replace_text(&format!("{handle:X}"), "Nieuw profieltekstveld")
        .unwrap();
    assert!(Index::new(&staged, &cancel).is_err());
    assert!(index
        .bounds(&[], &mut |_| Ok(Font::monospace()), &AtomicBool::new(true))
        .is_err());
    let changed = input.replace("LWPOLYLINE", "UNSUPPORTED_ENTITY");
    let doc = Document::from_bytes(changed.into_bytes(), &cancel).unwrap();
    let id = doc
        .records()
        .find(|r| r.kind() == "UNSUPPORTED_ENTITY" && r.section() == Some("ENTITIES"))
        .unwrap()
        .handle()
        .unwrap();
    let index = Index::new(&doc, &cancel).unwrap();
    assert!(index
        .selector(&[id], &mut |_| Ok(Font::monospace()), &cancel)
        .is_err());
}
