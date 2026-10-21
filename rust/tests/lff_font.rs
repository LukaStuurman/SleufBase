use serde_json::Value;
use sleufbase_rust::{
    dxf_fonts::Catalog,
    dxf_text_metrics::{Font, Text},
    lff_font::Program,
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lff-font")
}
fn reference() -> Value {
    serde_json::from_str(include_str!("fixtures/lff-font/reference.json")).unwrap()
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
            if path.ends_with(".corners") {
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
                                        <= 1e-11 + 64. * f64::EPSILON * x.abs().max(y.abs())
                                })),
                            "{path}: missing corner {p}"
                        );
                    }
                }
            } else {
                assert_eq!(a.len(), b.len(), "{path}");
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    compare(a, b, &format!("{path}[{i}]"));
                }
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
fn original_lff_parser_inheritance_snapshots_and_all_control_geometry_match() {
    let data = reference();
    let cancel = AtomicBool::new(false);
    let mut programs = std::collections::HashMap::new();
    for case in data["metadata"].as_array().unwrap() {
        let name = case["file"].as_str().unwrap();
        let p = Program::parse(&std::fs::read(directory().join(name)).unwrap(), &cancel).unwrap();
        assert_eq!(p.name, case["name"].as_str().unwrap());
        compare(
            &serde_json::json!(p.letter_spacing),
            &case["letter_spacing"],
            name,
        );
        compare(
            &serde_json::json!(p.word_spacing),
            &case["word_spacing"],
            name,
        );
        let mut codes = p.codes().collect::<Vec<_>>();
        codes.sort_unstable();
        assert_eq!(serde_json::json!(codes), case["codes"]);
        programs.insert(name.to_owned(), p);
    }
    for (i, case) in data["traces"].as_array().unwrap().iter().enumerate() {
        let file = case["file"].as_str().unwrap();
        let result = programs[file]
            .render(case["code"].as_u64().unwrap() as u32, &cancel)
            .unwrap();
        compare(
            &serde_json::to_value(result).unwrap(),
            &case["expected"],
            &format!("trace {i} {file}"),
        );
    }
}
#[test]
fn original_lff_advance_metrics_fallbacks_alignments_and_ocs_match() {
    let data = reference();
    let cancel = AtomicBool::new(false);
    let mut fonts = std::collections::HashMap::new();
    for (i, case) in data["extents"].as_array().unwrap().iter().enumerate() {
        let file = case["file"].as_str().unwrap();
        let font = fonts
            .entry(file.to_owned())
            .or_insert_with(|| Font::load(&directory().join(file), &cancel).unwrap());
        let text: Text = serde_json::from_value(case["text"].clone()).unwrap();
        let result = font.extent(&text, &cancel).unwrap();
        compare(
            &serde_json::to_value(result).unwrap(),
            &case["expected"],
            &format!("extent {i} {file}"),
        );
    }
}
#[test]
fn lff_fallback_uses_the_lff_priority_list_instead_of_shx_files() {
    let data = reference();
    assert_eq!(data["legacy_missing_lff_fallback"], "");
    assert_eq!(data["legacy_wrong_lff_fallback"], "txt.shx");
    let dir = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(false);
    let bytes = include_bytes!("fixtures/lff-font/polygons.lff");
    std::fs::write(
        dir.path().join("txt.shx"),
        include_bytes!("fixtures/stroke-font/fixture-1.0.shx"),
    )
    .unwrap();
    for name in ["simplex.lff", "iso.lff", "standard.lff"] {
        std::fs::write(dir.path().join(name), bytes).unwrap();
        let mut c = Catalog::from_dirs(&[dir.path().to_owned()], &cancel).unwrap();
        assert_eq!(
            c.selected_path("missing.lff"),
            Some(dir.path().join(name).as_path())
        );
        assert!(
            c.resolve("missing.lff", &cancel)
                .unwrap()
                .extent(
                    &Text {
                        content: "Axp".into(),
                        ..Default::default()
                    },
                    &cancel
                )
                .unwrap()
                .width
                > 0.
        );
        assert_eq!(
            c.selected_path("txt"),
            Some(dir.path().join("txt.shx").as_path())
        );
    }
}
#[test]
fn inheritance_shares_contours_and_has_bounded_depth_work_and_storage() {
    let cancel = AtomicBool::new(false);
    let mut text = String::from("[0041] A\n0,0;8,12;0,0\n[0042] B\n");
    for i in 0..2000 {
        text.push_str(&format!("{i},0;{i},1\n"));
    }
    text.push('\n');
    for i in 0..1000 {
        text.push_str(&format!("[{:04x}] child\nC0042\n", 0x400 + i));
    }
    let p = Program::parse(text.as_bytes(), &cancel).unwrap();
    assert!(
        p.source_bytes() < 512 * 1024,
        "shared font storage: {}",
        p.source_bytes()
    );
    let parent = p.render(66, &cancel).unwrap();
    let child = p.render(0x400 + 999, &cancel).unwrap();
    assert_eq!(parent.controls, child.controls);
    let mut deep = String::from("[0041]\n0,0;8,12\n");
    for i in 1..65 {
        deep.push_str(&format!("[{:04x}]\nC{:04x}\n", 65 + i, 64 + i));
    }
    assert!(Program::parse(deep.as_bytes(), &cancel)
        .unwrap_err()
        .contains("te diep"));
}
#[test]
fn ignored_contours_still_count_toward_the_shared_text_work_budget() {
    let mut text = String::from("[0041]\n0,0;8,12;0,0\n[0042]\n");
    text.push_str(&"one\n".repeat(100_000));
    let mut content = String::new();
    for i in 0..250 {
        text.push_str(&format!("[{:04x}]\nC0042\n", 0x500 + i));
        content.push(char::from_u32(0x500 + i).unwrap());
    }
    let cancel = AtomicBool::new(false);
    let p = Program::parse(text.as_bytes(), &cancel).unwrap();
    let mut font = sleufbase_rust::lff_font::Font::new(p, &cancel).unwrap();
    assert!(font
        .measure(&content, 1., 1., &cancel)
        .unwrap_err()
        .contains("werkbudget"));
}
#[test]
fn bad_lff_data_and_cancellation_fail_explicitly_without_nonfinite_bounds() {
    let cancel = AtomicBool::new(false);
    for s in [
        "",
        "[0041]\n",
        "#WordSpacing: NaN\n[0041]\n0,0;1,1",
        "[0041]\n0,0;inf,2",
        "[0041]\n0,0;1,2,ANaN",
    ] {
        assert!(Program::parse(s.as_bytes(), &cancel).is_err(), "{s}");
    }
    assert!(Program::parse(&[255], &cancel).is_err());
    let bytes = include_bytes!("fixtures/lff-font/polygons.lff");
    assert!(Program::parse(bytes, &AtomicBool::new(true)).is_err());
    let p = Program::parse(bytes, &cancel).unwrap();
    assert!(p.render(65, &AtomicBool::new(true)).is_err());
    let empty = Program::parse(b"[0042]\n0,0;1,1", &cancel).unwrap();
    assert!(sleufbase_rust::lff_font::Font::new(empty, &cancel).is_err());
}
