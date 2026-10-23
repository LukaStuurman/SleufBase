use sleufbase_rust::virtual_raster;
use std::{path::PathBuf,sync::atomic::AtomicBool};
#[test]
fn every_pixel_matches_active_original_local_spans_masks_colors_and_rounding(){
    let fixture:serde_json::Value=serde_json::from_str(include_str!("fixtures/virtual-raster/reference.json")).unwrap();
    let root=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/virtual-raster");
    for case in fixture["cases"].as_array().unwrap(){
        let source:virtual_raster::Source=serde_json::from_value(case["source"].clone()).unwrap();
        let layout=virtual_raster::layout(&source.trench,source.quality).unwrap();
        assert_eq!(serde_json::to_value([layout.width,layout.height]).unwrap(),case["size"],"{}",case["name"]);
        let image=virtual_raster::render(&source,&AtomicBool::new(false)).unwrap();
        let expected=image::open(root.join(format!("{}.png",case["name"].as_str().unwrap()))).unwrap().into_rgba8();
        let mut mismatches=0;let mut first=None;
        for ((x,y,a),b) in image.enumerate_pixels().zip(expected.pixels()){
            if a!=b{mismatches+=1;if first.is_none(){first=Some((x,y,a.0,b.0));}}
        }
        assert_eq!(mismatches,0,"{}: {mismatches} different pixels; first {first:?}",case["name"]);
    }
    for case in fixture["colors"].as_array().unwrap(){assert_eq!(serde_json::to_value(virtual_raster::display_rgb(&serde_json::json!({"display_rgb":case["value"]}))).unwrap(),case["expected"],"{case}");}
}
