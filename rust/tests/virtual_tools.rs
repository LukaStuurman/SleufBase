use serde_json::{json,Value};
use sleufbase_rust::{virtual_map::{self as map,MapPoint},virtual_trench::VirtualTrench,model::Vertex};
fn close(a:f64,b:f64){assert!((a-b).abs()<2e-8,"{a} != {b}");}
fn points(t:&VirtualTrench)->Vec<Vertex>{std::iter::once(t.start).chain(std::iter::once(t.end)).chain(t.objects.iter().map(|p|p.position)).collect()}
#[test]
fn original_axis_transform_direction_copy_point_hits_and_actual_ahn_queue(){
    let data:Value=serde_json::from_str(include_str!("fixtures/virtual-tools/reference.json")).unwrap();
    for (n,c) in data["cases"].as_array().unwrap().iter().enumerate(){
        let source:VirtualTrench=serde_json::from_value(c["source"].clone()).unwrap();let mut t=source.clone();
        let start=serde_json::from_value(c["target_start"].clone()).unwrap();let end=serde_json::from_value(c["target_end"].clone()).unwrap();
        map::transform_axis(&mut t,start,end).unwrap();
        let expected:Vec<Vertex>=serde_json::from_value(c["expected_geometry"].clone()).unwrap();
        for (a,b) in points(&t).iter().zip(expected){close(a.x,b.x);close(a.y,b.y);assert_eq!(a.z,b.z);}
        let original_objects=serde_json::to_value(&source.objects).unwrap();let changed_objects=serde_json::to_value(&t.objects).unwrap();
        for (a,b) in original_objects.as_array().unwrap().iter().zip(changed_objects.as_array().unwrap()){for key in ["role","object_name","source_name","attributes","metadata"]{assert_eq!(a[key],b[key]);}}
        let direction=serde_json::from_value(c["direction"].clone()).unwrap();let actual=map::copy_target_end(&source,start,direction).unwrap();let expected:[f64;2]=serde_json::from_value(c["copy_end"].clone()).unwrap();for i in 0..2{close(actual[i],expected[i]);}
        for (p,expected) in c["picks"].as_array().unwrap().iter().zip(c["expected_picks"].as_array().unwrap()){
            let p=serde_json::from_value(p.clone()).unwrap();let id=map::pick_point(&source,p,0.05,0.3).map(|p|match p{MapPoint::Start=>"start".into(),MapPoint::End=>"end".into(),MapPoint::Object(i)=>format!("p{i}")});
            assert_eq!(id.as_deref(),expected.as_str(),"case{n}");
        }
        let mut t=source;map::apply_surface_height(&mut t,MapPoint::Start,c["height"].as_f64()).unwrap();
        let expected:VirtualTrench=serde_json::from_value(c["expected_surface"].clone()).unwrap();
        for (a,b) in points(&t).iter().zip(points(&expected)){assert_eq!(a.z,b.z,"AHN {n}");}
        for (a,b) in std::iter::once(&t.start_metadata).chain(t.objects.iter().map(|p|&p.metadata)).zip(std::iter::once(&expected.start_metadata).chain(expected.objects.iter().map(|p|&p.metadata))){for k in ["surface_z","surface_mode","surface_status","unknown"]{assert_eq!(a.get(k),b.get(k),"AHN{n} {k}");}}
    }
}
#[test]
fn atomic_guards_measured_copy_project_dekband_and_pending_relative_depth(){
    let mut t=VirtualTrench::new("Bron".into(),Vertex{x:100.,y:200.,z:None},Vertex{x:110.,y:200.,z:None},0.6).unwrap();
    t.objects.push(sleufbase_rust::virtual_trench::TrenchPoint{role:"object".into(),object_name:"Kabel".into(),source_name:"LS".into(),position:Vertex{x:103.,y:201.,z:None},attributes:Default::default(),metadata:json!({"surface_mode":"start","sleufbase_depth_meters":0.7,"unknown":{"deep":"bewaren"}})});
    map::apply_surface_height(&mut t,MapPoint::Start,Some(3.2)).unwrap();close(t.objects[0].position.z.unwrap(),2.5);assert!(t.end.z.is_none());
    map::apply_surface_height(&mut t,MapPoint::Start,Some(4.2)).unwrap();close(t.objects[0].position.z.unwrap(),3.5);
    t.objects[0].metadata["surface_mode"]=json!("manual");map::apply_surface_height(&mut t,MapPoint::Start,Some(7.)).unwrap();close(t.objects[0].position.z.unwrap(),3.5);
    let before=serde_json::to_vec(&t).unwrap();assert!(map::move_point(&mut t,MapPoint::End,[100.,200.]).is_err());assert_eq!(serde_json::to_vec(&t).unwrap(),before);
    assert!(map::move_point(&mut t,MapPoint::Object(99),[103.,200.]).is_err());assert_eq!(serde_json::to_vec(&t).unwrap(),before);
    assert!(map::apply_surface_height(&mut t,MapPoint::Start,Some(f64::NAN)).is_err());assert_eq!(serde_json::to_vec(&t).unwrap(),before);
    map::move_point(&mut t,MapPoint::Object(0),[120.,203.]).unwrap();assert_eq!([t.objects[0].position.x,t.objects[0].position.y],[110.,200.]);
    map::add_dekband(&mut t,[108.,203.],[102.,199.],"LS Ø".into(),Some([10,20,30])).unwrap();assert_eq!(t.dekband_rows[0].start_chainage,2.);assert_eq!(t.dekband_rows[0].end_chainage,8.);
    t.boundary_3d=vec![t.start,t.end,Vertex{x:110.,y:201.,z:Some(3.)}];t.source_path=Some("source.dxf".into());let before=serde_json::to_vec(&t).unwrap();assert!(map::move_point(&mut t,MapPoint::Start,[95.,200.]).is_err());assert_eq!(serde_json::to_vec(&t).unwrap(),before);
    let copied=map::copy_in_direction(&t,"Kopie".into(),[200.,300.],[200.,310.]).unwrap();close(copied.length(),t.length());assert!(copied.boundary_3d.is_empty());assert!(copied.source_path.is_none());assert_eq!(copied.objects[0].metadata["unknown"],t.objects[0].metadata["unknown"]);
    let project=sleufbase_rust::project::NativeProject{virtual_trenches:vec![t,copied],..Default::default()};project.validate().unwrap();let bytes=serde_json::to_vec(&project).unwrap();let roundtrip:sleufbase_rust::project::NativeProject=serde_json::from_slice(&bytes).unwrap();assert_eq!(serde_json::to_vec(&roundtrip).unwrap(),bytes);
}
