#[path = "common/mtext_render.rs"]
#[allow(dead_code)] // This suite uses the shared font catalog, not the complex output summary.
mod common;
use serde_json::Value;
use sleufbase_rust::{
    mtext_plain,
    mtext_render::{EstimateRequest, Renderer, SimpleRequest},
};
use std::sync::atomic::AtomicBool;
fn reference() -> Value {
    serde_json::from_str(include_str!("fixtures/mtext-simple/reference.json")).unwrap()
}
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert!(
                (a - b).abs() <= 1e-9 + 64. * f64::EPSILON * a.abs().max(b.abs()),
                "{path}: {a} != {b}"
            );
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
                compare(&a[k], b, &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[test]
fn fast_decoder_matches_original_permissive_escapes_unknown_commands_and_unterminated_stacks() {
    let r = reference();
    let cases = r["plain"].as_array().unwrap();
    assert_eq!(cases.len(), 616);
    for (i, c) in cases.iter().enumerate() {
        assert_eq!(
            mtext_plain::decode(c["input"].as_str().unwrap(), &AtomicBool::new(false)).unwrap(),
            c["output"].as_str().unwrap(),
            "{i}"
        );
    }
}
#[test]
fn complete_simple_route_preserves_exact_wrap_decisions_line_alignment_matrices_and_glyphs() {
    let r = reference();
    let cases = r["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 189);
    let cancel = AtomicBool::new(false);
    let mut c = common::catalog(&common::root(), &cancel).unwrap();
    let mut changes = 0;
    for (i, case) in cases.iter().enumerate() {
        let request: SimpleRequest = serde_json::from_value(case["input"].clone()).unwrap();
        let output = Renderer::new(&mut c, &cancel)
            .render_simple(request)
            .unwrap_or_else(|e| panic!("{i}: {e}"));
        compare(
            &serde_json::to_value(output).unwrap(),
            &case["output"],
            &format!("case {i}"),
        );
        if case.get("legacy").is_some() {
            changes += 1;
            assert_eq!(case["legacy"]["error"], "IndexError");
        }
    }
    assert_eq!(changes, 21);
}
#[test]
fn long_single_lines_unknown_escapes_and_overflow_are_bounded_without_prefix_rescans() {
    let cancel = AtomicBool::new(false);
    let mut c = common::catalog(&common::root(), &cancel).unwrap();
    let r = reference();
    let mut request: SimpleRequest =
        serde_json::from_value(r["cases"][2]["input"].clone()).unwrap();
    request.content = "A ".repeat(60_000);
    request.width = 1e9;
    request.height = 1.;
    request.direction = None;
    let out = Renderer::new(&mut c, &cancel)
        .render_simple(request.clone())
        .unwrap();
    assert_eq!(out.lines.len(), 1);
    assert_eq!(out.lines[0].text.len(), 119_999);
    let text = r"\Zx".repeat(100_000);
    assert_eq!(mtext_plain::decode(&text, &cancel).unwrap(), text);
    assert!(mtext_plain::decode(&r"\S".repeat(10_000), &cancel)
        .err()
        .unwrap()
        .contains("gedecodeerd"));
    assert!(Renderer::new(&mut c, &AtomicBool::new(true))
        .render_simple(request.clone())
        .is_err());
    request.rotation = f64::INFINITY;
    assert!(Renderer::new(&mut c, &cancel)
        .render_simple(request)
        .is_err());
}
#[test]
fn original_fast_content_extent_estimates_match_whitespace_column_count_and_safety_factor() {
    let r = reference();
    let cases = r["estimates"].as_array().unwrap();
    assert_eq!(cases.len(), 324);
    let cancel = AtomicBool::new(false);
    let mut c = common::catalog(&common::root(), &cancel).unwrap();
    for (i, case) in cases.iter().enumerate() {
        let request: EstimateRequest = serde_json::from_value(case["input"].clone()).unwrap();
        let output = Renderer::new(&mut c, &cancel)
            .estimate(request)
            .unwrap_or_else(|e| panic!("estimate {i}: {e}"));
        compare(
            &serde_json::to_value(output).unwrap(),
            &case["output"],
            &format!("estimate {i}"),
        );
    }
}
