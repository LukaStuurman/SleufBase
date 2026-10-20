use image::{Rgba, RgbaImage};
use serde_json::Value;
use sleufbase_rust::{
    map_export::{self, Background},
    map_page, map_raster, map_text,
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/map-export")
}
fn fixture() -> Value {
    serde_json::from_slice(&std::fs::read(root().join("reference.json")).unwrap()).unwrap()
}
fn image(name: &str) -> RgbaImage {
    image::open(root().join(name)).unwrap().into_rgba8()
}
fn decoded<T: serde::de::DeserializeOwned>(v: &Value) -> T {
    serde_json::from_value(v.clone()).unwrap()
}
fn pixels(actual: &RgbaImage, expected: &RgbaImage, name: &str) {
    assert_eq!(actual.dimensions(), expected.dimensions(), "{name}");
    if let Some((i, (a, b))) = actual
        .pixels()
        .zip(expected.pixels())
        .enumerate()
        .find(|(_, (a, b))| a != b)
    {
        panic!(
            "{name}: first different pixel {},{}: {:?} != {:?}",
            i as u32 % actual.width(),
            i as u32 / actual.width(),
            a,
            b
        );
    }
}
fn equivalent(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64(), "{path}"),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                equivalent(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, a) in a {
                equivalent(a, &b[k], &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[test]
fn all_bilinear_crops_match_original_rgba_including_hidden_rgb() {
    let fixture = fixture();
    let cancel = AtomicBool::new(false);
    for c in fixture["resizes"].as_array().unwrap() {
        let resized = map_raster::resize(
            &image(c["input"].as_str().unwrap()),
            decoded(&c["crop"]),
            decoded(&c["size"]),
            &cancel,
        )
        .unwrap();
        pixels(
            &resized,
            &image(c["expected"].as_str().unwrap()),
            c["name"].as_str().unwrap(),
        );
    }
}
#[test]
fn full_tiff_paint_matches_original_axis_and_affine_rendering() {
    let fixture = fixture();
    let cancel = AtomicBool::new(false);
    for c in fixture["paints"].as_array().unwrap() {
        let layout: map_page::Layout = decoded(&c["layout"]);
        let mut painted = RgbaImage::from_pixel(
            layout.map_size[0],
            layout.map_size[1],
            Rgba(decoded(&c["background"])),
        );
        map_raster::paint(
            &mut painted,
            image(c["input"].as_str().unwrap()),
            decoded(&c["transform"]),
            &layout,
            &cancel,
        )
        .unwrap();
        pixels(
            &painted,
            &image(c["expected"].as_str().unwrap()),
            c["name"].as_str().unwrap(),
        );
    }
}
#[test]
fn every_page_operation_and_layout_matches_original_with_source_font_metrics() {
    for c in fixture()["pages"].as_array().unwrap() {
        let layout = map_page::layout(decoded(&c["bounds"]), decoded(&c["settings"])).unwrap();
        equivalent(
            &serde_json::to_value(&layout).unwrap(),
            &c["layout"],
            c["name"].as_str().unwrap(),
        );
        let ops = map_page::plan(
            &layout,
            c["attribution"].as_str(),
            decoded(&c["reference"]),
            &mut |t, s, b| {
                let m = c["measures"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|m| {
                        m["text"] == t && m["size"].as_f64() == Some(f64::from(s)) && m["bold"] == b
                    })
                    .unwrap();
                Ok(decoded(&m["bbox"]))
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        equivalent(
            &serde_json::to_value(ops).unwrap(),
            &c["ops"],
            c["name"].as_str().unwrap(),
        );
    }
}
pub fn font_paths() -> map_text::Paths {
    map_text::Paths {
        regular: root().join("Hack-Regular.ttf"),
        bold: root().join("Hack-Regular.ttf"),
    }
}
#[test]
fn page_border_preserves_the_map_interior_and_cancel_rejects_before_fetch() {
    let source = map_export::Source {
        settings: map_page::Settings {
            dpi: 96,
            scale: 225.,
        },
        background: Background::Solid {
            color: [123, 156, 189, 255],
        },
        include_tiff: false,
        fonts: Some(font_paths()),
        ..Default::default()
    };
    let b = sleufbase_rust::Bounds {
        min_x: 168000.,
        min_y: 450000.,
        max_x: 168030.,
        max_y: 450020.,
    };
    let page = map_export::Renderer::default()
        .render(&source, b, None, None, &AtomicBool::new(false))
        .unwrap();
    let map_pixels = page
        .image
        .pixels()
        .filter(|p| p.0 == [123, 156, 189, 255])
        .count();
    assert!(map_pixels > page.layout.map_size[0] as usize * page.layout.map_size[1] as usize / 2);
    assert_eq!(page.image.get_pixel(82, 82).0, [0, 0, 0, 255]);
    assert_eq!(page.image.get_pixel(0, 0).0, [255; 4]);
    let mut network = source;
    network.background = Background::Pdok;
    assert!(map_export::Renderer::default()
        .render(&network, b, None, None, &AtomicBool::new(true))
        .is_err());
}
#[test]
fn invalid_maps_fail_before_network_or_large_allocations() {
    let b = sleufbase_rust::Bounds {
        min_x: 0.,
        min_y: 0.,
        max_x: 1.,
        max_y: 1.,
    };
    let cancel = AtomicBool::new(false);
    let mut source = map_export::Source {
        include_tiff: false,
        overlays: vec![map_export::Stroke {
            points: vec![[f64::NAN, 0.]],
            color: [0; 3],
            selected: false,
            highlighted: false,
        }],
        ..Default::default()
    };
    assert!(map_export::Renderer::default()
        .render(&source, b, None, None, &cancel)
        .is_err());
    source.overlays.clear();
    source.settings.dpi = u32::MAX;
    assert!(map_export::Renderer::default()
        .render(&source, b, None, None, &cancel)
        .is_err());
    source.settings = Default::default();
    source.include_tiff = true;
    assert!(map_export::Renderer::default()
        .render(&source, b, None, None, &cancel)
        .is_err());
    assert!(map_raster::resize(&RgbaImage::new(3, 3), [0, 0, 4, 4], [2, 2], &cancel).is_err());
    assert!(map_page::layout(
        sleufbase_rust::Bounds { max_x: 1e200, ..b },
        Default::default()
    )
    .is_err());
}
