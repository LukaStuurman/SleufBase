use serde_json::Value;
use sleufbase_rust::{model::Geometry,virtual_map,virtual_trench::VirtualTrench};
use std::sync::atomic::AtomicBool;

#[test]
fn original_dekband_profile_injection_clips_depths_and_uses_chosen_direction(){
    let fixture:Value=serde_json::from_str(include_str!("fixtures/dekband/reference.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap(){
        let t:VirtualTrench=serde_json::from_value(case["source"].clone()).unwrap();
        assert_eq!(t.to_dataset(0,true).features.len(),2,"Objectexport bevat uitsluitend meetpunten");
        let mut result=t.to_base_dataset_with_cancel(0,true,&AtomicBool::new(false)).unwrap();
        let rules=sleufbase_rust::RULES.to_vec();
        let start=if case["reverse"].as_bool().unwrap(){t.end}else{t.start};
        let profile=sleufbase_rust::profile::build(&result,&rules,sleufbase_rust::profile::Options{forced_start:Some([start.x,start.y]),marker_scale:0.1,..Default::default()}).unwrap().unwrap();
        assert!(profile.axis_length>t.length(),"endpoint markers must expand the base profile");
        let before=serde_json::to_vec(&result).unwrap();
        assert!(t.append_dekband_for_profile(&mut result,&profile,&AtomicBool::new(true)).is_err());
        assert_eq!(serde_json::to_vec(&result).unwrap(),before);
        t.append_dekband_for_profile(&mut result,&profile,&AtomicBool::new(false)).unwrap();
        assert_eq!(result.job_id,73);
        let lines:Vec<_>=result.features.iter().filter(|f|matches!(f.geometry,Geometry::Polyline(_))).collect();
        let expected=case["expected"].as_array().unwrap();assert_eq!(lines.len(),expected.len());
        for (line,expected) in lines.iter().zip(expected){
            let actual=serde_json::to_value(line).unwrap();
            for key in ["object_name","source_name","attributes"]{assert_eq!(actual[key],expected[key]);}
            for (a,b) in actual["geometry"]["Polyline"].as_array().unwrap().iter().zip(expected["geometry"]["Polyline"].as_array().unwrap()){
                for k in ["x","y","z"]{assert!((a[k].as_f64().unwrap()-b[k].as_f64().unwrap()).abs()<1e-9,"{k}: {a} != {b}");}
            }
        }
        assert!(t.to_dataset_with_direction(0,true,false,&AtomicBool::new(true)).is_err());
    }
}

#[test]
fn reverse_map_picks_keep_each_clicked_endpoint_depth_and_reject_atomically(){
    let mut t=VirtualTrench::new("PS".into(),sleufbase_rust::model::Vertex{x:0.,y:0.,z:Some(2.)},sleufbase_rust::model::Vertex{x:10.,y:0.,z:Some(4.)},0.6).unwrap();
    virtual_map::add_dekband_with_depth(&mut t,[8.,3.],[2.,-1.],"Band".into(),Some([1,2,3]),0.7,0.2).unwrap();
    let row=&t.dekband_rows[0];
    assert_eq!((row.start_chainage,row.end_chainage,row.start_depth,row.end_depth),(2.,8.,0.2,0.7));
    let before=serde_json::to_vec(&t).unwrap();
    for (a,b,depth) in [([4.,1.],[4.,-1.],0.2),([1.,0.],[2.,0.],f64::NAN),([1.,0.],[2.,0.],-1.)]{
        assert!(virtual_map::add_dekband_with_depth(&mut t,a,b,"Band".into(),None,depth,0.2).is_err());
        assert_eq!(serde_json::to_vec(&t).unwrap(),before);
    }
}
