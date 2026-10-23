use sleufbase_rust::{project::NativeProject,virtual_trench::VirtualTrench,Vertex};
fn trench()->VirtualTrench{VirtualTrench::new("Metadata".into(),Vertex{x:155000.,y:463000.,z:None},Vertex{x:155010.,y:463000.,z:None},0.6).unwrap()}
#[test]
fn endpoint_metadata_counts_against_both_layer_and_project_budgets(){
    let mut layer=trench();let block=serde_json::json!({"unknown":"x".repeat(1024*1024)});
    layer.start_metadata=block.clone();assert!(layer.validate().is_ok());
    layer.end_metadata=block.clone();assert!(layer.validate().unwrap_err().contains("puntmetadata"));
    layer.end_metadata=serde_json::Value::Null;
    let mut project=NativeProject{virtual_trenches:vec![layer.clone();15],..Default::default()};
    project.validate().unwrap();project.virtual_trenches.extend([layer.clone(),layer]);
    assert!(project.validate().unwrap_err().contains("laagmetadata"));
}
