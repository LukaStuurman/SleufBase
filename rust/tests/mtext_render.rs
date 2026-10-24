#[path = "common/mtext_render.rs"]
mod common;
use serde_json::Value;
use sleufbase_rust::{
    dxf_fonts::Catalog,
    mtext::FontFace,
    mtext_render::{Renderer, Request},
};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
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
                compare(
                    a.get(k).unwrap_or_else(|| panic!("{path}: missing {k}")),
                    b,
                    &format!("{path}.{k}"),
                );
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
fn reference() -> Value {
    serde_json::from_str(include_str!("fixtures/mtext-render/reference.json")).unwrap()
}
#[test]
fn complete_face_descriptors_match_original_filename_style_weight_italic_width_and_fallbacks() {
    let cancel = AtomicBool::new(false);
    let mut c = common::catalog(&common::root(), &cancel).unwrap();
    let r = reference();
    let queries = r["faces"].as_array().unwrap();
    assert_eq!(queries.len(), 167);
    for (i, q) in queries.iter().enumerate() {
        let face: FontFace = serde_json::from_value(q["input"].clone()).unwrap();
        c.resolve_face(&face, &cancel).unwrap();
        assert_eq!(
            c.selected_face_path(&face)
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy(),
            q["selected"].as_str().unwrap(),
            "{i}: {face:?}"
        );
    }
}
#[test]
fn composed_parser_font_measurement_layout_and_curves_match_original_with_explicit_font_fixes() {
    let cancel = AtomicBool::new(false);
    let mut c = common::catalog(&common::root(), &cancel).unwrap();
    let r = reference();
    let cases = r["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 123);
    let mut changed = 0;
    for (i, case) in cases.iter().enumerate() {
        let request: Request = serde_json::from_value(case["input"].clone()).unwrap();
        let result = Renderer::new(&mut c, &cancel).render(request);
        if case.get("error").is_some() {
            assert!(
                result.is_err(),
                "{i}: original rejected malformed tab stops"
            );
            continue;
        }
        let result = result.unwrap_or_else(|e| panic!("{i}: {e}"));
        compare(
            &common::summary(result),
            &case["output"],
            &format!("case {i}"),
        );
        if case.get("legacy").is_some() {
            changed += 1;
            assert_ne!(case["legacy"]["output"], case["output"]);
        }
    }
    assert_eq!(changed, 50);
}
#[test]
fn no_fonts_cancellation_invalid_input_and_output_limits_never_return_partial_results() {
    let cancel = AtomicBool::new(false);
    let mut c = Catalog::from_dirs(&[], &cancel).unwrap();
    let r = reference();
    let mut request: Request = serde_json::from_value(r["cases"][2]["input"].clone()).unwrap();
    let result = Renderer::new(&mut c, &cancel)
        .render(request.clone())
        .unwrap();
    assert!(result
        .words
        .iter()
        .all(|w| w.font_kind == "MonospaceFont" && w.selected.is_none()));
    assert!(Renderer::new(&mut c, &AtomicBool::new(true))
        .render(request.clone())
        .is_err());
    request.matrix[1][0] = f64::INFINITY;
    assert!(Renderer::new(&mut c, &cancel)
        .render(request.clone())
        .is_err());
    request.matrix[1][0] = 0.;
    request.width = 1e100;
    assert!(Renderer::new(&mut c, &cancel)
        .render(request.clone())
        .err()
        .unwrap()
        .contains("tabstopbudget"));
    request.width = 10.;
    request.content = "x".repeat(1_048_577);
    assert!(Renderer::new(&mut c, &cancel).render(request).is_err());
}
#[test]
fn bold_and_italic_use_the_requested_face_for_both_measurement_and_drawing() {
    let cancel = AtomicBool::new(false);
    let mut c = common::catalog(&common::root(), &cancel).unwrap();
    let r = reference();
    let case = &r["cases"][15];
    let request: Request = serde_json::from_value(case["input"].clone()).unwrap();
    let output = Renderer::new(&mut c, &cancel).render(request).unwrap();
    assert_eq!(
        output
            .words
            .iter()
            .map(|w| w.selected.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["Regular.ttf", "Bold.ttf", "Italic.ttf", "BoldItalic.ttf"]
    );
    assert_eq!(
        case["legacy"]["output"]["words"][1]["selected"],
        "Regular.ttf"
    );
    assert_ne!(
        case["legacy"]["output"]["draws"][2],
        case["output"]["draws"][2]
    );
}
#[test]
fn large_shared_font_names_and_tabs_are_resolved_once_across_paragraph_changes() {
    let cancel = AtomicBool::new(false);
    let mut c = common::catalog(&common::root(), &cancel).unwrap();
    let r = reference();
    let mut request: Request = serde_json::from_value(r["cases"][2]["input"].clone()).unwrap();
    request.width = 20_001.;
    request.initial.cap_height = 1.;
    Arc::make_mut(&mut request.initial.font_face).family =
        Arc::from("shared font family ".repeat(1500));
    let p = Arc::make_mut(&mut request.initial.paragraph);
    p.tab_stops = (1..=20_000)
        .rev()
        .map(|n| sleufbase_rust::mtext::TabStop::Number(n as f64))
        .collect::<Vec<_>>()
        .into();
    // Each indent command creates a new paragraph context but retains that
    // exact tab array. Re-expanding 20k stops 3k times exceeds the shared budget.
    request.content = (0..3000).map(|n| format!("\\pi{};A\\P", n % 2)).collect();
    let output = Renderer::new(&mut c, &cancel).render(request).unwrap();
    assert_eq!(output.words.len(), 3000);
    assert!(output
        .words
        .iter()
        .all(|w| Arc::ptr_eq(&w.context.font_face, &output.words[0].context.font_face)));
    assert!(output.words.iter().all(|w| Arc::ptr_eq(
        &w.context.paragraph.tab_stops,
        &output.words[0].context.paragraph.tab_stops
    )));
}
