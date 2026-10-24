use serde::Deserialize;
use serde_json::Value;
use sleufbase_rust::{dxf_document::Document, template_slots};
use std::{path::PathBuf, sync::atomic::AtomicBool};
#[derive(Deserialize)]
struct Case {
    name: String,
    path: String,
    slots: Value,
    error: bool,
}
fn compare(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-8,
            "{path}: {a} != {b}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, b) in b {
                compare(a.get(k).unwrap(), b, &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
#[test]
fn original_python_slots_on_actual_and_synthetic_templates() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/templates/reference.json")).unwrap();
    assert_eq!(cases.len(), 6);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    for case in cases {
        let doc = Document::load(&root.join(case.path), &AtomicBool::new(false)).unwrap();
        let result = template_slots::detect(&doc, &AtomicBool::new(false));
        if case.error {
            assert!(result.is_err(), "{}", case.name);
            continue;
        }
        let slots = result.unwrap();
        if case.name == "shipped" {
            assert_eq!(slots.len(), 32);
        } else {
            assert_eq!(slots.len(), 6);
        }
        compare(
            &serde_json::to_value(slots).unwrap(),
            &case.slots,
            &case.name,
        );
        assert!(template_slots::detect(&doc, &AtomicBool::new(true)).is_err());
    }
}
