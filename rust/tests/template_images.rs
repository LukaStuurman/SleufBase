use serde::Deserialize;
use sleufbase_rust::{
    dxf_document::Document,
    template_images::{self, Image},
    Bounds,
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
#[derive(Deserialize)]
struct Request {
    bounds: Bounds,
    width: u32,
    height: u32,
    inset: f64,
    preserve_aspect: bool,
}
#[derive(Deserialize)]
struct Expected {
    insert: [f64; 3],
    u_pixel: [f64; 3],
    v_pixel: [f64; 3],
}
#[derive(Deserialize)]
struct Case {
    requests: Vec<Request>,
    expected: Vec<Expected>,
}
#[test]
fn all_original_fits_and_stretches_match_python() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/template-images/reference.json")).unwrap();
    let mut total = 0;
    for case in cases {
        for (r, e) in case.requests.iter().zip(case.expected) {
            let bounds =
                template_images::fit(r.bounds, [r.width, r.height], r.inset, r.preserve_aspect)
                    .unwrap();
            let expected = [
                e.insert[0],
                e.insert[1],
                e.insert[0] + e.u_pixel[0] * r.width as f64 + e.v_pixel[0] * r.height as f64,
                e.insert[1] + e.u_pixel[1] * r.width as f64 + e.v_pixel[1] * r.height as f64,
            ];
            for (a, b) in [bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y]
                .iter()
                .zip(expected)
            {
                assert!((a - b).abs() < 1e-8, "{a} != {b}");
            }
            total += 1;
        }
    }
    assert_eq!(total, 91);
    let normal = Bounds {
        min_x: 0.,
        min_y: 0.,
        max_x: 2.,
        max_y: 1.,
    };
    assert!(template_images::fit(normal, [0, 1], 0., true).is_err());
    assert!(template_images::fit(normal, [1, 1], f64::NAN, true).is_err());
    assert!(template_images::fit(normal, [1, 1], 10., false).is_err());
    assert!(template_images::fit(
        Bounds {
            min_x: -1e308,
            max_x: 1e308,
            ..normal
        },
        [1, 1],
        0.,
        true
    )
    .is_err());
}
fn bytes(doc: &Document) -> Vec<u8> {
    let mut out = Vec::new();
    doc.write_to(&mut out, &AtomicBool::new(false)).unwrap();
    out
}
#[test]
fn failed_image_batches_preserve_edits_and_a_successful_batch_can_be_reloaded() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let cancel = AtomicBool::new(false);
    let mut doc = Document::load(
        &root.join("rust/tests/fixtures/templates/standard.dxf"),
        &cancel,
    )
    .unwrap();
    let slot = sleufbase_rust::template_slots::detect(&doc, &cancel)
        .unwrap()
        .remove(0);
    doc.replace_text(&slot.label_handle, "Gewijzigde titel")
        .unwrap();
    let before = bytes(&doc);
    let dir = tempfile::tempdir().unwrap();
    let path = std::fs::canonicalize(dir.path())
        .unwrap()
        .join("raster.png");
    image::RgbaImage::from_pixel(40, 20, image::Rgba([1, 2, 3, 255]))
        .save(&path)
        .unwrap();
    let mut spec = Image {
        path,
        bounds: slot.tiff_box,
        definition_name: "raster".into(),
        inset: 0.04,
        preserve_aspect: true,
        layout: None,
    };
    spec.layout = Some("missing layout".into());
    assert!(template_images::add_images(&mut doc, std::slice::from_ref(&spec), &cancel).is_err());
    assert_eq!(bytes(&doc), before);
    spec.layout = None;
    spec.definition_name = "bad\n0\nEOF".into();
    assert!(template_images::add_images(&mut doc, std::slice::from_ref(&spec), &cancel).is_err());
    assert_eq!(bytes(&doc), before);
    spec.definition_name = "raster".into();
    assert!(template_images::add_images(
        &mut doc,
        std::slice::from_ref(&spec),
        &AtomicBool::new(true)
    )
    .is_err());
    assert_eq!(bytes(&doc), before);
    spec.bounds.max_x = f64::NAN;
    assert!(template_images::add_images(&mut doc, std::slice::from_ref(&spec), &cancel).is_err());
    assert_eq!(bytes(&doc), before);
    spec.bounds = slot.tiff_box;
    template_images::add_images(&mut doc, std::slice::from_ref(&spec), &cancel).unwrap();
    let successful = bytes(&doc);
    let reloaded = Document::from_bytes(successful.clone(), &cancel).unwrap();
    assert_eq!(
        reloaded.records().filter(|r| r.kind() == "IMAGE").count(),
        1
    );
    assert_eq!(
        reloaded
            .by_handle(&slot.label_handle)
            .unwrap()
            .unwrap()
            .first(1),
        Some("Gewijzigde titel")
    );
    assert!(template_images::add_images(&mut doc, std::slice::from_ref(&spec), &cancel).is_err());
    assert_eq!(bytes(&doc), successful);
}
