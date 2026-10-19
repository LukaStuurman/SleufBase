use sleufbase_rust::{
    bgt_surface::{Feature, Geometry, Index},
    profile::Profile,
    profile_surface, Vertex,
};
use std::sync::atomic::AtomicBool;
#[derive(serde::Deserialize)]
struct Case {
    name: String,
    start: [f64; 2],
    end: [f64; 2],
    axis_length: f64,
    features: Vec<Feature>,
    expected: Option<String>,
    texts: serde_json::Value,
}
fn profile(start: [f64; 2], end: [f64; 2], axis_length: f64) -> Profile<'static> {
    Profile {
        start: Vertex {
            x: start[0],
            y: start[1],
            z: Some(0.),
        },
        end: Vertex {
            x: end[0],
            y: end[1],
            z: Some(0.),
        },
        axis_dx: end[0] - start[0],
        axis_dy: end[1] - start[1],
        axis_length,
        reference_level: 0.,
        points: Vec::new(),
    }
}
#[test]
fn original_geos_intersection_votes_and_all_profile_probes() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/bgt-surface/reference.json")).unwrap();
    let cancel = AtomicBool::new(false);
    for c in cases {
        let index = Index::new(&c.features, &cancel).unwrap();
        assert_eq!(
            index.dominant(c.start, c.end, &cancel).unwrap(),
            c.expected.as_deref(),
            "{}",
            c.name
        );
        let p = profile(c.start, c.end, c.axis_length);
        assert_eq!(index.texts(&p, &cancel).unwrap(), c.texts, "{}", c.name);
        let metadata = serde_json::json!({"template_maaiveld_segments":{"middle":{"text":"INVULLEN","color":"#123456"}}});
        let expected = profile_surface::resolve(&metadata, &c.texts, true).unwrap();
        let actual = profile_surface::resolve_bgt(&metadata, &index, &p, true, &cancel).unwrap();
        for (a, e) in actual.iter().zip(expected.iter()) {
            assert_eq!(a.text, e.text);
            assert_eq!(a.rgb, e.rgb);
        }
    }
}
#[test]
fn invalid_input_limits_and_cancellation_fail_before_returning_texts() {
    let cancel = AtomicBool::new(false);
    let f = Feature {
        physical_appearance: "A".into(),
        geometry: Geometry::Point([f64::NAN, 0.]),
    };
    assert!(Index::new(&[f], &cancel).is_err());
    let features = [Feature {
        physical_appearance: "A".into(),
        geometry: Geometry::Polygon(vec![vec![[0., 0.], [10., 0.], [10., 10.], [0., 10.]]]),
    }];
    assert!(Index::new(&features, &AtomicBool::new(true)).is_err());
    let index = Index::new(&features, &cancel).unwrap();
    assert!(index
        .dominant([0., 0.], [1., 1.], &AtomicBool::new(true))
        .is_err());
    assert!(index
        .dominant([f64::INFINITY, 0.], [1., 1.], &cancel)
        .is_err());
    let mut g = Geometry::Point([0., 0.]);
    for _ in 0..18 {
        g = Geometry::Collection(vec![g]);
    }
    assert!(Index::new(
        &[Feature {
            physical_appearance: "A".into(),
            geometry: g
        }],
        &cancel
    )
    .is_err());
}

#[test]
fn many_polygon_crossings_are_swept_without_repeated_point_in_polygon_scans() {
    let count = 12000usize;
    let mut ring = vec![[0., -2.], [2. * count as f64, -2.], [2. * count as f64, 1.]];
    for i in (0..count).rev() {
        let x = 2. * i as f64;
        ring.extend([
            [x + 1.5, 1.],
            [x + 1.5, -1.],
            [x + 0.5, -1.],
            [x + 0.5, 1.],
            [x, 1.],
        ]);
    }
    let features = [
        Feature {
            physical_appearance: "Asfalt".into(),
            geometry: Geometry::Polygon(vec![ring]),
        },
        Feature {
            physical_appearance: "Gras".into(),
            geometry: Geometry::Polygon(vec![vec![
                [0., -1.],
                [count as f64 - 0.25, -1.],
                [count as f64 - 0.25, 1.],
                [0., 1.],
            ]]),
        },
    ];
    let cancel = AtomicBool::new(false);
    let index = Index::new(&features, &cancel).unwrap();
    assert_eq!(
        index
            .dominant([-1., 0.], [count as f64 * 2. + 1., 0.], &cancel)
            .unwrap(),
        Some("Asfalt")
    );
}
