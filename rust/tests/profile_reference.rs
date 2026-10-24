use sleufbase_rust::{
    dxf_document::Document,
    profile,
    profile_drawing::{self, Templates},
    profile_reference, profile_surface, template_geometry, Bounds, Dataset, Feature, Geometry,
    Vertex, RULES,
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
#[test]
fn rd_rounding_and_reference_fallback_follow_python() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/profile-drawing/reference.json")).unwrap();
    for c in fixture["rd_values"].as_array().unwrap() {
        assert_eq!(
            profile_reference::rd_coordinate(c["value"].as_f64().unwrap()).unwrap(),
            c["text"].as_str().unwrap()
        );
    }
    let data = dataset();
    let profile = profile::build(&data, RULES, Default::default())
        .unwrap()
        .unwrap();
    let default = profile_reference::resolve(&profile, &serde_json::json!({})).unwrap();
    for metadata in [
        serde_json::json!({"template_reference_point":null}),
        serde_json::json!({"template_reference_point":{"x":"bad","y":2}}),
        serde_json::json!({"template_reference_point":{"x":1}}),
    ] {
        assert_eq!(
            profile_reference::resolve(&profile, &metadata).unwrap().raw,
            default.raw
        );
    }
    let value = profile_reference::resolve(
        &profile,
        &serde_json::json!({"template_reference_point":{"x":"1_0","y":"٢"}}),
    )
    .unwrap();
    assert_eq!(value.raw, [10., 2.]);
    assert_eq!(value.description, "RD 10, 2");
    assert!(profile_reference::resolve(
        &profile,
        &serde_json::json!({"template_reference_point":{"x":"NaN","y":0}})
    )
    .is_err());
    assert!(profile_reference::rd_coordinate(f64::INFINITY).is_err());
}
#[test]
fn display_rotation_validates_only_needed_coordinates() {
    let r = profile_reference::Reference {
        raw: [1., 2.],
        chainage: 0.,
        description: "RD 1, 2".into(),
    };
    assert_eq!(
        r.display(&serde_json::json!({}), [f64::NAN, 0.]).unwrap(),
        r.raw
    );
    let p = r
        .display(&serde_json::json!({"rotation_degrees":90}), [0., 0.])
        .unwrap();
    assert!((p[0] + 2.).abs() < 1e-12);
    assert!((p[1] - 1.).abs() < 1e-12);
    assert!(r
        .display(&serde_json::json!({"rotation_degrees":90}), [f64::NAN, 0.])
        .is_err());
    assert!(r
        .display(&serde_json::json!({"rotation_degrees":"NaN"}), [0., 0.])
        .is_err());
}
#[test]
fn indicator_composes_into_body_with_obstacles_and_real_dxf_links() {
    let cancel = AtomicBool::new(false);
    let data = dataset();
    let profile = profile::build(&data, RULES, Default::default())
        .unwrap()
        .unwrap();
    let reference = profile_reference::resolve(
        &profile,
        &serde_json::json!({"template_reference_point":{"x":4,"y":0}}),
    )
    .unwrap();
    let mut doc = Document::from_bytes(SOURCE.to_vec(), &cancel).unwrap();
    let templates = Templates::read(&doc, &cancel).unwrap();
    let tiff = Bounds {
        min_x: 25.,
        min_y: 0.,
        max_x: 27.,
        max_y: 2.,
    };
    let mut frame = profile_drawing::frame_and_bands(
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
        profile_drawing::Options {
            reference_chainage: reference.chainage,
            distance_description: &reference.description,
            ..Default::default()
        },
        &cancel,
    )
    .unwrap()
    .unwrap();
    assert!(profile_reference::draw(
        &profile,
        frame.layout,
        tiff.min_x,
        &reference,
        &AtomicBool::new(true)
    )
    .is_err());
    let fills =
        profile_surface::resolve(&serde_json::json!({}), &serde_json::json!({}), false).unwrap();
    let mut body = profile_surface::build(
        &profile,
        &data,
        &frame,
        tiff,
        &fills,
        Default::default(),
        &cancel,
    )
    .unwrap();
    let before = (body.obstacles.soft.len(), body.obstacles.text.len());
    let indicator =
        profile_reference::draw(&profile, frame.layout, tiff.min_x, &reference, &cancel).unwrap();
    assert_eq!(indicator.entities.len(), 5);
    indicator.append(&mut body);
    assert_eq!(
        (body.obstacles.soft.len(), body.obstacles.text.len()),
        (before.0 + 1, before.1 + 2)
    );
    let (layers, _) = profile_surface::compose(&mut frame, body).unwrap();
    profile_surface::prepare_marker(&mut doc, &cancel).unwrap();
    template_geometry::add(&mut doc, &layers, &frame.entities, &cancel).unwrap();
    let mut bytes = Vec::new();
    doc.write_to(&mut bytes, &cancel).unwrap();
    let parsed = Document::from_bytes(bytes, &cancel).unwrap();
    assert!(parsed
        .records()
        .any(|r| r.kind() == "TEXT" && r.first(1) == Some("Referentiepunt")));
}
