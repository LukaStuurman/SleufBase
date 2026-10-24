use sleufbase_rust::{
    dxf_document::Document,
    template_geometry::{self, Entity, Layer},
    template_images::{self, Image},
    template_prepare, Bounds,
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn bytes(doc: &Document) -> Vec<u8> {
    let mut v = Vec::new();
    doc.write_to(&mut v, &AtomicBool::new(false)).unwrap();
    v
}
fn doc() -> Document {
    Document::from_bytes(
        include_bytes!("fixtures/template-preparation/empty.dxf").to_vec(),
        &AtomicBool::new(false),
    )
    .unwrap()
}
#[test]
fn original_preparation_states_preserve_custom_values_and_revert_pending_colors() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/template-preparation/reference.json")).unwrap();
    let cancel = AtomicBool::new(false);
    for case in cases.as_array().unwrap() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut doc = Document::load(&root.join(case["path"].as_str().unwrap()), &cancel).unwrap();
        for (step_index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            if step["kind"] == "prepare" {
                let points: Vec<template_prepare::LayerColor> =
                    serde_json::from_value(step["points"].clone()).unwrap();
                template_prepare::profile(
                    &mut doc,
                    points.iter().map(|p| (p.name.as_str(), p.color)),
                    &cancel,
                )
                .unwrap();
                let before = bytes(&doc);
                template_prepare::profile(
                    &mut doc,
                    points.iter().map(|p| (p.name.as_str(), p.color)),
                    &cancel,
                )
                .unwrap();
                assert_eq!(bytes(&doc), before);
            } else {
                let layers: Vec<Layer> = serde_json::from_value(step["layers"].clone()).unwrap();
                let entities: Vec<Entity> =
                    serde_json::from_value(step["entities"].clone()).unwrap();
                template_geometry::add(&mut doc, &layers, &entities, &cancel).unwrap();
            }
            let written = Document::from_bytes(bytes(&doc), &cancel).unwrap();
            let layers = written
                .records()
                .filter(|r| r.kind() == "LAYER")
                .collect::<Vec<_>>();
            assert_eq!(
                layers.len(),
                step["expected"]["layers"].as_object().unwrap().len()
            );
            for layer in layers {
                let expected = &step["expected"]["layers"][layer.first(2).unwrap()];
                assert_eq!(
                    layer
                        .first(62)
                        .unwrap_or("7")
                        .trim()
                        .parse::<i32>()
                        .unwrap(),
                    expected["color"].as_i64().unwrap() as i32,
                    "case {} step {} layer {}",
                    case["name"],
                    step_index,
                    layer.first(2).unwrap()
                );
                assert_eq!(
                    layer
                        .first(370)
                        .unwrap_or("-3")
                        .trim()
                        .parse::<i32>()
                        .unwrap(),
                    expected["lineweight"].as_i64().unwrap() as i32
                );
            }
        }
    }
}
#[test]
fn cancellation_and_invalid_requests_do_not_commit_styles_layers_or_pending_replacements() {
    let cancel = AtomicBool::new(false);
    let mut d = doc();
    template_prepare::profile(&mut d, [("Early", 3)], &cancel).unwrap();
    let before = bytes(&d);
    assert!(template_prepare::profile(&mut d, [("Early", 5)], &AtomicBool::new(true)).is_err());
    assert_eq!(bytes(&d), before);
    for (name, color) in [("Early", 256), ("Bad\nName", 3)] {
        assert!(template_prepare::profile(&mut d, [("Early", 5), (name, color)], &cancel).is_err());
        assert_eq!(bytes(&d), before);
    }
    template_prepare::profile(&mut d, [("Early", 7)], &cancel).unwrap();
    template_prepare::profile(&mut d, [("Early", 3)], &cancel).unwrap();
    assert_eq!(bytes(&d), before);
}
#[test]
fn image_created_layer_can_be_updated_without_corrupting_its_batch_or_later_geometry() {
    let cancel = AtomicBool::new(false);
    let directory = tempfile::tempdir().unwrap();
    let raster = directory.path().join("image.png");
    image::RgbaImage::new(2, 2).save(&raster).unwrap();
    let mut d = doc();
    template_images::add_images(
        &mut d,
        &[Image {
            path: raster,
            bounds: Bounds {
                min_x: 0.,
                min_y: 0.,
                max_x: 2.,
                max_y: 2.,
            },
            definition_name: "prepare-image".into(),
            inset: 0.,
            preserve_aspect: true,
            layout: None,
        }],
        &cancel,
    )
    .unwrap();
    template_prepare::profile(
        &mut d,
        [("PROEFSLEUF_TEMPLATE_IMAGES", 5), ("New", 3)],
        &cancel,
    )
    .unwrap();
    template_prepare::profile(
        &mut d,
        [("PROEFSLEUF_TEMPLATE_IMAGES", 7), ("New", 5)],
        &cancel,
    )
    .unwrap();
    let written = Document::from_bytes(bytes(&d), &cancel).unwrap();
    let layer = written
        .records()
        .find(|r| r.kind() == "LAYER" && r.first(2) == Some("PROEFSLEUF_TEMPLATE_IMAGES"))
        .unwrap();
    assert_eq!(layer.first(62), Some("7"));
    assert_eq!(layer.first(370), Some("0"));
    assert_eq!(written.records().filter(|r| r.kind() == "IMAGE").count(), 1);
}
