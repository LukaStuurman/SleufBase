use serde_json::{json, Value};
use std::path::PathBuf;
use sleufbase_rust::{
    geo::GeoTransform,
    legacy_project::{import_value, read_legacy_project},
    project::{read_project, save_project},
};

const FIXTURE: &str = include_str!("fixtures/python-v1.klicproject");
fn fixture() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7, "{a} != {b}");
}

#[test]
fn original_python_serializer_fixture_imports_and_preserves_all_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("PS42.klicproject");
    std::fs::write(&source, FIXTURE).unwrap();
    let project = read_legacy_project(&source).unwrap();
    assert_eq!(project.name, "PS42");
    assert_eq!(project.virtual_trenches.len(), 1);
    let trench = &project.virtual_trenches[0];
    assert_eq!(trench.start.z, Some(11.0));
    assert_eq!(trench.end.z, Some(16.0));
    assert_eq!(trench.boundary_3d.len(), 4);
    assert_eq!(trench.objects.len(), 1);
    assert_eq!(
        trench.objects[0].attributes,
        ["water detail", "Water", "marxact_point"]
    );
    assert_eq!(trench.objects[0].source_name, "Waterleiding");
    assert_eq!(trench.to_dataset(0, true).job_id, 123);
    assert_eq!(project.comments[0].text, "Controleer diepte – café");
    assert_eq!(
        project.legacy_archive.as_ref().unwrap().original.as_ref(),
        &fixture()
    );
    assert_eq!(project.legacy_archive.as_ref().unwrap().notices.len(), 3);
    let native = dir.path().join("converted.json");
    save_project(&native, &project).unwrap();
    let restored = read_project(&native).unwrap();
    assert_eq!(
        restored.legacy_archive.unwrap().original.as_ref(),
        &fixture()
    );
    assert_eq!(std::fs::read_to_string(source).unwrap(), FIXTURE);
}

#[test]
fn relative_assets_rotation_job_identity_and_dxf_visibility_are_restored() {
    let dir = tempfile::tempdir().unwrap();
    let assets = dir.path().join("assets");
    std::fs::create_dir(&assets).unwrap();
    let file = std::fs::File::create(assets.join("reference.tiff")).unwrap();
    let mut encoder = tiff::encoder::TiffEncoder::new(file).unwrap();
    let mut image = encoder
        .new_image::<tiff::encoder::colortype::RGB8>(4, 2)
        .unwrap();
    use tiff::tags::Tag;
    image
        .encoder()
        .write_tag(Tag::ModelPixelScaleTag, &[1.0f64, 1.0, 0.0][..])
        .unwrap();
    image
        .encoder()
        .write_tag(
            Tag::ModelTiepointTag,
            &[0.0f64, 0.0, 0.0, 168000.0, 401002.0, 0.0][..],
        )
        .unwrap();
    image.write_data(&[100u8; 24]).unwrap();
    drop(encoder);
    std::fs::write(assets.join("context.dxf"),"0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nCABLE\n10\n168000\n20\n401000\n11\n168004\n21\n401002\n0\nENDSEC\n0\nEOF\n").unwrap();
    let mut legacy=fixture();
    legacy["tiffs"][1]["metadata"]["template_reference_point"]=json!({"x":168001.,"y":401001.});
    legacy["tiffs"][1]["metadata"]["template_cross_section_start_point"]=json!({"x":168000.,"y":401000.});
    legacy["tiffs"][1]["metadata"]["kickthemap_job_features_path"]=json!("assets/reference_jobFeatures.json");
    legacy["tiffs"][1]["metadata"]["keep"]=json!({"unknown":[1,2,3]});
    let project = import_value(legacy.clone(), &dir.path().join("PS42.klicproject")).unwrap();
    assert_eq!(project.rasters.len(), 1);
    assert_eq!(project.overlays.len(), 1);
    assert!(!project.overlays[0].visible);
    let raster = &project.rasters[0];
    assert_eq!(raster.kickthemap_job_id, Some(456));
    assert_eq!(sleufbase_rust::raster_points::stored(raster,sleufbase_rust::raster_points::REFERENCE),Some([168001.,401001.]));
    assert_eq!(sleufbase_rust::raster_points::stored(raster,sleufbase_rust::raster_points::START),Some([168000.,401000.]));
    assert_eq!(raster.metadata["keep"],legacy["tiffs"][1]["metadata"]["keep"]);
    assert_eq!(PathBuf::from(raster.metadata["kickthemap_job_features_path"].as_str().unwrap()),assets.join("reference_jobFeatures.json"));
    assert_eq!(project.legacy_archive.as_ref().unwrap().original.as_ref(),&legacy);
    let expected = GeoTransform {
        a: 1.0,
        b: 0.0,
        c: 168000.0,
        d: 0.0,
        e: -1.0,
        f: 401002.0,
    }
    .rotate_degrees(15.0, 4, 2)
    .unwrap();
    close(raster.transform.a, expected.a);
    close(raster.transform.c, expected.c);
    close(raster.transform.f, expected.f);
    assert_eq!(raster.metadata["base_transform_matrix"],json!([1.,0.,168000.,0.,-1.,401002.,0.,0.,1.]));
    assert_eq!(project.legacy_archive.as_ref().unwrap().notices.len(), 1);
    // A saved base transform overrides the original TIFF transform, just as Python does.
    let mut original = fixture();
    original["tiffs"][1]["metadata"]["base_transform_matrix"] =
        json!([2, 0, 168100, 0, -2, 401100, 0, 0, 1]);
    let project = import_value(original, &dir.path().join("PS42.klicproject")).unwrap();
    let expected = GeoTransform {
        a: 2.0,
        b: 0.0,
        c: 168100.0,
        d: 0.0,
        e: -2.0,
        f: 401100.0,
    }
    .rotate_degrees(15.0, 4, 2)
    .unwrap();
    close(project.rasters[0].transform.c, expected.c);
    let mut revolution=fixture();revolution["tiffs"][1]["metadata"]["rotation_degrees"]=json!(720.);
    let normalized=import_value(revolution.clone(),&dir.path().join("PS42.klicproject")).unwrap();
    assert_eq!(normalized.rasters[0].metadata["rotation_degrees"],json!(0.));
    assert_eq!(normalized.rasters[0].transform,GeoTransform{a:1.,b:0.,c:168000.,d:0.,e:-1.,f:401002.});
    assert_eq!(normalized.legacy_archive.as_ref().unwrap().original.as_ref(),&revolution);
}

