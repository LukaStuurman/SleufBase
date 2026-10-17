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
        path: "overlay.dxf".into(),
        visible: true,
        layers: Default::default(),
        blocks:Default::default(),warnings:Vec::new(),expanded:Default::default(),block_visibility:Default::default(),block_warnings:Vec::new(),cached_bounds:None,bounds_ready:false,
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
        kickthemap_job_sidecar: None,
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
