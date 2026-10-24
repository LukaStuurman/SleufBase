use sleufbase_rust::{
    cadastral_overview::{self, Request},
    dxf_document::Document,
    export_control::Control,
};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
fn request(index: usize) -> Request {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cadastral-overview");
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("reference.json")).unwrap()).unwrap();
    let mut request: Request = serde_json::from_value(cases[index]["request"].clone()).unwrap();
    for layer in &mut request.layers {
        layer.path = root.join(layer.path.file_name().unwrap());
    }
    request
}
fn files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut result = Vec::new();
    for e in std::fs::read_dir(root).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            result.extend(files(&p));
        } else {
            result.push((p.clone(), std::fs::read(p).unwrap()));
        }
    }
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}
#[test]
fn eight_original_option_combinations_control_geometry_rasters_and_labels() {
    let dir = tempfile::tempdir().unwrap();
    for index in 0..8 {
        let request = request(index);
        let path = dir.path().join(format!("{index}.dxf"));
        let info = cadastral_overview::export(&path, &request, &Control::default()).unwrap();
        let doc = Document::load(&path, &AtomicBool::new(false)).unwrap();
        let count = |kind: &str| {
            doc.records()
                .filter(|r| r.section() == Some("ENTITIES") && r.kind() == kind)
                .count()
        };
        assert_eq!(count("IMAGE"), if index % 2 == 1 { 3 } else { 0 });
        assert_eq!(count("HATCH"), if index == 0 { 3 } else { 0 });
        assert_eq!(
            doc.records()
                .filter(|r| r.kind() == "TEXT" && r.first(8) == Some("PROEFSLEUVEN_LABEL"))
                .count(),
            if request.settings.trench_mode == cadastral_overview::TrenchMode::None {
                0
            } else {
                request.layers.len()
            }
        );
        assert_eq!(info.images, if index % 2 == 1 { 3 } else { 0 });
    }
}
#[test]
fn source_protection_and_final_gate_cancellation_preserve_old_output_and_assets() {
    let dir = tempfile::tempdir().unwrap();
    let request = request(1);
    let source = request.layers[0].path.clone();
    let before = std::fs::read(&source).unwrap();
    assert!(cadastral_overview::export(&source, &request, &Control::default()).is_err());
    assert_eq!(std::fs::read(source).unwrap(), before);
    let path = dir.path().join("old.dxf");
    cadastral_overview::export(&path, &request, &Control::default()).unwrap();
    let before = files(dir.path());
    let control = Control::default();
    let mut reached = false;
    assert!(
        cadastral_overview::export_with_progress(&path, &request, &control, |stage| {
            if stage == "Kadastraal overzicht definitief opslaan" {
                reached = true;
                assert!(control.cancel());
            }
        })
        .is_err()
    );
    assert!(reached);
    assert_eq!(files(dir.path()), before);
    let mut bad = request;
    bad.layers[0].path = dir.path().join("missing.tif");
    assert!(cadastral_overview::export(&path, &bad, &Control::default()).is_err());
    assert_eq!(files(dir.path()), before);
}

