use sleufbase_rust::dxf_text_metrics::{self, Font, Text};
use std::{path::PathBuf, sync::atomic::AtomicBool};
#[test]
fn original_font_metrics_alignments_and_ocs_match_all_corners() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/dxf-text-metrics/reference.json")).unwrap();
    let cancel = AtomicBool::new(false);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/dxf-text-metrics/SleufBaseFixture.ttf");
    let mut ttf = Font::load(&path, &cancel).unwrap();
    let mut mono = Font::monospace();
    for (i, case) in data["cases"].as_array().unwrap().iter().enumerate() {
        let text: Text = serde_json::from_value(case["text"].clone()).unwrap();
        let font = if case["font"] == "mono" {
            &mut mono
        } else {
            &mut ttf
        };
        let actual = serde_json::to_value(font.extent(&text, &cancel).unwrap()).unwrap();
        fn compare(a: &serde_json::Value, b: &serde_json::Value, path: &str) {
            match (a, b) {
                (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
                    let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
                    assert!(
                        (a - b).abs() <= 1e-12 + 32. * f64::EPSILON * a.abs().max(b.abs()),
                        "{path}: {a} != {b}"
                    );
                }
                (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
                    if path.ends_with(".corners") {
                        // A zero-width text rectangle has duplicate corners;
                        // ezdxf's Path removes adjacent identical vertices.
                        for (from, to) in [(a, b), (b, a)] {
                            for p in from {
                                assert!(
                                    to.iter().any(|q| p
                                        .as_array()
                                        .unwrap()
                                        .iter()
                                        .zip(q.as_array().unwrap())
                                        .all(|(x, y)| {
                                            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
                                            (x - y).abs()
                                                <= 1e-12 + 32. * f64::EPSILON * x.abs().max(y.abs())
                                        })),
                                    "{path}: missing corner {p}"
                                );
                            }
                        }
                        return;
                    }
                    assert_eq!(a.len(), b.len(), "{path}");
                    for (i, (a, b)) in a.iter().zip(b).enumerate() {
                        compare(a, b, &format!("{path}[{i}]"));
                    }
                }
                (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
                    assert_eq!(a.len(), b.len(), "{path}");
                    for (k, a) in a {
                        compare(a, &b[k], &format!("{path}.{k}"));
                    }
                }
                _ => assert_eq!(a, b, "{path}"),
            }
        }
        compare(&actual, &case["expected"], &format!("case {i}"));
    }
    for case in data["plain"].as_array().unwrap() {
        assert_eq!(
            dxf_text_metrics::plain_text(case["value"].as_str().unwrap()),
            case["expected"].as_str().unwrap()
        );
    }
}
#[test]
fn invalid_text_or_font_and_cancellation_never_return_invalid_bounds() {
    let cancel = AtomicBool::new(false);
    let mut font = Font::monospace();
    for i in 0..7 {
        let mut text = Text {
            content: "A".into(),
            ..Default::default()
        };
        match i {
            0 => text.height = f64::NAN,
            1 => text.normal = [0.; 3],
            2 => text.insert[0] = f64::INFINITY,
            3 => text.align_point = Some([f64::NAN, 0., 0.]),
            4 => text.halign = 6,
            5 => text.height = 0.,
            _ => text.width = f64::INFINITY,
        }
        assert!(font.extent(&text, &cancel).is_err());
    }
    assert!(font
        .extent(
            &Text {
                content: "A".into(),
                ..Default::default()
            },
            &AtomicBool::new(true)
        )
        .is_err());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.ttf");
    std::fs::write(&path, b"bad font").unwrap();
    assert!(Font::load(&path, &cancel).is_err());
}
