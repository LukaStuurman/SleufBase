use sleufbase_rust::{
    bgt::{BgtKind, BgtLayer},
    cadastral::{CadLabel, CadastralLayer},
    dxf_read::{DxfFeature, DxfOverlay},
    geo::GeoTransform,
    geotiff::GeoTiffInfo,
    model::Bounds,
    project::{
        read_project, save_project, save_project_new, Comment, NativeProject, MAX_PROJECT_BYTES,
    },
};

fn bounds() -> Bounds {
    Bounds {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 10.0,
        max_y: 10.0,
    }
}

#[test]
fn invalid_persisted_geometry_never_replaces_previous_project() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.json");
    let good = NativeProject::default();
    save_project(&path, &good).unwrap();
    let original = std::fs::read(&path).unwrap();
    let mut invalid = Vec::new();
    let mut p = good.clone();
    p.comments.push(Comment {
        x: f64::NAN,
        y: 0.0,
        text: "test".into(),
    });
    invalid.push(p);
    let mut p = good.clone();
    p.bgt_layers.push(BgtLayer {
        kind: BgtKind::Wegdeel,
        bounds: bounds(),
        visible: true,
        paths: vec![vec![(0.0, 0.0), (f64::INFINITY, 1.0)]],
    });
    invalid.push(p);
    let mut p = good.clone();
    p.overlays.push(DxfOverlay {
        space_policy:Default::default(),
        path: "overlay.dxf".into(),
        visible: true,
        layers: Default::default(),
        layer_aliases: Default::default(),
        blocks: Default::default(),
        warnings: Vec::new(),
        expanded: Default::default(),
        block_visibility: Default::default(),
        block_warnings: Vec::new(),
        cached_bounds: None,
        bounds_ready: false,
        features: vec![DxfFeature {
            entity_type: "LINE".into(),
            handle: "1".into(),
            layer: "test".into(),
            color_aci: 7,
            points: vec![sleufbase_rust::Vertex {
                x: 0.0,
                y: 0.0,
                z: Some(f64::NAN),
            }],
            raw_text: String::new(),
            block_name: String::new(),
            closed: None,
            cad: None,
        }],
    });
    invalid.push(p);
    let mut p = good.clone();
    p.cadastral = Some(CadastralLayer {
        bounds: bounds(),
        visible: true,
        linework: vec![],
        labels: vec![CadLabel {
            layer_name: "label".into(),
            text: "street".into(),
            position: (0.0, 0.0),
            rotation: f64::NAN,
        }],
    });
    invalid.push(p);
    let mut p = good.clone();
    p.rasters.push(GeoTiffInfo {
        path: "raster.tif".into(),
        width: 10,
        height: 10,
        transform: GeoTransform {
            a: 1.0,
            b: 1.0,
            c: 0.0,
            d: 1.0,
            e: 1.0,
            f: 0.0,
        },
        bounds: bounds(),
        epsg: Some(28992),
        kickthemap_job_id: None,
        kickthemap_job_title: String::new(),
        kickthemap_job_sidecar: None,metadata:Default::default(),
    });
    invalid.push(p);
    for p in invalid {
        assert!(save_project(&path, &p).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn conversion_and_backups_cannot_overwrite_existing_output() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.json");
    std::fs::write(&path, b"previous drawing").unwrap();
    assert!(save_project_new(&path, &NativeProject::default()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"previous drawing");
}

#[test]
fn unknown_project_fields_and_oversized_files_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.json");
    let mut value = serde_json::to_value(NativeProject::default()).unwrap();
    value["future_measurements"] = serde_json::json!([{"value":123}]);
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(read_project(&path)
        .unwrap_err()
        .contains("future_measurements"));
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(MAX_PROJECT_BYTES + 1).unwrap();
    drop(file);
    assert!(read_project(&path).unwrap_err().contains("te groot"));
}

#[test]
fn saved_measurements_preserve_f64_bits_and_archive_large_json_numbers() {
    use sleufbase_rust::{legacy_project::LegacyArchive, project::Comment};
    use std::sync::Arc;
    let mut values = vec![
        -0.42838541666666685,
        -0.0,
        0.0,
        f64::MAX,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
    ];
    let mut bits = 0x83a9_d285_176f_b42du64;
    for _ in 0..4096 {
        bits ^= bits << 13;
        bits ^= bits >> 7;
        bits ^= bits << 17;
        let value = f64::from_bits(bits);
        if value.is_finite() {
            values.push(value);
        }
    }
    let original: serde_json::Value = serde_json::from_str(r#"{"version":1,"positive":1234567890123456789012345678901234567890,"negative":-1234567890123456789012345678901234567890,"decimal":0.123456789012345678901234567890123456789,"large_exponent":1e10000}"#).unwrap();
    let archive_text = serde_json::to_string(&original).unwrap();
    assert!(archive_text.contains("1234567890123456789012345678901234567890"));
    assert!(archive_text.contains("0.123456789012345678901234567890123456789"));
    assert_eq!(
        original["large_exponent"].to_string().replace("e+", "e"),
        "1e10000"
    );
    let project = NativeProject {
        comments: values
            .iter()
            .map(|&v| Comment {
                x: v,
                y: -v,
                text: String::new(),
            })
            .collect(),
        legacy_archive: Some(LegacyArchive {
            source_path: "synthetic.klicproject".into(),
            original: Arc::new(original.clone()),
            notices: vec![],
        }),
        ..Default::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("numbers.json");
    save_project(&path, &project).unwrap();
    let loaded = read_project(&path).unwrap();
    assert_eq!(loaded.comments.len(), values.len());
    for (actual, expected) in loaded.comments.iter().zip(&values) {
        assert_eq!(
            actual.x.to_bits(),
            expected.to_bits(),
            "measurement {expected}"
        );
        assert_eq!(actual.y.to_bits(), (-expected).to_bits());
    }
    assert_eq!(*loaded.legacy_archive.unwrap().original, original);
}
