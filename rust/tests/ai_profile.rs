use sleufbase_rust::{ai_profile::{self,Source},ai_scene,export_control::Control,profile_context::Kind,project::NativeProject,settings_compat::MigratedSettings,Vertex};
use serde_json::Value;
use std::sync::Arc;
#[test]
fn original_complete_ai_profile_context_injection_rotation_and_provider_policy(){
    let oracle:Value=serde_json::from_str(include_str!("fixtures/ai-profile/reference.json")).unwrap();
    for case in oracle["cases"].as_array().unwrap(){
        let mut project=NativeProject::default();
        let source=if case["virtual"].as_bool().unwrap(){project.virtual_trenches.push(serde_json::from_value(case["trench"].clone()).unwrap());Source::Virtual(0)}else{project.rasters.push(serde_json::from_value(case["raster"].clone()).unwrap());project.datasets.push(serde_json::from_value(case["dataset"].clone()).unwrap());Source::Raster(0)};
        let original=serde_json::to_vec(&project).unwrap();let mut settings=MigratedSettings::default();settings.object_rules=sleufbase_rust::RULES.to_vec();settings.export_preferences.template_cross_section_marker_diameter=0.1;
        let mut calls=Vec::new();
        let got=ai_profile::prepare_with(&project,source,&settings,&Control::default(),|kind,bounds,_|{calls.push((kind,bounds));if kind==Kind::Roads&&case["fail_roads"].as_bool().unwrap(){Err("offline roads".into())}else{Ok(Arc::new(serde_json::from_value(case[if kind==Kind::Roads{"roads"}else{"terrain"}].clone()).unwrap()))}}).unwrap();
        assert_eq!(calls.len(),2);assert_eq!((calls[0].0,calls[1].0),(Kind::Roads,Kind::Terrain));
        for key in ["min_x","min_y","max_x","max_y"]{for(_,bounds)in &calls{let actual=serde_json::to_value(bounds).unwrap()[key].as_f64().unwrap();assert!((actual-case["fetch_bounds"][key].as_f64().unwrap()).abs()<1e-8,"{} {key}",case["name"]);}}
        for(point,expected)in got.points.iter().zip(case["expected_points"].as_array().unwrap()){
            assert_eq!(point.key,expected["key"].as_str().unwrap());assert_eq!(point.label,expected["label"].as_str().unwrap());
            for (key,value)in[("x",point.x),("y",point.y)]{assert!((value-expected[key].as_f64().unwrap()).abs()<1e-8,"{} {key}: {value} != {}",case["name"],expected[key]);}
        }
        let expected:Vec<Vertex>=serde_json::from_value(case["expected_axis"].clone()).unwrap();for(a,b)in got.final_axis.iter().zip(expected){assert!((a.x-b.x).abs()<1e-8&&(a.y-b.y).abs()<1e-8);assert_eq!(a.z,b.z);}
        assert_eq!(got.bands_added,2);assert_eq!(got.context_failures.len(),usize::from(case["fail_roads"].as_bool().unwrap()));assert_eq!(serde_json::to_vec(&project).unwrap(),original);
        let cancel=Control::default();cancel.cancel();assert!(ai_profile::prepare_with(&project,source,&settings,&cancel,|_,_,_|panic!("cancel must precede context IO")).is_err());
    }
    for row in oracle["providers"].as_array().unwrap(){assert_eq!(format!("{:?}",ai_scene::background_provider(row["setting"].as_str().unwrap())),row["expected"].as_str().unwrap());}
}
