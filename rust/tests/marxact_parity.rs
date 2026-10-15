//! End-to-end native ASCII MarXact import fixture.
//! Behavioural checks derived from tests/test_marxact_import.py. This test
//! deliberately goes from DXF group codes through project serialization.

use sleufbase_rust::{
    dxf_read::parse_dxf,
    marxact::parse_overlay,
    project::{NativeProject,read_project,save_project},
};
use std::collections::BTreeMap;
use std::path::Path;

fn entity(out:&mut String,kind:&str,pairs:&[(i32,&str)]) {
    out.push_str(&format!("0\n{kind}\n"));
    for (code,value) in pairs {
        out.push_str(&format!("{code}\n{value}\n"));
    }
}
fn fixture()->String {
    let mut dxf="0\nSECTION\n2\nENTITIES\n".to_owned();
    let boundary=[
        ("168000","401000","10"),
        ("168004","401000","14"),
        ("168004","401002","18"),
        ("168000","401002","12"),
    ];
    for (x,y,z) in boundary {
        entity(&mut dxf,"MULTILEADER", &[
            (8,"MEET_LABEL"),(304,"Id=1^JName=PS7^JHeight=10"),
            (10,x),(20,y),(30,z)
        ]);
    }
    entity(&mut dxf,"POLYLINE",&[(8,"Grens"),(70,"9")]);
    for (x,y,z) in boundary {
        entity(&mut dxf,"VERTEX",&[(10,x),(20,y),(30,z)]);
    }
    entity(&mut dxf,"SEQEND",&[]);
    entity(&mut dxf,"MULTILEADER",&[
        (8,"MEET_LABEL"),(304,"Name=water detail^JHeight=3,12"),
        (10,"168002"),(20,"401001"),(30,"3")
    ]);
    entity(&mut dxf,"INSERT",&[
        (8,"Water"),(2,"marxact_point"),
        (10,"168002"),(20,"401001"),(30,"3")
    ]);
    entity(&mut dxf,"INSERT",&[
        (8,"Water"),(2,"marxact_point"),
        (10,"169000"),(20,"401001"),(30,"3")
    ]);
    dxf.push_str("0\nENDSEC\n0\nEOF\n");
    dxf
}

#[test]
fn dxf_to_native_project_roundtrip_preserves_measured_geometry() {
    let overlay=parse_dxf(&fixture(), Path::new("PS7-MarXact.dxf")).unwrap();
    assert_eq!(overlay.features.iter().filter(|f|f.entity_type=="MULTILEADER").count(),5);
    let mappings=BTreeMap::from([("Water".to_owned(),"Waterleiding".to_owned())]);
    let result=parse_overlay(&overlay,&mappings).unwrap();
    assert_eq!(result.trenches.len(),1);
    assert_eq!(result.leader_count,5);
    assert_eq!(result.insert_count,2);
    assert_eq!(result.assigned_insert_count,1);
    let trench=&result.trenches[0];
    assert_eq!(trench.label,"PS7");
    assert_eq!(trench.boundary_3d.len(),4);
    assert_eq!(trench.boundary_3d[2].z,Some(18.0));
    assert_eq!(trench.objects[0].attributes,
        ["water detail".to_owned(),"Water".to_owned(),"marxact_point".to_owned()]);
    assert_eq!(trench.objects[0].source_name,"Waterleiding");
    assert!((trench.objects[0].position.z.unwrap()-3.12).abs()<1e-8);

    let mut project=NativeProject::default();
    project.virtual_trenches=result.trenches;
    let temp=tempfile::tempdir().unwrap();
    let path=temp.path().join("marxact.sleufbase-rust.json");
    save_project(&path,&project).unwrap();
    let restored=read_project(&path).unwrap();
    let trench=&restored.virtual_trenches[0];
    assert_eq!(trench.measured_boundary().len(),5);
    assert_eq!(trench.objects.len(),1);
    assert_eq!(trench.to_dataset(0,false).features.len(),1);
}
