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
fn python_exchange_preserves_existing_assets_that_import_could_not_read(){
    use sleufbase_rust::legacy_project::{ProjectUiState,legacy_payload};
    let dir=tempfile::tempdir().unwrap();std::fs::create_dir(dir.path().join("assets")).unwrap();
    std::fs::write(dir.path().join("assets/reference.tiff"),b"not a TIFF").unwrap();
    std::fs::write(dir.path().join("assets/context.dxf"),b"not a DXF").unwrap();
    let project=import_value(fixture(),&dir.path().join("source.klicproject")).unwrap();
    assert!(project.rasters.is_empty());assert!(project.overlays.is_empty());
    let out=legacy_payload(&project,&ProjectUiState::default()).unwrap();
    assert_eq!(out["tiffs"].as_array().unwrap().len(),2);assert_eq!(out["dxfs"].as_array().unwrap().len(),1);
    assert_eq!(PathBuf::from(out["tiffs"][1]["path"].as_str().unwrap()),dir.path().join("assets/reference.tiff"));
}

#[test]
fn python_exchange_remaps_archived_layer_selection_after_reordering(){
    use sleufbase_rust::legacy_project::{ProjectUiState,legacy_payload};
    let dir=tempfile::tempdir().unwrap();let source=dir.path().join("source.klicproject");
    let mut raw=fixture();raw["tiffs"].as_array_mut().unwrap().swap(0,1);
    raw["ui"]["selected_tiff_index"]=json!(1);
    let project=import_value(raw.clone(),&source).unwrap();
    let ui=ProjectUiState::from_project(&project).unwrap().unwrap();assert_eq!(ui.selected_tiff_index,Some(0));
    let out=legacy_payload(&project,&ui).unwrap();assert_eq!(out["tiffs"][0]["type"],"virtual_trench");
    raw["ui"]["selected_tiff_index"]=json!(99);let project=import_value(raw,&source).unwrap();
    assert_eq!(ProjectUiState::from_project(&project).unwrap().unwrap().selected_tiff_index,None);
}

#[test]
fn python_exchange_does_not_resurrect_removed_alignment(){
    use sleufbase_rust::legacy_project::{ProjectUiState,legacy_payload};
    let dir=tempfile::tempdir().unwrap();
    let mut project=import_value(fixture(),&dir.path().join("source.klicproject")).unwrap();
    assert!(project.virtual_trenches[0].alignment.is_some());
    project.virtual_trenches[0].alignment=None;
    let out=legacy_payload(&project,&ProjectUiState::default()).unwrap();
    for key in ["marxact_alignment_auto_start","marxact_alignment_auto_end","marxact_alignment_auto_width","marxact_alignment_rotation_degrees"]{assert!(out["tiffs"][0]["metadata"].get(key).is_none());}
    let restored=import_value(out,&dir.path().join("roundtrip.klicproject")).unwrap();
    // Python boundary geometry supplies a fresh default alignment basis.
    assert_eq!(restored.virtual_trenches[0].alignment.as_ref().unwrap().rotation_degrees,0.);
}

#[test]
fn python_exchange_keeps_unknown_fields_after_native_edit_and_never_overwrites(){
    use sleufbase_rust::legacy_project::{ProjectUiState,legacy_payload,write_legacy_project};
    let dir=tempfile::tempdir().unwrap();let source=dir.path().join("original.klicproject");std::fs::write(&source,FIXTURE).unwrap();
    let mut project=read_legacy_project(&source).unwrap();let mut ui=ProjectUiState::from_project(&project).unwrap().unwrap();
    assert_eq!(ui.center_x,168002.);assert_eq!(ui.selected_tiff_index,Some(0));
    project.name="Bewerkt project Ø".into();let trench=&mut project.virtual_trenches[0];trench.label="Hernoemde sleuf".into();trench.objects[0].object_name="Gewijzigd object".into();trench.objects[0].position.z=Some(4.5);ui.center_x+=100.;
    let payload=legacy_payload(&project,&ui).unwrap();assert_eq!(payload["project_name"],"Bewerkt project Ø");assert_eq!(payload["template_export_settings"],fixture()["template_export_settings"]);assert_eq!(payload["tiffs"][0]["metadata"]["custom_metadata"],fixture()["tiffs"][0]["metadata"]["custom_metadata"]);
    let p=&payload["tiffs"][0]["metadata"]["virtual_trench_payload"]["points"][1];assert_eq!(p["custom_object_metadata"],json!({"keep":true}));assert_eq!(p["object_name"],"Gewijzigd object");assert_eq!(p["z"],4.5);
    assert!(payload["tiffs"][0]["metadata"].get("_sleufbase_python_tiff_index").is_none());
    let path=dir.path().join("export.klicproject");write_legacy_project(&project,&path,&ui,&std::sync::atomic::AtomicBool::new(false)).unwrap();
    let restored=read_legacy_project(&path).unwrap();assert_eq!(restored.virtual_trenches[0].objects[0].position.z,Some(4.5));assert_eq!(restored.virtual_trenches[0].objects[0].metadata["custom_object_metadata"],json!({"keep":true}));assert_eq!(restored.comments[0].text,project.comments[0].text);
    assert!(write_legacy_project(&project,&source,&ui,&std::sync::atomic::AtomicBool::new(false)).is_err());assert_eq!(std::fs::read_to_string(source).unwrap(),FIXTURE);
    assert!(write_legacy_project(&project,&dir.path().join("cancel.klicproject"),&ui,&std::sync::atomic::AtomicBool::new(true)).is_err());assert!(!dir.path().join("cancel.klicproject").exists());
}

