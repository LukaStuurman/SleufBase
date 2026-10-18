use sleufbase_rust::{
    dxf_document::Document,
    profile::{self, Options as ProfileOptions},
    profile_drawing::{self, Band, Options, Templates},
    Bounds, Dataset, Feature, Geometry, Vertex, RULES,
};
use std::sync::atomic::AtomicBool;
const SOURCE: &[u8] = include_bytes!("fixtures/profile-drawing/blocks.dxf");
fn dataset() -> Dataset {
    Dataset {
        job_id: 1,
        title: "PS1".into(),
        source_path: "fixture.json".into(),
        features: [
            (0., 10., "punt"),
            (10., 11., "punt"),
            (3., 9., "water"),
            (3.01, 8., "water"),
        ]
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
fn band(anchor: f64, pinned: bool) -> Band {
    Band {
        point_index: 0,
        feature_index: 0,
        anchor_x: anchor,
        placed_x: anchor,
        chainage: 0.,
        point_z: 0.,
        pinned,
        marker_scale: 0.02,
    }
}
fn boxes() -> [Bounds; 2] {
    [
        Bounds {
            min_x: 0.,
            min_y: -1.,
            max_x: 30.,
            max_y: 3.,
        },
        Bounds {
            min_x: 25.,
            min_y: 0.,
            max_x: 27.,
            max_y: 2.,
        },
    ]
}
#[test]
fn original_band_distribution_names_and_decimal_text_match() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/profile-drawing/reference.json")).unwrap();
    for case in f["band_cases"].as_array().unwrap() {
        let mut bands: Vec<_> = case["anchors"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["pinned"].as_array().unwrap())
            .map(|(a, p)| band(a.as_f64().unwrap(), p.as_bool().unwrap()))
            .collect();
        profile_drawing::distribute(
            &mut bands,
            case["height"].as_f64().unwrap() * 0.92,
            case["min_x"].as_f64().unwrap(),
            case["max_x"].as_f64().unwrap(),
            &AtomicBool::new(false),
        )
        .unwrap();
        for (actual, expected) in bands.iter().zip(case["expected"].as_array().unwrap()) {
            assert!((actual.placed_x - expected.as_f64().unwrap()).abs() < 2e-8);
            if actual.pinned {
                assert_eq!(actual.anchor_x, actual.placed_x);
            }
        }
    }
    for c in f["labels"].as_array().unwrap() {
        assert_eq!(
            profile_drawing::names(c["label"].as_str().unwrap()).unwrap(),
            (
                c["number"].as_str().unwrap().into(),
                c["description"].as_str().unwrap().into()
            )
        );
    }
    for c in f["distances"].as_array().unwrap() {
        assert_eq!(
            profile_drawing::format_distance(c["value"].as_f64().unwrap()).unwrap(),
            c["text"].as_str().unwrap()
        );
    }
    assert!(profile_drawing::format_distance(f64::NAN).is_err());
}
#[test]
fn manual_auto_layout_and_frame_leave_source_measurements_unchanged() {
    let data = dataset();
    let before = serde_json::to_value(&data).unwrap();
    let cancel = AtomicBool::new(false);
    let profile = profile::build(&data, RULES, ProfileOptions::default())
        .unwrap()
        .unwrap();
    let doc = Document::from_bytes(SOURCE.to_vec(), &cancel).unwrap();
    let templates = Templates::read(&doc, &cancel).unwrap();
    for (manual, denominator) in [
        (Some(-100), Some(20)),
        (Some(999_999), Some(5000)),
        (None, None),
    ] {
        let drawing = profile_drawing::frame_and_bands(
            &profile,
            &data,
            boxes(),
            &templates,
            Options {
                manual_scale: manual,
                ..Default::default()
            },
            &cancel,
        )
        .unwrap()
        .unwrap();
        if let Some(denominator) = denominator {
            assert_eq!(drawing.layout.denominator, denominator);
        } else {
            assert_eq!(drawing.layout.denominator % 20, 0);
        }
        assert!(drawing.entities.len() >= 11 + 3 * drawing.bands.len());
        assert!(drawing
            .bands
            .iter()
            .filter(|b| b.pinned)
            .all(|b| b.anchor_x == b.placed_x));
        assert_eq!(serde_json::to_value(&data).unwrap(), before);
    }
}
#[test]
fn invalid_cancelled_and_extreme_placement_is_rejected_without_partial_band_moves() {
    let cancel = AtomicBool::new(false);
    let mut bands = vec![band(0., true), band(0., false), band(1., true)];
    for (width, low, high) in [
        (f64::NAN, 0., 1.),
        (0., 0., 1.),
        (1., 1., 0.),
        (f64::MAX, f64::MAX, f64::MAX),
    ] {
        assert!(profile_drawing::distribute(&mut bands, width, low, high, &cancel).is_err());
        assert_eq!(bands[1].placed_x, 0.);
    }
    assert!(profile_drawing::distribute(&mut bands, 0.03, 0., 1., &AtomicBool::new(true)).is_err());
    assert_eq!(bands[1].placed_x, 0.);
    let data = dataset();
    let mut profile = profile::build(&data, RULES, ProfileOptions::default())
        .unwrap()
        .unwrap();
    let doc = Document::from_bytes(SOURCE.to_vec(), &cancel).unwrap();
    let templates = Templates::read(&doc, &cancel).unwrap();
    assert!(profile_drawing::frame_and_bands(
        &profile,
        &data,
        boxes(),
        &templates,
        Options::default(),
        &AtomicBool::new(true)
    )
    .is_err());
    profile.points[0].feature_index = usize::MAX;
    assert!(profile_drawing::frame_and_bands(
        &profile,
        &data,
        boxes(),
        &templates,
        Options::default(),
        &cancel
    )
    .is_err());
    assert!(profile_drawing::names("PS1\nEOF").is_err());
}
