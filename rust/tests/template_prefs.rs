use sleufbase_rust::{
    project::{self,NativeProject},
    template_prefs::TemplatePreferences,
    template_project::Options,
};
#[test]
fn native_project_roundtrips_saved_template_preferences_and_old_v3_files() {
    let dir=tempfile::tempdir().unwrap();
    let path=dir.path().join("native-project.json");
    let mut project=NativeProject::default();
    project.name="Persistente DXF-sjabloonlayout".into();
    let mut options=Options::default();
    options.project="Project 17".into();
    options.drawn_by="Techbase".into();
    options.include_cad=false;
    options.scale=500.;
    project.template_preferences=Some(TemplatePreferences::capture(
        &options,&[],&[],&[],
    ));
    project::save_project_new(&path,&project).unwrap();
    let recovered=project::read_project(&path).unwrap();
    assert_eq!(recovered.name,project.name);
    let preferences=recovered.template_preferences.unwrap();
    assert!(preferences.validate().is_ok());
    let restored=preferences.restore(&[],&[],&[]);
    assert_eq!(restored.project,"Project 17");
    assert_eq!(restored.drawn_by,"Techbase");
    assert_eq!(restored.scale,500.);
    assert!(!restored.include_cad);
    // A native file created before the preferences field existed must remain
    // readable without requiring an explicit schema conversion.
    let original=std::fs::read_to_string(&path).unwrap();
    let mut json:serde_json::Value=serde_json::from_str(&original).unwrap();
    json.as_object_mut().unwrap().remove("template_preferences");
    let old=dir.path().join("native-old-v3.json");
    std::fs::write(&old,serde_json::to_vec(&json).unwrap()).unwrap();
    let recovered=project::read_project(&old).unwrap();
    assert!(recovered.template_preferences.is_none());
}
