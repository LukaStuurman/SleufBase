use serde::Deserialize;
use serde_json::Value;
use sleufbase_rust::{
    profile::{self, Options, XY},
    Dataset, Feature, Geometry, Rule, Vertex,
};
use std::sync::atomic::AtomicBool;

#[derive(Deserialize)]
struct Fixture {
    rules: Vec<Rule>,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    features: Vec<Feature>,
    roads: Vec<Vec<XY>>,
    terrain: Vec<Vec<XY>>,
    forced_start: Option<XY>,
    marker_scale: f64,
    reverse: bool,
    expected: Value,
    projected: Value,
    surface: Value,
    visible_range: Value,
    rules: Option<Vec<Rule>>,
}
fn compare(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert!((a - b).abs() <= 2e-8, "{path}: {a} != {b}");
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, b) in b {
                compare(
                    a.get(k).unwrap_or_else(|| panic!("{path}.{k} missing")),
                    b,
                    &format!("{path}.{k}"),
                );
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
#[test]
fn original_patched_python_profiles() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("fixtures/profiles/reference.json")).unwrap();
    assert_eq!(fixture.cases.len(), 224);
    for case in fixture.cases {
        let dataset = Dataset {
            job_id: 1,
            title: case.name.clone(),
            source_path: "fixture.json".into(),
            features: case.features,
        };
        let result = profile::build(
            &dataset,
            case.rules.as_deref().unwrap_or(&fixture.rules),
            Options {
                roads: &case.roads,
                terrain_boundaries: &case.terrain,
                forced_start: case.forced_start,
                reverse: case.reverse,
                marker_scale: case.marker_scale,
            },
        )
        .unwrap();
        compare(
            &serde_json::to_value(&result).unwrap(),
            &case.expected,
            &case.name,
        );
        if let Some(p) = result {
            let projected: Vec<Vec<XY>> = dataset
                .features
                .iter()
                .filter_map(|f| match &f.geometry {
                    Geometry::Polyline(vertices) => Some(
                        vertices
                            .iter()
                            .filter_map(|v| p.project(*v, 3., 4., -2., 1.2, 0.8))
                            .collect(),
                    ),
                    _ => None,
                })
                .collect();
            compare(
                &serde_json::to_value(projected).unwrap(),
                &case.projected,
                &format!("{}.projected", case.name),
            );
            let surface: Vec<_> = [
                -1.,
                0.,
                p.axis_length * 0.37,
                p.axis_length,
                p.axis_length + 1.,
            ]
            .into_iter()
            .map(|c| p.terrain_z(c))
            .collect();
            compare(
                &serde_json::to_value(surface).unwrap(),
                &case.surface,
                &format!("{}.surface", case.name),
            );
            compare(
                &serde_json::to_value(p.visible_range()).unwrap(),
                &case.visible_range,
                &format!("{}.range", case.name),
            );
            let start = p.project(p.start, 0., 0., 0., 1., 1.).unwrap();
            assert!(start[0].abs() < 1e-12);
            assert_eq!(p.terrain_z(-1.), p.start.z.unwrap());
            assert!((p.terrain_z(p.axis_length + 1.) - p.end.z.unwrap()).abs() < 1e-12);
            let range = p.visible_range();
            assert!(range[0] <= 0. && range[1] >= p.axis_length);
        }
    }
}
fn dataset() -> Dataset {
    Dataset {
        job_id: 1,
        title: "test".into(),
        source_path: "test.json".into(),
        features: [0., 10., 5.]
            .into_iter()
            .map(|x| Feature {
                object_name: "point".into(),
                source_name: if x == 5. { "water" } else { "punt" }.into(),
                geometry: Geometry::Point(Vertex {
                    x,
                    y: 0.,
                    z: Some(10.),
                }),
                attributes: Default::default(),
            })
            .collect(),
    }
}
#[test]
fn invalid_inputs_are_errors_and_cancellation_is_observed() {
    let mut dataset = dataset();
    assert!(
        profile::build_with_cancel(&dataset, &[], Options::default(), &AtomicBool::new(true))
            .is_err()
    );
    assert!(profile::build(
        &dataset,
        &[],
        Options {
            marker_scale: f64::NAN,
            ..Options::default()
        }
    )
    .is_err());
    assert!(profile::build(
        &dataset,
        &[],
        Options {
            forced_start: Some([0., f64::INFINITY]),
            ..Options::default()
        }
    )
    .is_err());
    assert!(profile::build(
        &dataset,
        &[],
        Options {
            roads: &[vec![[0., f64::NAN], [1., 2.]]],
            ..Options::default()
        }
    )
    .is_err());
    dataset.features[0].geometry = Geometry::Point(Vertex {
        x: f64::NAN,
        y: 0.,
        z: Some(10.),
    });
    assert!(profile::build(&dataset, &[], Options::default()).is_err());
}
#[test]
fn finite_overflow_cannot_escape_as_nan_profile() {
    let mut dataset = dataset();
    dataset.features[0].geometry = Geometry::Point(Vertex {
        x: -1e308,
        y: 0.,
        z: Some(10.),
    });
    dataset.features[1].geometry = Geometry::Point(Vertex {
        x: 1e308,
        y: 0.,
        z: Some(10.),
    });
    assert!(profile::build(&dataset, &[], Options::default()).is_err());
}
