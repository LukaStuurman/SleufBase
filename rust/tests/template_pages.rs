use sleufbase_rust::{
    dxf_document::Document,
    template_geometry::{self, Entity, Layer, Paint, Shape},
    template_pages, template_slots,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};
fn source(name: &str) -> Document {
    Document::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/template-pages")
            .join(name),
        &AtomicBool::new(false),
    )
    .unwrap()
}
fn bytes(d: &Document) -> Vec<u8> {
    let mut b = Vec::new();
    d.write_to(&mut b, &AtomicBool::new(false)).unwrap();
    b
}
fn compare(a: &serde_json::Value, b: &serde_json::Value) {
    match (a, b) {
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            assert!((a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-8)
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                compare(a, b);
            }
        }
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (k, b) in b {
                compare(&a[k], b);
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
fn original_sheet_sequences_and_new_slot_handles_are_usable() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let cancel = AtomicBool::new(false);
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/template-pages/reference.json")).unwrap();
    for c in cases.as_array().unwrap() {
        let mut doc = Document::load(&root.join(c["path"].as_str().unwrap()), &cancel).unwrap();
        for step in c["steps"].as_array().unwrap() {
            let before = bytes(&doc);
            let result = template_pages::extend(
                &mut doc,
                step["required"].as_u64().unwrap() as usize,
                &cancel,
            );
            if step["error"] == true {
                assert!(result.is_err());
                assert_eq!(bytes(&doc), before);
                continue;
            }
            let result = result.unwrap();
            assert_eq!(
                result.added_pages,
                step["added_pages"].as_u64().unwrap() as usize
            );
            assert_eq!(
                result.slots.len(),
                step["expected"]["slots"].as_array().unwrap().len()
            );
            for (actual, expected) in result
                .slots
                .iter()
                .zip(step["expected"]["slots"].as_array().unwrap())
            {
                let mut actual = serde_json::to_value(actual).unwrap();
                let mut expected = expected.clone();
                for key in ["label_handle", "address_handle", "comments_handle"] {
                    if let Some(h) = actual[key].as_str() {
                        assert!(doc.by_handle(h).unwrap().is_some());
                    }
                    actual.as_object_mut().unwrap().remove(key);
                    expected.as_object_mut().unwrap().remove(key);
                }
                compare(&actual, &expected);
            }
            let _ = template_slots::detect(&doc, &cancel).unwrap();
        }
    }
}
#[test]
fn cancellation_after_work_and_worker_panic_leave_all_previous_bytes_intact() {
    let mut doc = source("one-page.dxf");
    let before = bytes(&doc);
    let cancel = AtomicBool::new(false);
    let result = template_pages::extend_with_progress(&mut doc, 25, &cancel, |done, _| {
        if done > 8 {
            cancel.store(true, Ordering::Relaxed);
        }
    });
    assert!(result.is_err());
    assert_eq!(bytes(&doc), before);
    cancel.store(false, Ordering::Relaxed);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        template_pages::extend_with_progress(&mut doc, 25, &cancel, |done, _| {
            if done > 8 {
                panic!("worker failed after copying a sheet");
            }
        })
    }));
    assert!(result.is_err());
    assert_eq!(bytes(&doc), before);
    assert_eq!(
        template_pages::extend(&mut doc, 25, &cancel)
            .unwrap()
            .slots
            .len(),
        32
    );
    assert!(template_pages::extend(&mut doc, 100_001, &cancel).is_err());
}
#[test]
fn structural_copy_materializes_staged_text_symbols_and_geometry() {
    let mut doc = source("one-page.dxf");
    let cancel = AtomicBool::new(false);
    let slots = template_slots::detect(&doc, &cancel).unwrap();
    doc.replace_text(&slots[0].label_handle, "Proefsleuf staged Ø")
        .unwrap();
    template_geometry::add(
        &mut doc,
        &[Layer {
            name: "Staged layer".into(),
            color: 3,
            true_color: None,
            lineweight: None,
        }],
        &[Entity {
            layer: "Staged layer".into(),
            paint: Paint::default(),
            shape: Shape::Circle {
                center: [1000., 1000.],
                radius: 1.,
            },
            layout: None,
        }],
        &cancel,
    )
    .unwrap();
    let before = bytes(&doc);
    let result = template_pages::extend(&mut doc, 8, &cancel).unwrap();
    assert_eq!(result.added_pages, 0);
    assert_eq!(bytes(&doc), before);
    let result = template_pages::extend(&mut doc, 9, &cancel).unwrap();
    assert!(doc
        .records()
        .any(|r| r.kind() == "LAYER" && r.first(2) == Some("Staged layer")));
    assert!(doc
        .records()
        .any(|r| r.kind() == "CIRCLE" && r.first(8) == Some("Staged layer")));
    assert!(result.slots[8..].iter().any(|s| doc
        .by_handle(&s.label_handle)
        .unwrap()
        .unwrap()
        .first(1)
        == Some("Proefsleuf staged Ø")));
}