#[test]
fn template_overview_stays_outside_paired_states_and_shares_atomic_rollback() {
    use sleufbase_rust::template_export;
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/template-export/reference.json")).unwrap();
    let mut value = cases
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "native-tiff")
        .unwrap()["request"]
        .clone();
    value["template"] = serde_json::json!(repository.join("assets/cadastral_template.dxf"));
    value["slots"].as_array_mut().unwrap().truncate(1);
    let slot = &mut value["slots"][0];
    slot["profile"] = serde_json::Value::Null;
    slot["tiff_source"]["path"] = serde_json::json!(
        repository.join("rust/tests/fixtures/template-raster/orientation-1.tiff")
    );
    slot["map_png"] = serde_json::json!("");
    slot["map_source"] = serde_json::json!({"settings":{"dpi":40,"scale":225.},"background":{"kind":"solid","color":[245,245,245,255]}});
    value["settings"]["paired"] = serde_json::json!(true);
    let mut template: template_export::Request = serde_json::from_value(value).unwrap();
    assert!(template.overview.is_none()); // old prepared manifests remain valid
    template.overview = Some(request(3));
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("paired.dxf");
    template_export::export(&output, &template, &Control::default()).unwrap();
    let doc = Document::load(&output, &AtomicBool::new(false)).unwrap();
    let images: Vec<_> = doc
        .records()
        .filter(|r| r.kind() == "IMAGE" && r.first(8) == Some("PROEFSLEUVEN_TIFF"))
        .collect();
    assert_eq!(images.len(), 3);
    assert!(images.iter().all(|r| r.section() == Some("ENTITIES")));
    let root=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cadastral-overview");
    let original=Document::load(&root.join("centerline-tiffs.dxf"),&AtomicBool::new(false)).unwrap();
    for layer in original.records().filter(|r|r.kind()=="LAYER"&&r.first(2).is_some_and(|name|name.starts_with("KAD_")||name.starts_with("BGT_")||name.starts_with("PROEFSLEUVEN_"))){
        let actual=doc.records().find(|r|r.kind()=="LAYER"&&r.first(2)==layer.first(2)).unwrap();
        for code in [62,370,420]{assert_eq!(actual.first(code),layer.first(code),"original layer {:?}, tag {code}",layer.first(2));}
    }
    let source = template.overview.as_ref().unwrap().layers[0].path.clone();
    let bytes = std::fs::read(&source).unwrap();
    assert!(template_export::export(&source, &template, &Control::default()).is_err());
    assert_eq!(std::fs::read(source).unwrap(), bytes);
    let before = files(dir.path());
    let control = Control::default();
    let mut reached = false;
    assert!(
        template_export::export_with_progress(&output, &template, &control, |stage| {
            if stage == "Export definitief opslaan" {
                reached = true;
                assert!(control.cancel());
            }
        })
        .is_err()
    );
    assert!(reached);
    assert_eq!(files(dir.path()), before);
    template.overview.as_mut().unwrap().layers[0].path = dir.path().join("missing-source.tiff");
    assert!(template_export::export(&output, &template, &Control::default()).is_err());
    assert_eq!(files(dir.path()), before);
}

#[test]
fn no_trench_and_no_tiffs_do_not_open_pixel_sources() {
    let dir = tempfile::tempdir().unwrap();
    let mut request = request(4);
    for layer in &mut request.layers {
        layer.path = dir.path().join("not-present.tiff");
    }
    assert_eq!(
        request.settings.trench_mode,
        cadastral_overview::TrenchMode::None
    );
    assert!(!request.settings.include_tiff_images);
    let result = cadastral_overview::export(
        &dir.path().join("cadastre-only.dxf"),
        &request,
        &Control::default(),
    )
    .unwrap();
    assert_eq!(result.images, 0);
}

#[test]
fn virtual_metadata_shared_by_layer_and_source_is_quoted_once() {
    use sleufbase_rust::{virtual_trench::VirtualTrench, Vertex};
    let mut request = request(4);
    let mut prototype = request.layers[0].clone();
    let mut trench = VirtualTrench::new(
        "PS1".into(),
        Vertex {
            x: 0.,
            y: 0.,
            z: None,
        },
        Vertex {
            x: 10.,
            y: 0.,
            z: None,
        },
        0.6,
    )
    .unwrap();
    trench.metadata = serde_json::json!({"payload":"x".repeat(1024*1024)});
    prototype.metadata = trench.metadata.clone();
    prototype.virtual_trench = Some(trench);
    request.layers = vec![prototype; 9];
    assert!(
        cadastral_overview::export_bounds(&request).is_ok(),
        "nine shared MiB must not be counted as eighteen"
    );
    for layer in &mut request.layers {
        layer.metadata = serde_json::json!({"different":"y".repeat(1024*1024)});
    }
    assert!(
        cadastral_overview::export_bounds(&request)
            .unwrap_err()
            .contains("metadata"),
        "genuinely separate maps still count toward sixteen MiB"
    );
}
