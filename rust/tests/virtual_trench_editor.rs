use sleufbase_rust::{
    project::{read_project,save_project_new,NativeProject},
    virtual_trench::VirtualTrench,
    virtual_trench_editor::{self as editor},
    model::Vertex,
};
#[test]
fn edited_virtual_trench_survives_native_project_roundtrip_and_feeds_native_exports() {
    let dir=tempfile::tempdir().unwrap();
    let path=dir.path().join("measurements.sleufbase-rust.json");
    let mut t=VirtualTrench::new("Oorspronkelijk".into(),
        Vertex{x:155000.,y:463000.,z:Some(-1.0)},
        Vertex{x:155010.,y:463000.,z:Some(-1.2)},0.6).unwrap();
    editor::import_tabular(&mut t,
        "Rol\tObject\tBron\tKettingmaat\tDwarsafstand\tZ\tAttribuut1\tAttribuut2\tAttribuut3\nLS\tKabel 1\tLS\t2,5\t-0,3\t-1,5\ttype A\t42\tgeel\nGAS\tBuis\tGAS\t7\t0,2\t-0,9\tPVC\t\t"
    ).unwrap();
    assert_eq!(t.objects.len(),2);
    let points=editor::export_tabular(&t).unwrap();
    let mut project=NativeProject::default();
    project.name="MarXact en virtuele sleuven".into();
    project.virtual_trenches.push(t);
    save_project_new(&path,&project).unwrap();
    let saved=read_project(&path).unwrap();
    assert_eq!(saved.virtual_trenches.len(),1);
    let imported=&saved.virtual_trenches[0];
    assert_eq!(imported.objects[0].attributes[1],"42");
    assert_eq!(imported.objects[0].position.z,Some(-1.5));
    let converted=imported.to_dataset(0,true);
    assert_eq!(converted.features.len(),4);
    assert_eq!(converted.features[0].object_name,"Beginpunt");
    assert_eq!(converted.features[1].object_name,"Kabel 1");
    assert_eq!(converted.features[2].object_name,"Buis");
    assert_eq!(converted.features[3].object_name,"Eindpunt");
    assert_eq!(editor::export_tabular(imported).unwrap(),points);
}
#[test]
fn imported_marxact_source_and_original_geometry_survive_editable_copy() {
    let mut original=VirtualTrench::new("Inmeting".into(),
        Vertex{x:100.,y:200.,z:None},
        Vertex{x:110.,y:200.,z:None},0.6).unwrap();
    original.source_path=Some("onderzoeksgebied.dxf".into());
    original.job_id=12345;
    original.boundary_3d=vec![
        Vertex{x:100.,y:200.,z:Some(-1.)},
        Vertex{x:110.,y:200.,z:Some(-1.)},
        Vertex{x:110.,y:201.,z:Some(-1.)},
        Vertex{x:100.,y:201.,z:Some(-1.)},
    ];
    let copy=editor::manual_copy(&original,"(kopie)").unwrap();
    assert!(editor::is_measured(&original));
    assert!(!editor::is_measured(&copy));
    assert_eq!(original.source_path.as_deref(),Some(std::path::Path::new("onderzoeksgebied.dxf")));
    assert_eq!(original.boundary_3d.len(),4);
    assert_eq!(copy.job_id,0);
    assert!(copy.source_path.is_none());
    assert!(copy.boundary_3d.is_empty());
}
