use sleufbase_rust::{
    dxf_document::Document,
    template_geometry::{self, Entity, Layer, Paint, Shape},
    template_images::{self, Image},
    Bounds,
};
use std::{collections::BTreeMap, sync::atomic::AtomicBool};
const SOURCE: &[u8] = include_bytes!("fixtures/template-geometry/blocks.dxf");
fn doc() -> Document {
    Document::from_bytes(SOURCE.to_vec(), &AtomicBool::new(false)).unwrap()
}
fn encoded(doc: &Document) -> Vec<u8> {
    let mut out = Vec::new();
    doc.write_to(&mut out, &AtomicBool::new(false)).unwrap();
    out
}
fn entity(shape: Shape) -> Entity {
    Entity {
        layer: "0".into(),
        paint: Paint::default(),
        shape,
        layout: None,
    }
}
fn line() -> Entity {
    entity(Shape::Polyline {
        vertices: vec![[0., 1.], [2., 3.]],
        closed: false,
    })
}
fn block() -> Entity {
    entity(Shape::Block {
        name: "Titel Ø".into(),
        insert: [10., 20.],
        scale: 0.02,
        attributes: BTreeMap::from([("NAME".into(), "Nieuw Ø 漢😀".into())]),
        attribute_placements: BTreeMap::new(),
    })
}
fn layer() -> Layer {
    Layer {
        name: "Native geometry".into(),
        color: 3,
        true_color: Some(0x123456),
        lineweight: None,
    }
}
#[test]
fn scaled_metrics_and_explicit_band_attribute_positions_match_original_setter() {
    let cancel = AtomicBool::new(false);
    let mut doc = doc();
    let metrics =
        template_geometry::block_attribute_metrics(&doc, "Titel Ø", " centered ", 0.02, &cancel)
            .unwrap()
            .unwrap();
    assert!((metrics.offset[0] - 0.08).abs() < 1e-10);
    assert!((metrics.offset[1] - 0.1).abs() < 1e-10);
    assert!((metrics.height - 0.11).abs() < 1e-10);
    assert!(
        template_geometry::block_attribute_metrics(&doc, "Titel Ø", "missing", 0.02, &cancel)
            .unwrap()
            .is_none()
    );
    assert!(template_geometry::block_attribute_metrics(
        &doc,
        "Titel Ø",
        "NAME",
        0.02,
        &AtomicBool::new(true)
    )
    .is_err());
    let mut request = block();
    if let Shape::Block {
        attribute_placements,
        ..
    } = &mut request.shape
    {
        attribute_placements.insert(
            "  nAme  ".into(),
            template_geometry::TextPlacement {
                insert: [8., 9.],
                height: 0.03,
                rotation: 90.,
            },
        );
    }
    template_geometry::add(&mut doc, &[], &[request], &cancel).unwrap();
    let written = Document::from_bytes(encoded(&doc), &cancel).unwrap();
    let attr = written
        .records()
        .find(|r| r.kind() == "ATTRIB" && r.first(2) == Some("NAME"))
        .unwrap();
    for (code, wanted) in [
        (10, 8.),
        (20, 9.),
        (30, 0.),
        (11, 8.),
        (21, 9.),
        (31, 0.),
        (40, 0.03),
        (50, 90.),
    ] {
        assert!((attr.first(code).unwrap().parse::<f64>().unwrap() - wanted).abs() < 1e-10);
    }
    assert_eq!(attr.first(1), Some("Nieuw Ø 漢😀"));
}
#[test]
fn bad_or_ambiguous_attribute_overrides_preserve_the_entire_previous_drawing() {
    let cancel = AtomicBool::new(false);
    let mut doc = doc();
    let before = encoded(&doc);
    for (height, rotation, duplicate) in
        [(0., 90., false), (0.03, f64::NAN, false), (0.03, 90., true)]
    {
        let mut request = block();
        if let Shape::Block {
            attribute_placements,
            ..
        } = &mut request.shape
        {
            let placement = template_geometry::TextPlacement {
                insert: [8., 9.],
                height,
                rotation,
            };
            attribute_placements.insert("NAME".into(), placement);
            if duplicate {
                attribute_placements.insert(" name ".into(), placement);
            }
        }
        assert!(template_geometry::add(&mut doc, &[layer()], &[line(), request], &cancel).is_err());
        assert_eq!(encoded(&doc), before);
    }
}
#[test]
fn block_attributes_are_attached_transformed_and_default_values_retained() {
    let cancel = AtomicBool::new(false);
    let mut doc = doc();
    let default = doc
        .records()
        .find(|r| r.kind() == "ATTDEF" && r.first(2) == Some("DEFAULT"))
        .unwrap()
        .handle()
        .unwrap();
    doc.replace_text(&format!("{default:X}"), "Edited default")
        .unwrap();
    let ids = template_geometry::add(&mut doc, &[], &[block()], &cancel).unwrap();
    let written = Document::from_bytes(encoded(&doc), &cancel).unwrap();
    let insert = written.by_handle(&ids[0]).unwrap().unwrap();
    assert_eq!(insert.first(66), Some("1"));
    let attrs: Vec<_> = written.records().filter(|r| r.kind() == "ATTRIB").collect();
    assert_eq!(attrs.len(), 4);
    let attr = attrs.iter().find(|r| r.first(2) == Some("NAME")).unwrap();
    assert_eq!(attr.first(1), Some("Nieuw Ø 漢😀"));
    assert!((attr.first(10).unwrap().parse::<f64>().unwrap() - 10.02).abs() < 1e-10);
    assert!((attr.first(20).unwrap().parse::<f64>().unwrap() - 20.04).abs() < 1e-10);
    assert_eq!(attr.first(30), Some("1"));
    assert!(attrs.iter().any(|r| r.first(1) == Some("Edited default")));
    let seqend = written.records().find(|r| r.kind() == "SEQEND").unwrap();
    assert_eq!(seqend.first(330), Some(ids[0].as_str()));
}
#[test]
fn invalid_geometry_batch_keeps_previous_text_and_resource_state() {
    let cancel = AtomicBool::new(false);
    let mut doc = doc();
    let id = doc
        .records()
        .find(|r| r.kind() == "ATTDEF")
        .unwrap()
        .handle()
        .unwrap();
    doc.replace_text(&format!("{id:X}"), "Previous edited definition")
        .unwrap();
    let before = encoded(&doc);
    let mut cases = vec![
        entity(Shape::Polyline {
            vertices: vec![[f64::NAN, 1.], [2., 3.]],
            closed: false,
        }),
        entity(Shape::Circle {
            center: [0., 0.],
            radius: 0.,
        }),
        entity(Shape::Circle {
            center: [f64::INFINITY, 0.],
            radius: 1.,
        }),
        entity(Shape::Block {
            name: "Missing".into(),
            insert: [0., 0.],
            scale: 1.,
            attributes: BTreeMap::new(),
            attribute_placements: BTreeMap::new(),
        }),
        entity(Shape::Block {
            name: "Titel Ø".into(),
            insert: [0., 0.],
            scale: -1.,
            attributes: BTreeMap::new(),
            attribute_placements: BTreeMap::new(),
        }),
        entity(Shape::Text {
            text: "x\n0\nEOF".into(),
            insert: [0., 0.],
            height: 1.,
            rotation: 0.,
            style: "Standard".into(),
            halign: 0,
            valign: 0,
        }),
        entity(Shape::Text {
            text: "ok".into(),
            insert: [0., 0.],
            height: 1.,
            rotation: 0.,
            style: "Missing".into(),
            halign: 0,
            valign: 0,
        }),
        entity(Shape::Text {
            text: "ok".into(),
            insert: [0., 0.],
            height: 1.,
            rotation: 0.,
            style: "Standard".into(),
            halign: 3,
            valign: 0,
        }),
    ];
    let mut missing = line();
    missing.layout = Some("Missing".into());
    cases.push(missing);
    let mut bad = line();
    bad.paint.color = Some(257);
    cases.push(bad);
    let mut bad = line();
    bad.paint.true_color = Some(0x1000000);
    cases.push(bad);
    let mut bad = line();
    bad.paint.lineweight = Some(27);
    cases.push(bad);
    for bad in cases {
        assert!(template_geometry::add(&mut doc, &[layer()], &[line(), bad], &cancel).is_err());
        assert_eq!(before, encoded(&doc));
    }
    let mut new = line();
    new.layer = layer().name;
    assert!(template_geometry::add(&mut doc, &[], &[new.clone()], &cancel).is_err());
    template_geometry::add(&mut doc, &[layer()], &[new], &cancel).unwrap();
    assert!(String::from_utf8(encoded(&doc))
        .unwrap()
        .contains("Previous edited definition"));
}
#[test]
fn images_and_repeated_geometry_share_layer_counts_and_unique_handles() {
    let cancel = AtomicBool::new(false);
    let directory = tempfile::tempdir().unwrap();
    let raster = directory.path().join("image.png");
    image::RgbaImage::new(2, 2).save(&raster).unwrap();
    for image_first in [false, true] {
        let mut doc = doc();
        let images = [Image {
            path: raster.clone(),
            bounds: Bounds {
                min_x: 0.,
                min_y: 0.,
                max_x: 1.,
                max_y: 1.,
            },
            definition_name: "mixed".into(),
            inset: 0.,
            preserve_aspect: true,
            layout: None,
        }];
        if image_first {
            template_images::add_images(&mut doc, &images, &cancel).unwrap();
        }
        template_geometry::add(&mut doc, &[layer()], &[line()], &cancel).unwrap();
        if !image_first {
            template_images::add_images(&mut doc, &images, &cancel).unwrap();
        }
        let mut alias = layer();
        alias.name = alias.name.to_uppercase();
        template_geometry::add(&mut doc, &[alias], &[block()], &cancel).unwrap();
        let written = Document::from_bytes(encoded(&doc), &cancel).unwrap();
        let count = written.records().filter(|r| r.kind() == "LAYER").count();
        let table = written
            .records()
            .find(|r| r.kind() == "TABLE" && r.first(2) == Some("LAYER"))
            .unwrap();
        assert_eq!(table.first(70).unwrap().parse::<usize>().unwrap(), count);
        assert_eq!(
            written
                .records()
                .filter(|r| r.kind() == "LAYER" && r.first(2) == Some("Native geometry"))
                .count(),
            1
        );
    }
}
#[test]
fn empty_cancelled_and_over_budget_requests_do_not_mutate_source() {
    let mut doc = doc();
    let cancel = AtomicBool::new(false);
    assert!(template_geometry::add(&mut doc, &[], &[], &cancel)
        .unwrap()
        .is_empty());
    assert_eq!(encoded(&doc), SOURCE);
    assert!(
        template_geometry::add(&mut doc, &[layer()], &[line()], &AtomicBool::new(true)).is_err()
    );
    let huge = entity(Shape::Polyline {
        vertices: vec![[0., 0.]; 1_000_001],
        closed: false,
    });
    assert!(template_geometry::add(&mut doc, &[], &[huge], &cancel).is_err());
    let invalid_layer = Layer {
        color: 256,
        ..layer()
    };
    assert!(template_geometry::add(&mut doc, &[invalid_layer], &[], &cancel).is_err());
    assert_eq!(encoded(&doc), SOURCE);
}
#[test]
fn unsupported_multiline_attribute_definitions_fail_without_partial_geometry() {
    let cancel = AtomicBool::new(false);
    let broken = std::str::from_utf8(SOURCE)
        .unwrap()
        .replace(" 71\n1\n", " 71\n4\n");
    let mut doc = Document::from_bytes(broken.as_bytes().to_vec(), &cancel).unwrap();
    assert!(template_geometry::add(&mut doc, &[layer()], &[line(), block()], &cancel).is_err());
    assert_eq!(encoded(&doc), broken.as_bytes());
    // A reference without automatic attributes still uses the original block.
    let mut reference = block();
    if let Shape::Block { attributes, .. } = &mut reference.shape {
        attributes.clear();
    }
    template_geometry::add(&mut doc, &[], &[reference], &cancel).unwrap();
}
