use sleufbase_rust::{
    dxf_document::Document, export_control::Control, template_branding, template_images,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};
fn source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/template-branding")
}
fn bytes(doc: &Document) -> Vec<u8> {
    let mut result = Vec::new();
    doc.write_to(&mut result, &AtomicBool::new(false)).unwrap();
    result
}
#[test]
fn original_logo_geometry_and_exact_contact_repair_are_preserved() {
    let root = source();
    let cancel = AtomicBool::new(false);
    let c: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/template-branding/reference.json")).unwrap();
    let mut doc = Document::load(&root.join("source.dxf"), &cancel).unwrap();
    assert_eq!(
        template_branding::restore_contact(&mut doc, &cancel).unwrap(),
        4
    );
    assert_eq!(
        template_branding::restore_contact(&mut doc, &cancel).unwrap(),
        0
    );
    let layouts: Vec<String> = serde_json::from_value(c["layouts"].clone()).unwrap();
    let images = template_branding::images(
        &doc,
        &layouts,
        Some(&root.join("tech.png")),
        Some(&root.join("client.jpg")),
        &cancel,
    )
    .unwrap();
    assert_eq!(images.len(), 10);
    for (actual, e) in images.iter().zip(c["expected"].as_array().unwrap()) {
        assert_eq!(actual.layout.as_deref(), e["layout"].as_str());
        assert_eq!(actual.definition_name, e["image"]["name"]);
        let size = image::image_dimensions(&actual.path).unwrap();
        let b = template_images::fit(
            actual.bounds,
            [size.0, size.1],
            actual.inset,
            actual.preserve_aspect,
        )
        .unwrap();
        let p = &e["image"];
        let expected = [
            p["insert"][0].as_f64().unwrap(),
            p["insert"][1].as_f64().unwrap(),
            p["u_pixel"][0].as_f64().unwrap() * size.0 as f64,
            p["v_pixel"][1].as_f64().unwrap() * size.1 as f64,
        ];
        for (a, b) in [b.min_x, b.min_y, b.max_x - b.min_x, b.max_y - b.min_y]
            .into_iter()
            .zip(expected)
        {
            assert!((a - b).abs() < 1e-10, "{a} != {b}");
        }
    }
    let before = bytes(&doc);
    cancel.store(true, Ordering::Relaxed);
    assert!(template_branding::restore_contact(&mut doc, &cancel).is_err());
    assert_eq!(bytes(&doc), before);
    assert!(template_branding::images(&doc, &layouts, None, None, &cancel).is_err());
}
#[test]
fn logo_normalization_keeps_alpha_resolution_and_rejects_invalid_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("wide.png");
    let image = image::RgbaImage::from_fn(2001, 2, |x, y| {
        image::Rgba([(x % 256) as u8, y as u8, 73, (x * 3 % 256) as u8])
    });
    image.save(&source).unwrap();
    let dest = dir.path().join("normalized.png");
    template_branding::prepare_logo(&source, &dest, &Control::default()).unwrap();
    assert_eq!(image::open(&dest).unwrap().into_rgba8(), image);
    let bad = dir.path().join("corrupt.png");
    std::fs::write(&bad, b"not an image").unwrap();
    let old = std::fs::read(&dest).unwrap();
    assert!(template_branding::prepare_logo(&bad, &dest, &Control::default()).is_err());
    assert_eq!(std::fs::read(&dest).unwrap(), old);
}

fn pipeline_request() -> sleufbase_rust::template_export::Request {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/template-export/reference.json")).unwrap();
    let mut slot = cases[0]["request"]["slots"][0].clone();
    slot["profile"] = serde_json::Value::Null;
    slot["tiff_png"] = serde_json::json!(root.join("tests/fixtures/template-export/tiff.png"));
    slot["map_png"] = serde_json::json!(root.join("tests/fixtures/template-export/map.png"));
    serde_json::from_value(serde_json::json!({"template":root.join("../assets/cadastral_template.dxf"),"slots":vec![slot;9],"rules":[],
        "settings":{"export_date":[2026,10,23],"logos":{"techbase":source().join("tech.png"),"client":source().join("client.jpg")}}})).unwrap()
}
fn files(root: &std::path::Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut result = Vec::new();
    for e in std::fs::read_dir(root).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            result.extend(files(&p));
        } else {
            result.push((p.clone(), std::fs::read(p).unwrap()));
        }
    }
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}
#[test]
fn complete_two_sheet_export_protects_logos_previous_drawing_and_assets() {
    use sleufbase_rust::template_export;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("complete.dxf");
    let mut request = pipeline_request();
    let result = template_export::export(&path, &request, &Control::default()).unwrap();
    assert_eq!(result.filled_slots, 9);
    let doc = Document::load(&path, &AtomicBool::new(false)).unwrap();
    let paper_logos = doc
        .records()
        .filter(|r| r.kind() == "IMAGE" && r.first(67) == Some("1"))
        .count();
    assert!(paper_logos >= 4);
    let before = files(dir.path());
    let control = Control::default();
    let mut reached = false;
    assert!(
        template_export::export_with_progress(&path, &request, &control, |stage| {
            if stage == "Export definitief opslaan" {
                reached = true;
                assert!(control.cancel());
            }
        })
        .is_err()
    );
    assert!(reached);
    assert_eq!(files(dir.path()), before);
    let bad = dir.path().join("corrupt.png");
    std::fs::write(&bad, b"invalid selected logo").unwrap();
    request.settings.logos.client = Some(bad);
    let before = files(dir.path());
    assert!(template_export::export(&path, &request, &Control::default()).is_err());
    assert_eq!(files(dir.path()), before);
    let selected = request.settings.logos.techbase.as_ref().unwrap();
    let bytes = std::fs::read(selected).unwrap();
    assert!(template_export::export(selected, &request, &Control::default()).is_err());
    assert_eq!(std::fs::read(selected).unwrap(), bytes);
}
