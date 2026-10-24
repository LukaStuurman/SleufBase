#[path = "common/font_resolution.rs"]
mod font_resolution;
use serde_json::Value;
use sleufbase_rust::{dxf_fonts::Catalog, font_error::Kind};
use std::sync::atomic::AtomicBool;
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert!(
                (a - b).abs() <= 1e-12 + 32. * f64::EPSILON * a.abs().max(b.abs()),
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
            for (k, b) in b {
                compare(&a[k], b, &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[test]
fn original_factory_routes_filesystem_changes_real_font_choices_and_all_corners_match() {
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/font-resolution/reference.json")).unwrap();
    let actual = font_resolution::run().unwrap();
    for (case, result) in expected["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(actual.as_array().unwrap())
    {
        assert_eq!(case["name"], result["name"]);
        for (i, (expected, actual)) in case["expected"]
            .as_array()
            .unwrap()
            .iter()
            .zip(result["actual"].as_array().unwrap())
            .enumerate()
        {
            if expected["status"] == "error" {
                assert_eq!(
                    actual["status"], "error",
                    "{} query {i}: {actual}",
                    case["name"]
                );
            } else {
                compare(actual, expected, &format!("{} query {i}", case["name"]));
            }
        }
    }
}
#[test]
fn font_limits_and_cancellation_are_typed_and_never_become_a_replacement_font() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(false);
    let font = include_bytes!("fixtures/dxf-text-metrics/SleufBaseFixture.ttf");
    std::fs::write(dir.path().join("Arial.ttf"), font).unwrap();
    let large = dir.path().join("large.ttf");
    std::fs::write(&large, font).unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&large)
        .unwrap()
        .set_len(65 * 1024 * 1024)
        .unwrap();
    let mut c = Catalog::from_dirs(&[dir.path().to_owned()], &cancel).unwrap();
    assert_eq!(c.selected_path("large.ttf"), Some(large.as_path()));
    assert!(c
        .resolve("large.ttf", &cancel)
        .err()
        .unwrap()
        .contains("te groot"));
    let program = format!(
        "*0,4,own\n21,5,0,0\n*65,0,A\n1,9,{}0,0,0\n",
        "1,0,".repeat(200_005)
    );
    let path = dir.path().join("point-limit.shp");
    std::fs::write(path, program.as_bytes()).unwrap();
    let p =
        sleufbase_rust::stroke_font::Program::parse(program.as_bytes(), "shp", &cancel).unwrap();
    assert_eq!(
        p.render(65, false, &cancel).err().unwrap().kind(),
        Kind::Limit
    );
    let mut c = Catalog::from_dirs(&[dir.path().to_owned()], &cancel).unwrap();
    assert!(c
        .resolve("point-limit.shp", &cancel)
        .err()
        .unwrap()
        .contains("grote SHX"));
    assert!(c
        .resolve("Arial.ttf", &AtomicBool::new(true))
        .err()
        .unwrap()
        .contains("geannuleerd"));
    assert_eq!(
        sleufbase_rust::stroke_font::Program::parse(b"", "shx", &AtomicBool::new(true))
            .err()
            .unwrap()
            .kind(),
        Kind::Cancelled
    );
}
#[test]
fn missing_files_can_recover_without_rebuilding_the_index() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(false);
    let path = dir.path().join("own.shx");
    let data = include_bytes!("fixtures/stroke-font/fixture-1.0.shx");
    std::fs::write(&path, data).unwrap();
    let mut c = Catalog::from_dirs(&[dir.path().to_owned()], &cancel).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        c.resolve("own.shx", &cancel).unwrap().kind(),
        "MonospaceFont"
    );
    assert_eq!(c.selected_path("own.shx"), None);
    std::fs::write(&path, data).unwrap();
    assert_eq!(
        c.resolve("own.shx", &cancel).unwrap().kind(),
        "ShapeFileFont"
    );
    assert_eq!(c.selected_path("own.shx"), Some(path.as_path()));
}