#[test]
fn conversion_cli_is_standalone_and_never_overwrites_the_source_or_output() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("PS42.klicproject");
    let output = dir.path().join("native.json");
    std::fs::write(&source, FIXTURE).unwrap();
    let run = || {
        std::process::Command::new(env!("CARGO_BIN_EXE_sleufbase-rust"))
            .arg("--import-python")
            .arg(&output)
            .arg(&source)
            .output()
            .unwrap()
    };
    let result = run();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(read_project(&output).unwrap().virtual_trenches.len(), 1);
    let bytes = std::fs::read(&output).unwrap();
    assert!(!run().status.success());
    assert_eq!(std::fs::read(&output).unwrap(), bytes);
    assert_eq!(std::fs::read_to_string(source).unwrap(), FIXTURE);
}

#[test]
fn malformed_geometry_and_unrecognized_versions_fail_explicitly() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("bad.klicproject");
    for path in ["version", "tiffs", "map_comments"] {
        let mut value = fixture();
        value[path] = json!({"wrong":"type"});
        assert!(import_value(value, &source).is_err());
    }
    let mut value = fixture();
    value["tiffs"][0]["metadata"]["virtual_trench_payload"]["points"][1]["x"] = json!("NaN");
    assert!(import_value(value, &source).is_err());
    let mut value = fixture();
    value["tiffs"][0]["metadata"]["marxact_alignment_rotation_degrees"] = json!(91);
    assert!(import_value(value, &source).is_err());
    let mut value = fixture();
    value["tiffs"][0]["type"] = json!("future_layer");
    let p = import_value(value.clone(), &source).unwrap();
    assert_eq!(p.legacy_archive.unwrap().original.as_ref(), &value);
    let mut value = fixture();
    value["tiffs"][0]["metadata"]["virtual_trench_payload"]["boundary_3d"] = Value::Null;
    value["map_comments"][0]["text"] = json!("  exact text\n ");
    let p = import_value(value, &source).unwrap();
    assert_eq!(p.virtual_trenches[0].boundary_3d.len(), 4);
    assert_eq!(p.comments[0].text, "  exact text\n ");
}
