use sleufbase_rust::{ai_maaiveld as ai,vllm_runtime as runtime};
use serde_json::Value;
#[test]
fn isolated_original_parser_prompt_wire_layout_and_managed_command(){
    let oracle:Value=serde_json::from_str(include_str!("fixtures/ai-maaiveld/reference.json")).unwrap();
    for case in oracle["parse"].as_array().unwrap(){let result=ai::parse_maaiveld_json(case["raw"].as_str().unwrap());if case["error"].as_bool()==Some(true){assert!(result.is_err());}else{assert_eq!(serde_json::to_value(result.unwrap()).unwrap(),case["expected"]);}}
    let mut payload=ai::payload(&image::RgbImage::from_pixel(2,2,image::Rgb([12,34,56])),"gemma-fixture").unwrap();payload["messages"][0]["content"][1]["image_url"]["url"]=Value::String("PNG_DATA_URL".into());assert_eq!(payload,oracle["protocol"]["json"]);
    for case in oracle["layout"].as_array().unwrap(){let bounds=serde_json::from_value(case["layer"].clone()).unwrap();let points:Vec<ai::Point>=serde_json::from_value(case["points"].clone()).unwrap();let size=serde_json::from_value(case["size"].clone()).unwrap();let got=serde_json::to_value(ai::numbered_layout(bounds,&points,size).unwrap()).unwrap();for key in ["min_x","min_y","max_x","max_y"]{assert!((got[key].as_f64().unwrap()-case["expected"][key].as_f64().unwrap()).abs()<1e-9);}}
    let temp=tempfile::tempdir().unwrap();let tools=temp.path().join(".tools/vllm");std::fs::create_dir_all(tools.join("Scripts")).unwrap();std::fs::write(tools.join("Scripts/vllm.exe"),b"never executed").unwrap();
    for case in oracle["launches"].as_array().unwrap(){let env=serde_json::from_value(case["overrides"].clone()).unwrap();let config=runtime::Config::from_environment(temp.path().into(),env);let launch=runtime::launch_plan(&config).unwrap();let actual=launch.args.iter().map(|s|s.replace(&temp.path().to_string_lossy().to_string(),"ROOT")).collect::<Vec<_>>();assert_eq!(serde_json::to_value(actual).unwrap(),case["args"]);for (key,value) in case["env"].as_object().unwrap(){assert_eq!(launch.environment[key].replace(&temp.path().to_string_lossy().to_string(),"ROOT"),value.as_str().unwrap(),"{key}");}}
}
