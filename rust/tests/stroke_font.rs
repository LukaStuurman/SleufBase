use serde_json::Value;
use sleufbase_rust::{
    dxf_text_metrics::{Font, Text},
    stroke_font::Program,
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stroke-font")
}
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert!(
                (a - b).abs() <= 1e-11 + 64. * f64::EPSILON * a.abs().max(b.abs()),
                "{path}: {a} != {b}"
            );
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}: array length");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, a) in a {
                compare(a, &b[k], &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[test]
fn original_binary_ascii_unicode_and_all_shape_instructions_match_control_geometry() {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/stroke-font/reference.json")).unwrap();
    let cancel = AtomicBool::new(false);
    let mut programs = std::collections::HashMap::new();
    for (i, case) in reference["traces"].as_array().unwrap().iter().enumerate() {
        let file = case["file"].as_str().unwrap();
        let program = programs.entry(file.to_owned()).or_insert_with(|| {
            let p = directory().join(file);
            Program::parse(
                &std::fs::read(&p).unwrap(),
                p.extension().unwrap().to_str().unwrap(),
                &cancel,
            )
            .unwrap()
        });
        let result = program
            .render(
                case["id"].as_u64().unwrap() as u32,
                case["stacked"].as_bool().unwrap(),
                &cancel,
            )
            .unwrap();
        compare(
            &serde_json::to_value(result).unwrap(),
            &case["expected"],
            &format!("trace {i} {file} glyph {}", case["id"]),
        );
    }
}
#[test]
fn original_shape_text_metrics_alignments_and_ocs_match() {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/stroke-font/reference.json")).unwrap();
    let cancel = AtomicBool::new(false);
    let mut fonts = std::collections::HashMap::new();
    for (i, case) in reference["extents"].as_array().unwrap().iter().enumerate() {
        let file = case["file"].as_str().unwrap();
        let font = fonts
            .entry(file.to_owned())
            .or_insert_with(|| Font::load(&directory().join(file), &cancel).unwrap());
        let text: Text = serde_json::from_value(case["text"].clone()).unwrap();
        let mut result = serde_json::to_value(font.extent(&text, &cancel).unwrap()).unwrap();
        // A zero-width rectangle has duplicate corners removed by ezdxf's Path.
        if result["corners"].is_array() && case["expected"]["corners"].is_array() {
            for (a, b) in [
                (&result["corners"], &case["expected"]["corners"]),
                (&case["expected"]["corners"], &result["corners"]),
            ] {
                for p in a.as_array().unwrap() {
                    assert!(
                        b.as_array().unwrap().iter().any(|q| p
                            .as_array()
                            .unwrap()
                            .iter()
                            .zip(q.as_array().unwrap())
                            .all(|(x, y)| {
                                let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
                                (x - y).abs() <= 1e-11 + 64. * f64::EPSILON * x.abs().max(y.abs())
                            })),
                        "extent {i} {file}: missing corner {p}"
                    );
                }
            }
            result["corners"] = case["expected"]["corners"].clone();
        }
        compare(&result, &case["expected"], &format!("extent {i} {file}"));
    }
}
#[test]
fn documented_subshape_pen_vertical_skip_and_legacy_header_fixes() {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/stroke-font/reference.json")).unwrap();
    let cancel = AtomicBool::new(false);
    let p = Program::parse(
        &std::fs::read(directory().join("corrections.shp")).unwrap(),
        "shp",
        &cancel,
    )
    .unwrap();
    for case in reference["corrections"].as_array().unwrap() {
        assert_ne!(case["legacy"], case["expected"]);
        compare(
            &serde_json::to_value(
                p.render(case["id"].as_u64().unwrap() as u32, false, &cancel)
                    .unwrap(),
            )
            .unwrap(),
            &case["expected"],
            "Autodesk correction",
        );
    }
    assert_eq!(reference["header12_legacy_error"], "FileStructureError");
    let font = Program::parse(
        &std::fs::read(directory().join("header12.shx")).unwrap(),
        "shx",
        &cancel,
    )
    .unwrap();
    assert_eq!(font.above, 12.);
    assert_eq!(font.render(65, false, &cancel).unwrap().end, [17., 0.]);
}
#[test]
fn malformed_instructions_cycles_stacks_and_cancel_fail_without_geometry() {
    let cancel = AtomicBool::new(false);
    for codes in [
        "7,65,0",
        "6,0",
        "5,5,5,5,5,0",
        "3,0,0",
        "8,128,0,0",
        "8,1,0",
        "15,0",
        "7,66,0",
        "11,300,0,0,1,0,0",
        "10,0,1,0",
        "10,256,1,0",
        "12,-128,1,1,0",
    ] {
        let bytes = format!("*0,4,test\n21,5,0,0\n*65,0,A\n{codes}\n");
        if let Ok(p) = Program::parse(bytes.as_bytes(), "shp", &cancel) {
            assert!(p.render(65, false, &cancel).is_err(), "{codes}");
        }
    }
    let bytes = std::fs::read(directory().join("unicode.shx")).unwrap();
    for end in [0, 1, 23, 24, 25, 30, 40, bytes.len() - 1] {
        assert!(
            Program::parse(&bytes[..end], "shx", &cancel).is_err(),
            "truncated {end}"
        );
    }
    assert!(Program::parse(&bytes, "shx", &AtomicBool::new(true)).is_err());
    let p = Program::parse(&bytes, "shx", &cancel).unwrap();
    assert!(p.render(65, false, &AtomicBool::new(true)).is_err());
    let mut f = Font::load(&directory().join("fixture.shp"), &cancel).unwrap();
    let result = f
        .extent(
            &Text {
                content: "A^Ix".into(),
                ..Default::default()
            },
            &cancel,
        )
        .unwrap();
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/stroke-font/reference.json")).unwrap();
    assert_eq!(reference["control_legacy_error"], "IndexError");
    compare(
        &serde_json::json!(result.width),
        &reference["control_expected_width"],
        "missing control uses the original cached space advance without first-use panic",
    );
}
#[test]
fn repeated_pen_up_subshapes_count_each_displacement_against_the_work_budget() {
    // Pen-up runs replace the current location without growing the point list.
    // A point budget alone therefore cannot bound repeated subshape work.
    let codes = format!(
        "*0,4,test\n21,5,0,0\n*65,0,A\n2,{}0\n*66,0,B\n9,{}0,0,0\n",
        "7,66,".repeat(1000),
        "1,0,".repeat(1500)
    );
    let cancel = AtomicBool::new(false);
    let p = Program::parse(codes.as_bytes(), "shp", &cancel).unwrap();
    assert!(p
        .render(65, false, &cancel)
        .unwrap_err()
        .contains("opdrachtbudget"));
}
