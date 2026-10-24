use serde_json::{json, Value};
use sleufbase_rust::{geotiff::GeoTiffInfo, geotiff_rotation as rotation};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/raster-rotation/reference.json")).unwrap()
}
fn close(a: &Value, b: &Value) {
    close_with(a, b, 2e-8)
}
fn close_with(a: &Value, b: &Value, tolerance: f64) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < tolerance,
            "{a} != {b}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                close_with(a, b, tolerance)
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (k, v) in a {
                close_with(v, &b[k], tolerance)
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
fn original_transform_chains_overlay_ring_hits_and_real_pointer_handler_angles_match() {
    let reference = fixture();
    let exact: rotation::Overlay =
        serde_json::from_value(reference["precise_hit_overlay"].clone()).unwrap();
    for q in reference["precise_hits"].as_array().unwrap() {
        assert_eq!(
            rotation::hit(
                &exact,
                serde_json::from_value(q["pointer"].clone()).unwrap()
            ),
            q["expected"].as_bool().unwrap()
        );
    }
    for c in fixture()["cases"].as_array().unwrap() {
        let mut raster: GeoTiffInfo = serde_json::from_value(c["input"].clone()).unwrap();
        let viewport = sleufbase_rust::geo::ViewportTransform {
            bounds: serde_json::from_value(c["viewport"]["bounds"].clone()).unwrap(),
            width_px: c["viewport"]["width_px"].as_u64().unwrap() as u32,
            height_px: c["viewport"]["height_px"].as_u64().unwrap() as u32,
        };
        for step in c["steps"].as_array().unwrap() {
            rotation::set(&mut raster, step["angle"].as_f64().unwrap()).unwrap();
            close(&json!(raster), &step["expected"]);
            let overlay = rotation::overlay(&raster, |p| {
                let (x, y) = viewport.world_to_screen(p[0], p[1]).unwrap();
                [x, y]
            })
            .unwrap();
            close_with(&json!(overlay), &step["overlay"], 2e-6);
            for (i, p) in step["picks"].as_array().unwrap().iter().enumerate() {
                let p = serde_json::from_value(p.clone()).unwrap();
                assert_eq!(
                    rotation::hit(&overlay, p),
                    step["hits"][i].as_bool().unwrap()
                );
                close(
                    &json!(rotation::screen_angle(
                        serde_json::from_value(step["angle_center"].clone()).unwrap(),
                        p
                    )),
                    &step["screen_angles"][i],
                );
            }
        }
        let drag = &c["drag"];
        let center = serde_json::from_value(drag["center"].clone()).unwrap();
        let start = rotation::screen_angle(
            center,
            serde_json::from_value(drag["press"].clone()).unwrap(),
        );
        for step in drag["moves"].as_array().unwrap() {
            let angle = rotation::dragged(
                drag["initial"].as_f64().unwrap(),
                start,
                rotation::screen_angle(
                    center,
                    serde_json::from_value(step["pointer"].clone()).unwrap(),
                ),
            )
            .unwrap();
            close(&json!(angle), &step["expected"]);
        }
    }
}
#[test]
fn no_drift_roundtrip_old_native_basis_repair_and_invalid_changes_are_atomic() {
    let c = &fixture()["cases"][1];
    let mut raster: GeoTiffInfo = serde_json::from_value(c["input"].clone()).unwrap();
    let metadata = raster.metadata.clone();
    let mut old = raster.clone();
    old.metadata.as_object_mut().unwrap().remove(rotation::BASE);
    rotation::set(&mut old, 0.).unwrap();
    let expected = &c["steps"].as_array().unwrap().last().unwrap()["expected"];
    close(&json!(old.transform), &expected["transform"]);
    for _ in 0..2000 {
        rotation::set(&mut raster, 90.).unwrap();
        rotation::set(&mut raster, -90.).unwrap();
    }
    rotation::set(&mut raster, 0.).unwrap();
    close(&json!(raster.transform), &expected["transform"]);
    assert!(!rotation::set(&mut raster, 0.).unwrap());
    assert_eq!(raster.metadata["keep"], metadata["keep"]);
    assert_eq!(
        raster.metadata["template_reference_point"],
        metadata["template_reference_point"]
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rotated.json");
    rotation::set(&mut raster, 37.5).unwrap();
    let project = sleufbase_rust::project::NativeProject {
        rasters: vec![raster.clone()],
        ..Default::default()
    };
    sleufbase_rust::project::save_project_new(&path, &project).unwrap();
    let mut restored = sleufbase_rust::project::read_project(&path)
        .unwrap()
        .rasters
        .remove(0);
    assert_eq!(json!(restored), json!(raster));
    rotation::set(&mut restored, 0.).unwrap();
    close(&json!(restored.transform), &expected["transform"]);
    let before = json!(raster);
    assert!(rotation::set(&mut raster, f64::NAN).is_err());
    assert_eq!(json!(raster), before);
    raster.metadata[rotation::BASE] = json!([0., 0., 0., 0., 0., 0., 0., 0., 1.]);
    let before = json!(raster);
    assert!(rotation::set(&mut raster, 90.).is_err());
    assert_eq!(json!(raster), before);
    assert_eq!(rotation::parse("１２,５").unwrap(), 12.5);
    assert_eq!(rotation::parse(" -540 ").unwrap(), 180.);
    assert!(rotation::parse("NaN").is_err());
    assert!(rotation::parse("1e999").is_err());
}
