use serde_json::Value;
use sleufbase_rust::ahn::{decode_ground_level,nearest_valid_height};
#[test]
fn original_pillow_tiff_nodata_nearest_scan_order_and_rounding(){
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ahn");
    let data:Value=serde_json::from_str(include_str!("fixtures/ahn/reference.json")).unwrap();
    for case in data["cases"].as_array().unwrap(){
        let pixels=std::fs::read(root.join(case["path"].as_str().unwrap())).unwrap();
        let expected=case["expected"].as_f64();
        assert_eq!(decode_ground_level(&pixels).unwrap(),expected,"{}",case["path"]);
    }
    for case in data["rounding"].as_array().unwrap(){assert_eq!(nearest_valid_height(&[case["value"].as_f64().unwrap()],1,1),case["expected"].as_f64());}
    assert!(decode_ground_level(b"not a TIFF").is_err());
    assert_eq!(nearest_valid_height(&[],usize::MAX,usize::MAX),None);
}
