use serde_json::{json,Value};
use sleufbase_rust::{legacy_project::{import_value,ProjectUiState},project::{NativeProject,read_project,save_project,FORMAT_VERSION},project_view::{Camera,ProjectView,TiffSelection}};
fn fixture()->Value{serde_json::from_str(include_str!("fixtures/python-v1.klicproject")).unwrap()}
#[test]
fn imported_camera_and_original_selection_are_independent_of_export_constraints(){
    let dir=tempfile::tempdir().unwrap();let mut raw=fixture();raw["tiffs"].as_array_mut().unwrap().swap(0,1);raw["ui"]["selected_tiff_index"]=json!(1);
    raw["ui"]["opacity_percent"]=json!(32.);raw["ui"]["use_background"]=json!(false);raw["ui"]["background_source"]=json!("OpenStreetMap");
    let mut project=import_value(raw,&dir.path().join("original.klicproject")).unwrap();
    assert_eq!(ProjectUiState::from_archive(&project).unwrap().unwrap().selected_tiff_index,Some(1));
    let view=project.view.as_ref().unwrap();assert_eq!(view.camera.as_ref().unwrap().center,[168002.,401001.]);assert_eq!(view.opacity_percent,32.);assert!(!view.use_background);assert_eq!(view.background_source,"OpenStreetMap");
    assert!(matches!(view.selected_tiff,Some(TiffSelection::Virtual{..})));assert_eq!(view.selected_virtual_index(&project),Some(0));
    project.template_export=Some(Default::default());
    assert!(ProjectView::from_legacy(&project).unwrap().is_some());
    let path=dir.path().join("native.json");save_project(&path,&project).unwrap();assert_eq!(read_project(&path).unwrap().view,project.view);
}
#[test]
fn old_native_versions_upgrade_without_rewriting_source_and_view_roundtrips(){
    let dir=tempfile::tempdir().unwrap();let source=dir.path().join("old.json");
    for version in 1..=4{
        let mut raw=serde_json::to_value(NativeProject::default()).unwrap();raw["version"]=json!(version);raw.as_object_mut().unwrap().remove("view");
        let bytes=serde_json::to_vec(&raw).unwrap();std::fs::write(&source,&bytes).unwrap();
        let restored=read_project(&source).unwrap();assert_eq!(restored.version,FORMAT_VERSION);assert!(restored.view.is_none());assert_eq!(std::fs::read(&source).unwrap(),bytes);
    }
    let mut project=NativeProject::default();project.view=Some(ProjectView{camera:Some(Camera{center:[123.,456.],meters_per_pixel:0.03125}),..Default::default()});
    save_project(&source,&project).unwrap();assert_eq!(read_project(&source).unwrap().view,project.view);
    project.view.as_mut().unwrap().camera.as_mut().unwrap().meters_per_pixel=0.;assert!(project.validate().is_err());
}
#[test]
fn native_selection_reordering_and_ambiguity_never_pick_another_trench(){
    let dir=tempfile::tempdir().unwrap();let mut project=import_value(fixture(),&dir.path().join("source.klicproject")).unwrap();
    let view=project.view.clone().unwrap();let mut other=project.virtual_trenches[0].clone();other.label="other".into();project.virtual_trenches.insert(0,other);
    assert_eq!(view.for_export(&project).unwrap().selected_tiff_index,Some(1));
    project.virtual_trenches.push(project.virtual_trenches[1].clone());assert_eq!(view.for_export(&project).unwrap().selected_tiff_index,None);
}
#[test]
fn background_provider_labels_preserve_historical_names_without_unknown_fallback(){
    use sleufbase_rust::project_view::BackgroundProvider as B;
    for label in ["PDOK luchtfoto","Luchtfoto NL actueel (8 cm, deels 5 cm)",B::PdokAerial.label()]{assert_eq!(B::from_label(label),B::PdokAerial);}
    for provider in [B::OpenStreetMap,B::Cadastral,B::Cyclomedia]{assert_eq!(B::from_label(provider.label()),provider);}
    let view=ProjectView{background_source:"future private source".into(),..Default::default()};assert_eq!(view.provider(),B::Unknown);assert_eq!(view.background_source,"future private source");
}
#[test]
fn scoped_project_settings_persist_without_mutating_general_settings(){
    let dir=tempfile::tempdir().unwrap();let path=dir.path().join("project.json");let mut general=sleufbase_rust::settings_compat::MigratedSettings::default();general.dxf_label_gap=7.;
    let scoped=sleufbase_rust::settings_compat::with_project_overrides(&general,&json!({"dxf_label_gap":2.,"template_client_logo_preset":"enexis"})).unwrap();
    let mut project=NativeProject::default();project.project_settings=Some(scoped);save_project(&path,&project).unwrap();
    let restored=read_project(&path).unwrap();assert_eq!(restored.project_settings.unwrap().dxf_label_gap,2.);assert_eq!(general.dxf_label_gap,7.);
}
#[test]
fn removed_cyclomedia_preference_migrates_active_view_but_keeps_original_archive(){
    let dir=tempfile::tempdir().unwrap();let mut raw=fixture();raw["ui"]["background_source"]=json!("Cyclomedia Luchtfoto NL (nieuwste jaar / laagste cm)");
    let project=import_value(raw.clone(),&dir.path().join("old.klicproject")).unwrap();assert_eq!(project.view.as_ref().unwrap().provider(),sleufbase_rust::project_view::BackgroundProvider::PdokAerial);assert_eq!(project.legacy_archive.as_ref().unwrap().original.as_ref(),&raw);
    let ui=ProjectUiState::from_project(&project).unwrap().unwrap();assert_eq!(sleufbase_rust::project_view::BackgroundProvider::from_label(&ui.background_source),sleufbase_rust::project_view::BackgroundProvider::PdokAerial);
}
