use sleufbase_rust::{
    dxf_document::Document,
    profile,
    profile_drawing::{self, Templates},
    profile_surface::{self, Options},
    template_geometry::{self, Shape},
    Bounds, Dataset, Feature, Geometry, Vertex, RULES,
};
use std::sync::atomic::AtomicBool;
const SOURCE: &[u8] = include_bytes!("fixtures/profile-drawing/blocks.dxf");
fn dataset() -> Dataset {
    Dataset {
        job_id: 1,
        title: "PS1".into(),
        source_path: "fixture.json".into(),
        features: [(0., 10., "punt"), (10., 11., "punt"), (3., 9., "water")]
            .into_iter()
            .map(|(x, z, s)| Feature {
                object_name: s.into(),
                source_name: s.into(),
                geometry: Geometry::Point(Vertex {
                    x,
                    y: 0.,
                    z: Some(z),
                }),
                attributes: Default::default(),
            })
            .collect(),
    }
}
fn encoded(doc: &Document) -> Vec<u8> {
    let mut bytes = Vec::new();
    doc.write_to(&mut bytes, &AtomicBool::new(false)).unwrap();
    bytes
}
#[test]
fn original_fill_precedence_and_clipping_match_python() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/profile-drawing/reference.json")).unwrap();
    for c in f["fill_cases"].as_array().unwrap() {
        let fills = profile_surface::resolve(
            &c["metadata"],
            &c["automatic"],
            c["use_custom"].as_bool().unwrap(),
        )
        .unwrap();
        assert_eq!(serde_json::to_value(fills).unwrap(), c["expected"]);
    }
    for c in f["clip_cases"].as_array().unwrap() {
        let point = |key: &str| [c[key][0].as_f64().unwrap(), c[key][1].as_f64().unwrap()];
        let actual = profile_surface::clip_segment(point("a"), point("b"), -1., 1., 0.).unwrap();
        if c["expected"].is_null() {
            assert!(actual.is_none());
        } else {
            for (a, e) in actual.unwrap().iter().flatten().zip(
                c["expected"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|v| v.as_array().unwrap()),
            ) {
                assert!((a - e.as_f64().unwrap()).abs() < 1e-9);
            }
        }
    }
    assert_eq!(
        profile_surface::clip_segment([-2., -1.], [2., 1.], -1., 1., 0.).unwrap(),
        Some([[0., 0.], [1., 0.5]])
    );
    assert!(profile_surface::clip_segment([f64::NAN, 0.], [0., 0.], -1., 1., 0.).is_err());
}
#[test]
fn malformed_color_falls_back_and_injected_text_is_rejected() {
    let a = serde_json::json!({});
    let bad = serde_json::json!({"template_maaiveld_segments":{"middle":{"color":"#12_345","text":"Asfalt"}}});
    assert_eq!(
        profile_surface::resolve(&bad, &a, true).unwrap()[1].rgb,
        0x808080
    );
    let bad = serde_json::json!({"template_maaiveld_segments":{"middle":{"text":"A\n0\nEOF"}}});
    assert!(profile_surface::resolve(&bad, &a, true).is_err());
}
#[test]
fn casefold_equal_layer_rules_share_one_definition_and_keep_final_color() {
    let cancel = AtomicBool::new(false);
    let mut data = dataset();
    let mut gas = data.features[2].clone();
    gas.source_name = "gas".into();
    if let Geometry::Point(p) = &mut gas.geometry {
        p.x = 5.;
    }
    data.features.push(gas);
    let rules = [
        sleufbase_rust::Rule::new(vec!["water".into()], "PIPE".into(), 2, "Water".into()),
        sleufbase_rust::Rule::new(vec!["gas".into()], "pipe".into(), 3, "Gas".into()),
    ];
    let profile = profile::build(&data, &rules, Default::default())
        .unwrap()
        .unwrap();
    let doc = Document::from_bytes(SOURCE.to_vec(), &cancel).unwrap();
    let templates = Templates::read(&doc, &cancel).unwrap();
    let tiff = Bounds {
        min_x: 25.,
        min_y: 0.,
        max_x: 27.,
        max_y: 2.,
    };
    let frame = profile_drawing::frame_and_bands(
        &profile,
        &data,
        [
            Bounds {
                min_x: 0.,
                min_y: -1.,
                max_x: 30.,
                max_y: 3.,
            },
            tiff,
        ],
        &templates,
        Default::default(),
        &cancel,
    )
    .unwrap()
    .unwrap();
    let fills =
        profile_surface::resolve(&serde_json::json!({}), &serde_json::json!({}), false).unwrap();
    let body = profile_surface::build(
        &profile,
        &data,
        &frame,
        tiff,
        &fills,
        Default::default(),
        &cancel,
    )
    .unwrap();
    let layers: Vec<_> = body
        .layers
        .iter()
        .filter(|l| l.name.eq_ignore_ascii_case("pipe"))
        .collect();
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].name, "PIPE");
    assert_eq!(layers[0].color, 3);
}
#[test]
fn marker_block_creation_is_atomic_reusable_and_usable_by_later_batches() {
    let cancel = AtomicBool::new(false);
    let mut doc = Document::from_bytes(SOURCE.to_vec(), &cancel).unwrap();
    assert!(profile_surface::prepare_marker(&mut doc, &AtomicBool::new(true)).is_err());
    assert_eq!(encoded(&doc), SOURCE);
    profile_surface::prepare_marker(&mut doc, &cancel).unwrap();
    assert!(template_geometry::block_attribute_metrics(
        &doc,
        profile_surface::MARKER,
        "WAARDE",
        0.02,
        &cancel
    )
    .unwrap()
    .is_none());
    let once = encoded(&doc);
    profile_surface::prepare_marker(&mut doc, &cancel).unwrap();
    assert_eq!(encoded(&doc), once);
    let written = Document::from_bytes(once, &cancel).unwrap();
    assert_eq!(
        written
            .records()
            .filter(|r| r.kind() == "BLOCK" && r.first(2) == Some(profile_surface::MARKER))
            .count(),
        1
    );
    let count = written
        .records()
        .filter(|r| r.kind() == "BLOCK_RECORD")
        .count();
    let table = written
        .records()
        .find(|r| r.kind() == "TABLE" && r.first(2) == Some("BLOCK_RECORD"))
        .unwrap();
    assert_eq!(table.first(70).unwrap().parse::<usize>().unwrap(), count);
    let before = encoded(&doc);
    let invalid = template_geometry::CircleBlock {
        name: "Broken",
        center: [0., 0., 0.],
        radius: f64::NAN,
        layer: "0",
        paint: Default::default(),
    };
    assert!(template_geometry::ensure_circle_block(&mut doc, invalid, &cancel).is_err());
    assert_eq!(encoded(&doc), before);
}
#[test]
fn body_composes_with_frame_and_suppresses_markers_in_dynamic_leader_mode() {
    let cancel = AtomicBool::new(false);
    let data = dataset();
    let before = serde_json::to_value(&data).unwrap();
    let profile = profile::build(&data, RULES, Default::default())
        .unwrap()
        .unwrap();
    let mut doc = Document::from_bytes(SOURCE.to_vec(), &cancel).unwrap();
    let templates = Templates::read(&doc, &cancel).unwrap();
    let tiff = Bounds {
        min_x: 25.,
        min_y: 0.,
        max_x: 27.,
        max_y: 2.,
    };
    let boxes = [
        Bounds {
            min_x: 0.,
            min_y: -1.,
            max_x: 30.,
            max_y: 3.,
        },
        tiff,
    ];
    let mut frame = profile_drawing::frame_and_bands(
        &profile,
        &data,
        boxes,
        &templates,
        Default::default(),
        &cancel,
    )
    .unwrap()
    .unwrap();
    let fills =
        profile_surface::resolve(&serde_json::json!({}), &serde_json::json!({}), false).unwrap();
    assert!(profile_surface::build(
        &profile,
        &data,
        &frame,
        tiff,
        &fills,
        Options::default(),
        &AtomicBool::new(true)
    )
    .is_err());
    let dynamic = profile_surface::build(
        &profile,
        &data,
        &frame,
        tiff,
        &fills,
        Options {
            marker_in_leader: true,
            ..Default::default()
        },
        &cancel,
    )
    .unwrap();
    assert!(dynamic
        .markers
        .iter()
        .flatten()
        .all(|e| !matches!(&e.shape,Shape::Block{name,..}if name==profile_surface::MARKER)));
    let body = profile_surface::build(
        &profile,
        &data,
        &frame,
        tiff,
        &fills,
        Options::default(),
        &cancel,
    )
    .unwrap();
    assert_eq!(body.obstacles.soft.len(), 3);
    assert!(!body.obstacles.text.is_empty());
    let (layers, obstacles) = profile_surface::compose(&mut frame, body).unwrap();
    assert!(obstacles.hard.len() > 4);
    profile_surface::prepare_marker(&mut doc, &cancel).unwrap();
    template_geometry::add(&mut doc, &layers, &frame.entities, &cancel).unwrap();
    Document::from_bytes(encoded(&doc), &cancel).unwrap();
    assert_eq!(serde_json::to_value(&data).unwrap(), before);
}
