use image::{Rgba, RgbaImage};
use serde_json::Value;
use sleufbase_rust::cadastral_tiles::Plan;
use std::sync::{atomic::AtomicBool, Arc};

#[test]
fn original_requests_and_every_crop_alpha_bilinear_pixel_match() {
    let data: Value =
        serde_json::from_str(include_str!("fixtures/cadastral-tiles/reference.json")).unwrap();
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cadastral-tiles");
    for case in data["cases"].as_array().unwrap() {
        let bounds = serde_json::from_value(case["request"]["bounds"].clone()).unwrap();
        let size = serde_json::from_value(case["request"]["size"].clone()).unwrap();
        let crop = case["plan"]["crop"].as_array().unwrap();
        let empty = crop[0] == crop[2] || crop[1] == crop[3];
        let plan = Plan::new(bounds, size);
        let expected = image::open(root.join(case["image"].as_str().unwrap()))
            .unwrap()
            .into_rgba8();
        if empty {
            assert!(
                expected.pixels().all(|p| p.0 == [0; 4]),
                "preserved original empty-crop bug"
            );
            assert!(plan.unwrap_err().contains("leeg"));
            continue;
        }
        let plan = plan.unwrap();
        let actual = serde_json::to_value(&plan).unwrap();
        for key in [
            "zoom",
            "min_tile_x",
            "max_tile_x",
            "min_tile_y",
            "max_tile_y",
            "stitched_size",
            "target_size",
            "crop",
        ] {
            assert_eq!(actual[key], case["plan"][key], "{key}");
        }
        for key in ["min_x", "min_y", "max_x", "max_y"] {
            assert!(
                (actual["mercator_bounds"][key].as_f64().unwrap()
                    - case["plan"]["mercator_bounds"][key].as_f64().unwrap())
                .abs()
                    < 2e-6,
                "Mercator {key}"
            );
        }
        let ids = plan.tiles();
        let coords = case["plan"]["tile_coords"].as_array().unwrap();
        for ((id, coord), url) in ids.iter().zip(coords).zip(case["urls"].as_array().unwrap()) {
            assert_eq!(
                [id.x, id.y],
                [
                    coord[0].as_u64().unwrap() as u32,
                    coord[1].as_u64().unwrap() as u32
                ]
            );
            assert_eq!(
                id.url().split_once("/Kadastralekaart/").unwrap().1,
                url.as_str()
                    .unwrap()
                    .split_once("/Kadastralekaart/")
                    .unwrap()
                    .1
            );
        }
        assert_eq!(ids.len(), coords.len());
        let salt = case["request"]["salt"].as_u64().unwrap() as u32;
        let image = plan
            .render_with(&AtomicBool::new(false), |ids| {
                Ok(ids
                    .iter()
                    .map(|id| {
                        Arc::new(RgbaImage::from_fn(256, 256, |x, y| {
                            Rgba([
                                ((x * 17 + y * 3 + id.x * 7 + id.y * 11 + salt) % 245) as u8,
                                ((x * 7 + y * 11 + id.x * 13 + id.y * 3 + salt) % 245) as u8,
                                ((x * 23 + y * 19 + id.x * 3 + id.y * 5 + salt) % 245) as u8,
                                [0, 31, 127, 201, 255][((x + y + id.x + id.y) % 5) as usize],
                            ])
                        }))
                    })
                    .collect())
            })
            .unwrap();
        assert_eq!(image, expected, "complete original image {}", case["image"]);
    }
}

#[test]
fn full_a4_plan_keeps_original_zoom_and_rejects_invalid_or_cancelled_inputs() {
    use sleufbase_rust::Bounds;
    let bounds = Bounds {
        min_x: 154980.,
        min_y: 462980.,
        max_x: 155040.,
        max_y: 463020.,
    };
    let plan = Plan::new(bounds, [3344, 2316]).unwrap();
    // No OSM-style zoom reduction to satisfy a preview tile limit: cadastral
    // parcel labels retain the original full export resolution.
    assert!(plan.tiles().len() > 24);
    assert!(plan
        .render_with(&AtomicBool::new(true), |_| panic!("cancel before tiles"))
        .is_err());
    assert!(plan
        .render_with(&AtomicBool::new(false), |_| Ok(Vec::new()))
        .is_err());
    assert!(Plan::new(bounds, [10001, 1]).is_err());
    assert!(Plan::new(bounds, [0, 1]).is_err());
    assert!(Plan::new(
        Bounds {
            min_x: f64::NAN,
            ..bounds
        },
        [32, 32]
    )
    .is_err());
    assert!(Plan::new(
        Bounds {
            min_x: -100000.,
            ..bounds
        },
        [32, 32]
    )
    .is_err());
}