#[test]
fn python_exchange_rejects_duplicate_provenance_and_preserves_missing_asset_links(){
    use sleufbase_rust::legacy_project::{ProjectUiState,legacy_payload};
    let dir=tempfile::tempdir().unwrap();let source=dir.path().join("PS42.klicproject");let mut project=import_value(fixture(),&source).unwrap();
    let payload=legacy_payload(&project,&ProjectUiState::default()).unwrap();assert_eq!(std::path::PathBuf::from(payload["tiffs"][1]["path"].as_str().unwrap()),dir.path().join("assets/reference.tiff"));assert_eq!(std::path::PathBuf::from(payload["dxfs"][0]["path"].as_str().unwrap()),dir.path().join("assets/context.dxf"));
    project.virtual_trenches.push(project.virtual_trenches[0].clone());assert!(legacy_payload(&project,&ProjectUiState::default()).unwrap_err().contains("dubbelzinnig"));
}

#[test]
fn manual_copy_is_a_new_exchange_layer_and_keeps_its_unknown_point_fields(){
    use sleufbase_rust::legacy_project::{ProjectUiState,legacy_payload};
    let dir=tempfile::tempdir().unwrap();let source=dir.path().join("source.klicproject");
    let mut project=import_value(fixture(),&source).unwrap();
    let copy=sleufbase_rust::virtual_trench_editor::manual_copy(&project.virtual_trenches[0],"kopie").unwrap();
    assert!(copy.metadata.get("_sleufbase_python_tiff_index").is_none());
    project.virtual_trenches.push(copy);
    let payload=legacy_payload(&project,&ProjectUiState::default()).unwrap();
    assert_eq!(payload["tiffs"][1]["metadata"]["virtual_trench_payload"]["points"][1]["custom_object_metadata"],serde_json::json!({"keep":true}));
    let restored=import_value(payload,&dir.path().join("roundtrip.klicproject")).unwrap();
    assert_eq!(restored.virtual_trenches.len(),2);assert!(restored.virtual_trenches[1].alignment.is_none());
}

#[test]
fn python_exchange_preserves_unknown_ui_and_loaded_dxf_fields(){
    use sleufbase_rust::legacy_project::{ProjectUiState,legacy_payload};
    let dir=tempfile::tempdir().unwrap();std::fs::create_dir(dir.path().join("assets")).unwrap();
    std::fs::write(dir.path().join("assets/context.dxf"),"0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nCABLE\n10\n168000\n20\n401000\n11\n168004\n21\n401002\n0\nENDSEC\n0\nEOF\n").unwrap();
    let mut raw=fixture();raw["ui"]["future_ui_option"]=json!({"keep":[1,"Ø"]});raw["dxfs"][0]["future_style"]=json!({"colors":[1,2,3]});
    let mut project=import_value(raw.clone(),&dir.path().join("source.klicproject")).unwrap();assert_eq!(project.overlays.len(),1);project.overlays[0].visible=true;
    let mut ui=ProjectUiState::from_project(&project).unwrap().unwrap();ui.opacity_percent=35.;let out=legacy_payload(&project,&ui).unwrap();assert_eq!(out["ui"]["future_ui_option"],raw["ui"]["future_ui_option"]);assert_eq!(out["ui"]["opacity_percent"],35.);assert_eq!(out["dxfs"][0]["future_style"],raw["dxfs"][0]["future_style"]);assert_eq!(out["dxfs"][0]["visible"],true);
}

