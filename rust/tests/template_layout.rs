use sleufbase_rust::{dxf_document::Document, template_layout, Bounds};
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn bytes(doc: &Document) -> Vec<u8> {
    let mut b = Vec::new();
    doc.write_to(&mut b, &AtomicBool::new(false)).unwrap();
    b
}
fn source() -> Document {
    Document::from_bytes(
        include_bytes!("fixtures/template-layout/sheets.dxf").to_vec(),
        &AtomicBool::new(false),
    )
    .unwrap()
}
#[test]
fn original_page_sequences_keep_empty_slots_and_repeated_changes() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/template-layout/reference.json")).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let cancel = AtomicBool::new(false);
    for case in fixture.as_array().unwrap() {
        let mut doc = Document::load(&root.join(case["path"].as_str().unwrap()), &cancel).unwrap();
        for step in case["steps"].as_array().unwrap() {
            let slots: Vec<Option<Bounds>> = serde_json::from_value(step["slots"].clone()).unwrap();
            let gap = step["label_gap"].as_f64().unwrap();
            let pages = template_layout::configure(&mut doc, &slots, gap, &cancel).unwrap();
            assert_eq!(serde_json::to_value(pages).unwrap(), step["pages"]);
            let before = bytes(&doc);
            template_layout::configure(&mut doc, &slots, gap, &cancel).unwrap();
            assert_eq!(bytes(&doc), before);
            let parsed = Document::from_bytes(before, &cancel).unwrap();
            for r in parsed.records().filter(|r| r.kind() == "ATTRIB") {
                assert_eq!(
                    r.first(1).unwrap(),
                    step["expected"]["attributes"][format!("{:X}", r.handle().unwrap())]
                        .as_str()
                        .unwrap()
                );
            }
        }
    }
}
#[test]
fn invalid_overflow_and_cancelled_requests_preserve_previous_page_edits() {
    let mut doc = source();
    let cancel = AtomicBool::new(false);
    let b = Bounds {
        min_x: 120000.,
        min_y: 450000.,
        max_x: 120010.,
        max_y: 450020.,
    };
    template_layout::configure(&mut doc, &[Some(b)], 18., &cancel).unwrap();
    let previous = bytes(&doc);
    assert!(template_layout::configure(&mut doc, &[Some(b)], 18., &AtomicBool::new(true)).is_err());
    for gap in [f64::NAN, f64::INFINITY, f64::MAX] {
        let mut slots = vec![Some(b); 8];
        slots.push(Some(b));
        assert!(template_layout::configure(&mut doc, &slots, gap, &cancel).is_err());
        assert_eq!(bytes(&doc), previous);
    }
    let mut inverted = b;
    inverted.min_x = inverted.max_x + 1.;
    assert!(template_layout::configure(&mut doc, &[Some(inverted)], 0., &cancel).is_err());
    assert_eq!(bytes(&doc), previous);
    let huge = Bounds {
        min_x: f64::MAX * 0.6,
        max_x: f64::MAX * 0.6,
        min_y: 0.,
        max_y: 1.,
    };
    let mut slots = vec![Some(b); 8];
    slots.push(Some(huge));
    assert!(template_layout::configure(&mut doc, &slots, 18., &cancel).is_err());
    assert_eq!(bytes(&doc), previous);
    assert!(template_layout::used_layout_names(100001).is_err());
}
