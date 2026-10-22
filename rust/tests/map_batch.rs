use serde_json::Value;
use sleufbase_rust::{
    export_control::Control,
    export_worker::{Exporter, Inputs, Kind, Request},
    geotiff::GeoTiffInfo,
    map_batch::{self, Settings},
    map_text::Paths,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/map-batch")
}
fn data() -> Value {
    serde_json::from_str(include_str!("fixtures/map-batch/reference.json")).unwrap()
}
fn raster(case: &Value) -> GeoTiffInfo {
    GeoTiffInfo {
        path: root().join(case["path"].as_str().unwrap()),
        width: 53,
        height: 37,
        transform: serde_json::from_value(case["transform"].clone()).unwrap(),
        bounds: serde_json::from_value(case["bounds"].clone()).unwrap(),
        epsg: Some(28992),
        kickthemap_job_id: None,
        kickthemap_job_title: String::new(),
        kickthemap_job_sidecar: None,metadata:Default::default(),
    }
}
fn fonts() -> Paths {
    let path = root().join("../map-export/Hack-Regular.ttf");
    Paths {
        regular: path.clone(),
        bold: path,
    }
}
fn settings(c: &Value) -> Settings {
    Settings {
        dpi: c["dpi"].as_u64().unwrap() as u32,
        scale: c["scale"].as_f64().unwrap(),
        background: "white".into(),
        ..Default::default()
    }
}
fn request(path: &Path, settings: Settings) -> Request {
    Request {
        kind: Kind::Maps,
        path: path.to_owned(),
        selection: sleufbase_rust::object_selection::Selection::all(0, 0),
        rules: Vec::new(),
        extra_layers: false,
        options: Default::default(),
        template: None,
        discipline_excel: false,
        maps: Some(settings),point_target:None,point_network:false,
    }
}
#[test]
fn original_standalone_pages_keep_rgb_dpi_dimensions_opacity_and_sample_pixels() {
    let data = data();
    for case in data["cases"].as_array().unwrap() {
        let dir = tempfile::tempdir().unwrap();
        let input = Inputs {
            rasters: vec![raster(case)],
            ..Default::default()
        };
        let paths = map_batch::export_with(
            dir.path(),
            &settings(case),
            &input,
            Some(fonts()),
            &Control::default(),
        )
        .unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths[0].file_name().unwrap().to_string_lossy(),
            case["expected"]["name"].as_str().unwrap()
        );
        let mut reader =
            png::Decoder::new(std::io::BufReader::new(fs::File::open(&paths[0]).unwrap()))
                .read_info()
                .unwrap();
        let info = reader.info();
        assert_eq!(info.color_type, png::ColorType::Rgb);
        let dims = info.pixel_dims.unwrap();
        assert_eq!(dims.unit, png::Unit::Meter);
        for (actual, expected) in [dims.xppu, dims.yppu]
            .into_iter()
            .zip(case["expected"]["dpi"].as_array().unwrap())
        {
            assert!((f64::from(actual) * 0.0254 - expected.as_f64().unwrap()).abs() < 1e-9);
        }
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let frame = reader.next_frame(&mut pixels).unwrap();
        assert_eq!(
            serde_json::json!([frame.width, frame.height]),
            case["expected"]["size"]
        );
        for sample in case["expected"]["samples"].as_array().unwrap() {
            let x = sample[0].as_u64().unwrap() as usize;
            let y = sample[1].as_u64().unwrap() as usize;
            let index = (y * frame.width as usize + x) * 3;
            assert_eq!(
                &pixels[index..index + 3],
                sample[2]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| n.as_u64().unwrap() as u8)
                    .collect::<Vec<_>>(),
                "{} ({x},{y})",
                case["path"]
            );
        }
    }
}
#[test]
fn duplicate_names_are_all_published_while_original_overwrite_bug_is_explicit() {
    let data = data();
    let bug = &data["original_duplicate_overwrite"];
    assert_eq!(
        bug["names"],
        serde_json::json!(["scan_kaart.png", "scan_kaart.png"])
    );
    assert_eq!(bug["remaining_files"], 1);
    assert_eq!(
        map_batch::filenames([
            "first/scan.tif",
            "second/scan.tiff",
            "Scan.TIF",
            "scan_kaart_2.tiff"
        ]),
        vec![
            "scan_kaart.png",
            "scan_kaart_2.png",
            "Scan_kaart_3.png",
            "scan_kaart_2_kaart.png"
        ]
    );
    let dir = tempfile::tempdir().unwrap();
    let mut a = raster(&data["cases"][0]);
    let mut b = a.clone();
    // Real distinct TIFF sources retain the same stem and separate input allocations.
    let source_dir = tempfile::tempdir().unwrap();
    fs::create_dir(source_dir.path().join("a")).unwrap();
    fs::create_dir(source_dir.path().join("b")).unwrap();
    let first = source_dir.path().join("a/scan.tif");
    let second = source_dir.path().join("b/scan.tiff");
    fs::copy(&a.path, &first).unwrap();
    fs::copy(&b.path, &second).unwrap();
    a.path = first;
    b.path = second;
    let old_pdf = dir.path().join("scan_kaart.pdf");
    fs::write(&old_pdf, b"keep unrelated legacy PDF").unwrap();
    let output = map_batch::export_with(
        dir.path(),
        &settings(&data["cases"][0]),
        &Inputs {
            rasters: vec![a, b],
            ..Default::default()
        },
        Some(fonts()),
        &Control::default(),
    )
    .unwrap();
    assert_eq!(output.len(), 2);
    assert_ne!(output[0], output[1]);
    assert!(output.iter().all(|p| p.is_file()));
    assert_eq!(fs::read(old_pdf).unwrap(), b"keep unrelated legacy PDF");
}
#[test]
fn late_render_error_cancel_and_protected_sources_keep_previous_outputs() {
    let data = data();
    let a = raster(&data["cases"][0]);
    let mut b = raster(&data["cases"][1]);
    b.path = PathBuf::from("missing-late-map.tiff");
    let dir = tempfile::tempdir().unwrap();
    let name = map_batch::filenames([&a.path]).remove(0);
    let output = dir.path().join(name);
    fs::write(&output, b"old page").unwrap();
    let input = Inputs {
        rasters: vec![a, b],
        ..Default::default()
    };
    assert!(map_batch::export_with(
        dir.path(),
        &settings(&data["cases"][0]),
        &input,
        Some(fonts()),
        &Control::default()
    )
    .is_err());
    assert_eq!(fs::read(&output).unwrap(), b"old page");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    let cancelled = Control::default();
    assert!(cancelled.cancel());
    assert!(map_batch::export(dir.path(), &Settings::default(), &input, &cancelled).is_err());
    let protected = Inputs {
        rasters: vec![raster(&data["cases"][0])],
        protected_paths: vec![output.clone()],
        ..Default::default()
    };
    assert!(map_batch::export_with(
        dir.path(),
        &settings(&data["cases"][0]),
        &protected,
        Some(fonts()),
        &Control::default()
    )
    .unwrap_err()
    .contains("projectbron"));
    assert_eq!(fs::read(output).unwrap(), b"old page");
}
#[test]
fn real_shared_worker_returns_sources_after_map_success_and_failure() {
    let data = data();
    let dir = tempfile::tempdir().unwrap();
    let mut worker = Exporter::new().unwrap();
    let input = Inputs {
        rasters: vec![raster(&data["cases"][0])],
        ..Default::default()
    };
    let pointer = input.rasters.as_ptr();
    let mut input = Some(input);
    for background in ["white", "cadastral", "white"] {
        let mut settings = settings(&data["cases"][0]);
        settings.background = background.into();
        worker
            .start(input.take().unwrap(), request(dir.path(), settings))
            .unwrap();
        let until = Instant::now() + Duration::from_secs(20);
        let ready = loop {
            if let Some(r) = worker.poll() {
                break r;
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(2));
        };
        assert_eq!(ready.inputs.rasters.as_ptr(), pointer);
        assert_eq!(ready.result.is_ok(), background == "white");
        input = Some(ready.inputs);
    }
}
#[test]
fn original_provider_preferences_migrate_and_older_native_settings_default() {
    use sleufbase_rust::settings_compat::{self, MigratedSettings};
    for provider in ["pdok", "osm", "cadastral"] {
        let config=settings_compat::from_legacy_json(&serde_json::json!({"map_export_background_source":provider,"include_klic_in_map_exports":"ja"}).to_string()).unwrap();
        assert_eq!(config.map_export.background, provider);
        assert!(config.map_export.include_cad);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        settings_compat::save_rust_settings(&path, &config).unwrap();
        assert_eq!(settings_compat::read_rust_settings(&path).unwrap(), config);
    }
    let mut old = serde_json::to_value(MigratedSettings::default()).unwrap();
    old.as_object_mut().unwrap().remove("map_export");
    let restored: MigratedSettings = serde_json::from_value(old).unwrap();
    assert_eq!(restored.map_export, Settings::default());
}
#[test]
fn removed_cyclomedia_preferences_migrate_without_reintroducing_a_native_provider() {
    use sleufbase_rust::settings_compat::{self, MigratedSettings};
    let legacy = settings_compat::from_legacy_json(
        r#"{"map_export_background_source":"cyclomedia_aerial"}"#,
    )
    .unwrap();
    assert_eq!(legacy.map_export.background, "pdok");
    let mut native = serde_json::to_value(MigratedSettings::default()).unwrap();
    native["map_export"]["background"] = Value::String("cyclomedia_aerial".into());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old-native-settings.json");
    let original = serde_json::to_vec(&native).unwrap();
    fs::write(&path, &original).unwrap();
    let restored = settings_compat::read_rust_settings(&path).unwrap();
    assert_eq!(restored.map_export.background, "pdok");
    assert_eq!(fs::read(path).unwrap(), original);
    assert!(Settings {
        background: "cyclomedia_aerial".into(),
        ..Default::default()
    }
    .validate()
    .is_err());
    let saved = serde_json::to_string(&restored).unwrap();
    assert!(!saved.contains("cyclomedia"));
}
#[test]
fn visible_cad_is_clipped_into_real_pngs_only_when_requested() {
    use sleufbase_rust::{
        dxf_read::{DxfFeature, DxfOverlay},
        Vertex,
    };
    let data = data();
    let source = raster(&data["cases"][0]);
    let layout = sleufbase_rust::map_page::layout(
        source.bounds,
        sleufbase_rust::map_page::Settings {
            dpi: 96,
            scale: 225.,
        },
    )
    .unwrap();
    let y = (source.bounds.min_y + source.bounds.max_y) * 0.5;
    let mut cad = DxfOverlay {
        visible: true,
        ..Default::default()
    };
    cad.features.push(DxfFeature {
        entity_type: "LINE".into(),
        handle: "F1".into(),
        layer: "KLIC".into(),
        color_aci: 1,
        points: vec![
            Vertex {
                x: layout.bounds.min_x - 100.,
                y,
                z: None,
            },
            Vertex {
                x: layout.bounds.max_x + 100.,
                y,
                z: None,
            },
        ],
        raw_text: String::new(),
        block_name: String::new(),
        closed: None,
        cad: None,
    });
    let mut input = Inputs {
        rasters: vec![source],
        overlays: vec![cad],
        ..Default::default()
    };
    let dir = tempfile::tempdir().unwrap();
    for (included, visible) in [(false, true), (true, true), (true, false)] {
        input.overlays[0].visible = visible;
        let mut settings = settings(&data["cases"][0]);
        settings.include_cad = included;
        let output = map_batch::export_with(
            dir.path(),
            &settings,
            &input,
            Some(fonts()),
            &Control::default(),
        )
        .unwrap();
        let image = image::open(&output[0]).unwrap().into_rgb8();
        let red = image
            .pixels()
            .filter(|p| p[0] > 245 && p[1] < 40 && p[2] < 40)
            .count();
        if included && visible {
            assert!(red > 100);
        } else {
            assert_eq!(red, 0);
        }
    }
}
