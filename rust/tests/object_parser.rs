use serde::Deserialize;
use sleufbase_rust::{
    disciplines::{matches_keyword, matching_rule},
    parse::{parse_job_features, parse_job_features_with_cancel},
    Feature, Geometry, RULES,
};
use std::sync::atomic::AtomicBool;
#[derive(Deserialize)]
struct Case {
    name: String,
    input: String,
    expected: Vec<Feature>,
}
#[test]
fn filtering_truth_before_trimming_properties_repr_and_coordinates_match_python() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/object-parser/parser.json")).unwrap();
    assert_eq!(cases.len(), 107);
    for case in cases {
        let mut parsed = parse_job_features(&case.input, 1, "PS1")
            .unwrap_or_else(|e| panic!("{}: {e}", case.name));
        // The Python model has separate point/polyline sequences.
        parsed
            .features
            .sort_by_key(|f| matches!(f.geometry, Geometry::Polyline(_)));
        assert_eq!(parsed.features.len(), case.expected.len(), "{}", case.name);
        for (i, (actual, expected)) in parsed.features.iter().zip(case.expected).enumerate() {
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                serde_json::to_value(expected).unwrap(),
                "{}, feature {i}",
                case.name
            );
        }
    }
}
#[test]
fn unicode_wildcards_classes_invalid_ranges_and_literal_gating_match_python_decisions() {
    #[derive(Deserialize)]
    struct Row {
        pattern: String,
        matches: Vec<bool>,
    }
    #[derive(Deserialize)]
    struct Matrix {
        names: Vec<String>,
        rules: Vec<Row>,
    }
    let matrix: Matrix =
        serde_json::from_str(include_str!("fixtures/object-parser/rules.json")).unwrap();
    let mut count = 0;
    for row in matrix.rules {
        for (name, expected) in matrix.names.iter().zip(row.matches) {
            assert_eq!(
                matches_keyword(name, &row.pattern),
                expected,
                "pattern {:?}, name {:?}",
                row.pattern,
                name
            );
            count += 1;
        }
    }
    assert_eq!(count, 149112);
    assert_eq!(
        matching_rule(" mſ ", RULES).unwrap().label,
        "Middenspanning"
    );
    assert_eq!(
        matching_rule("\u{1c}LS\u{1f}", RULES).unwrap().label,
        "Laagspanning"
    );
}
#[test]
fn malformed_roots_precancellation_and_property_limits_never_return_partial_jobs() {
    for input in [
        "[]",
        "null",
        "{}",
        r#"{"features":null}"#,
        r#"{"features":{}}"#,
    ] {
        assert!(parse_job_features(input, 1, "PS1").is_err());
    }
    assert!(
        parse_job_features_with_cancel("not JSON", 1, "PS1", &AtomicBool::new(true))
            .unwrap_err()
            .contains("geannuleerd")
    );
    let source = serde_json::json!({"features":[{"geometry":{"type":"Point","coordinates":[1,2]},"properties":{"idName":"x".repeat(1024*1024+1)}}]});
    assert!(parse_job_features(&source.to_string(), 1, "PS1")
        .unwrap_err()
        .contains("1 MiB"));
}