#[test]
fn virtual_reference_metadata_is_active_without_duplicating_geometry_or_losing_the_original(){
    let mut legacy=fixture();let item=legacy["tiffs"].as_array_mut().unwrap().iter_mut().find(|v|v["type"]=="virtual_trench").unwrap();
    item["metadata"]["template_reference_point"]=json!({"x":100.25,"y":200.75});item["metadata"]["rotation_degrees"]=json!(37.5);item["metadata"]["unknown_layer_option"]=json!({"preserve":"Ø"});
    let dir=tempfile::tempdir().unwrap();let project=import_value(legacy.clone(),&dir.path().join("original.klicproject")).unwrap();let trench=&project.virtual_trenches[0];
    assert_eq!(trench.metadata["template_reference_point"],json!({"x":100.25,"y":200.75}));assert!(trench.metadata.get("virtual_trench_payload").is_none());assert!(trench.metadata.get("marxact_boundary_3d").is_none());assert!(!trench.boundary_3d.is_empty());assert!(sleufbase_rust::virtual_map::reference(trench).unwrap().is_some());
    let native=dir.path().join("native.json");save_project(&native,&project).unwrap();let restored=read_project(&native).unwrap();assert_eq!(restored.virtual_trenches[0].metadata,trench.metadata);assert_eq!(restored.legacy_archive.as_ref().unwrap().original.as_ref(),&legacy);
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
    let exported=dir.path().join("roundtrip.klicproject");
    let export=||std::process::Command::new(env!("CARGO_BIN_EXE_sleufbase-rust")).arg("--export-python").arg(&exported).arg(&output).output().unwrap();
    let result=export();assert!(result.status.success(),"{}",String::from_utf8_lossy(&result.stderr));
    let exported_bytes=std::fs::read(&exported).unwrap();
    let restored=read_legacy_project(&exported).unwrap();
    assert_eq!(restored.virtual_trenches[0].objects[0].metadata["custom_object_metadata"],json!({"keep":true}));
    assert_eq!(restored.comments[0].text,"Controleer diepte – café");
    assert!(!export().status.success());assert_eq!(std::fs::read(&exported).unwrap(),exported_bytes);
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

#[test]
fn python_exchange_roundtrips_active_dekbands_and_rejects_ambiguous_extra_metadata(){
    use sleufbase_rust::legacy_project::{ProjectUiState,legacy_payload};
    let dir=tempfile::tempdir().unwrap();let source=dir.path().join("source.klicproject");
    let mut raw=fixture();raw["tiffs"][0]["metadata"]["template_dekband_lines"]=json!([
        {"start_chainage":0.5,"end_chainage":3.,"start_depth":0.2,"end_depth":0.7,"source_name":"Band Ø","display_rgb":[12,34,56],"future_band":{"preserve":true}}
    ]);
    let mut project=import_value(raw,&source).unwrap();
    assert_eq!(project.virtual_trenches[0].dekband_rows[0].end_depth,0.7);
    assert_eq!(project.virtual_trenches[0].dekband_rows[0].display_rgb,Some([12,34,56]));
    let out=legacy_payload(&project,&ProjectUiState::default()).unwrap();
    assert_eq!(out["tiffs"][0]["metadata"]["template_dekband_lines"][0]["future_band"],json!({"preserve":true}));
    project.virtual_trenches[0].dekband_rows[0].start_depth=0.4;
    assert!(legacy_payload(&project,&ProjectUiState::default()).unwrap_err().contains("onbekende metadata"));
    project.virtual_trenches[0].metadata["template_dekband_lines"][0].as_object_mut().unwrap().remove("future_band");
    let out=legacy_payload(&project,&ProjectUiState::default()).unwrap();
    assert_eq!(out["tiffs"][0]["metadata"]["template_dekband_lines"][0]["start_depth"],0.4);
    let restored=import_value(out,&source).unwrap();assert_eq!(restored.virtual_trenches[0].dekband_rows[0].start_depth,0.4);
    project.virtual_trenches[0].dekband_rows.clear();
    assert!(legacy_payload(&project,&ProjectUiState::default()).unwrap()["tiffs"][0]["metadata"].get("template_dekband_lines").is_none());
}

#[test]
fn python_exchange_rejects_native_template_preferences_instead_of_exporting_stale_archive(){
    use sleufbase_rust::legacy_project::{ProjectUiState,legacy_payload};
    let mut project=sleufbase_rust::project::NativeProject::default();
    project.template_export=Some(sleufbase_rust::template_order::State::default());
    assert!(legacy_payload(&project,&ProjectUiState::default()).unwrap_err().contains("Rust-sjabloonvoorkeuren"));
}
