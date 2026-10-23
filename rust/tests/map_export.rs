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

#[test]
fn export_annotations_fit_low_resolution_pages_and_keep_exact_rd_positions() {
    use sleufbase_rust::{map_annotations, Bounds};
    let cancel = AtomicBool::new(false);
    let mut fonts = map_text::Fonts::load(Some(&font_paths()), &cancel).unwrap();
    let reference = [155012.345, 463007.123];
    for dpi in [30, 72, 96, 150, 200, 300, 600] {
        let layout = map_page::layout(Bounds { min_x:155000., min_y:463000., max_x:155030., max_y:463020. }, map_page::Settings { dpi, scale:225. }).unwrap();
        let ops = map_annotations::plan(&layout, Some("Bron: PDOK ©"), Some(reference), &mut |t,s,b| fonts.measure(t,s,b), &cancel).unwrap();
        let ring = ops.iter().find_map(|op| match op { map_page::Op::Ellipse { bounds, .. } => Some(bounds), _=>None }).unwrap();
        let screen = layout.screen(reference);
        assert!(((ring[0]+ring[2])/2. - (screen[0]+layout.origin[0])).abs()<1e-8);
        assert!(((ring[1]+ring[3])/2. - (screen[1]+layout.origin[1])).abs()<1e-8);
        // Original 22px diameter at 300 DPI is 1.863mm on every output page.
        assert!(((ring[2]-ring[0])*25.4/f64::from(dpi)-22.*25.4/300.).abs()<0.006);
        let title = ops.iter().position(|op| matches!(op,map_page::Op::Text{text,..} if text=="Legenda")).unwrap();
        let legend = ops[..title].iter().rev().find_map(|op|match op { map_page::Op::Rect{bounds,..}=>Some(bounds),_=>None }).unwrap();
        assert!(legend[0]>=layout.origin[0] && legend[1]>=layout.origin[1], "{dpi} DPI: {legend:?}");
        assert!(legend[2]<=layout.origin[0]+f64::from(layout.map_size[0])+1e-8);
        assert!(legend[3]<=layout.origin[1]+f64::from(layout.map_size[1])+1e-8);
        assert!((legend[3]-legend[1])<f64::from(layout.map_size[1])*0.55);
        if dpi>=72 { assert!((legend[2]-legend[0])<f64::from(layout.map_size[0])*0.4); }
        let north = ops.iter().find_map(|op|match op {map_page::Op::Polygon{points,..} if points.len()==4=>Some(points),_=>None}).unwrap();
        assert!(north.iter().all(|p|p[0]<legend[0]), "north arrow overlaps legend at {dpi} DPI");
        if dpi==300 {
            let original = map_page::plan(&layout,Some("Bron: PDOK ©"),Some(reference),&mut |t,s,b|fonts.measure(t,s,b),&cancel).unwrap();
            assert_eq!(serde_json::to_value(&ops).unwrap(),serde_json::to_value(&original).unwrap());
        }
        let plain = sleufbase_rust::map_annotations::plan(&layout,Some("Bron: PDOK ©"),None,&mut |t,s,b|fonts.measure(t,s,b),&cancel).unwrap();
        let original = map_page::plan(&layout,Some("Bron: PDOK ©"),None,&mut |t,s,b|fonts.measure(t,s,b),&cancel).unwrap();
        assert_eq!(serde_json::to_value(plain).unwrap(),serde_json::to_value(original).unwrap());
    }
}

#[test]
fn annotation_only_changes_keep_raster_layout_and_cancellation_contracts() {
    let cancel = AtomicBool::new(true);
    let layout = map_page::layout(sleufbase_rust::Bounds { min_x:0.,min_y:0.,max_x:30.,max_y:20. },map_page::Settings{dpi:96,scale:225.}).unwrap();
    assert!(sleufbase_rust::map_annotations::plan(&layout,None,Some([10.,10.]),&mut |_,_,_|panic!("cancelled before fonts"),&cancel).is_err());
    let mut invalid = layout;
    invalid.page_size[0]=u32::MAX;
    assert!(sleufbase_rust::map_annotations::plan(&invalid,None,None,&mut |_,_,_|panic!("invalid before fonts"),&cancel).is_err());
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
fn virtual_rd_map_matches_an_actual_tiff_and_keeps_source_precedence_and_orientation() {
    use sleufbase_rust::{template_raster, virtual_raster, virtual_trench::VirtualTrench, Vertex};
    let cancel = AtomicBool::new(false);
    let mut virtual_source = virtual_raster::Source {
        trench: VirtualTrench::new(
            "PS virtueel".into(),
            Vertex {
                x: 168000.,
                y: 450000.,
                z: Some(1.),
            },
            Vertex {
                x: 168010.,
                y: 450004.,
                z: Some(0.8),
            },
            0.6,
        )
        .unwrap(),
        quality: 1.,
        reverse: false,
    };
    let info = virtual_raster::layout(&virtual_source.trench, virtual_source.quality).unwrap();
    let raw = virtual_raster::render(&virtual_source, &cancel).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("equivalent.tiff");
    let file = std::fs::File::create(&path).unwrap();
    let mut encoder = tiff::encoder::TiffEncoder::new(file).unwrap();
    let mut encoded = encoder
        .new_image::<tiff::encoder::colortype::RGBA8>(info.width, info.height)
        .unwrap();
    // Declare straight alpha; RGBA samples alone do not imply ExtraSamples.
    encoded
        .encoder()
        .write_tag(tiff::tags::Tag::ExtraSamples, &[2u16][..])
        .unwrap();
    encoded.write_data(raw.as_raw()).unwrap();
    drop(encoder);
    let physical = template_raster::Source {
        path,
        transform: info.transform(),
        reverse_orientation: false,
        axis: None,
    };
    let mut source = map_export::Source {
        settings: map_page::Settings {
            dpi: 72,
            scale: 225.,
        },
        background: Background::Solid {
            color: [245, 245, 245, 255],
        },
        fonts: Some(font_paths()),
        ..Default::default()
    };
    let mut renderer = map_export::Renderer::default();
    let physical_page = renderer
        .render(&source, info.bounds, Some(&physical), None, &cancel)
        .unwrap();
    let virtual_page = renderer
        .render_with_raster(
            &source,
            info.bounds,
            Some(map_export::Raster::Virtual(&virtual_source)),
            None,
            &cancel,
        )
        .unwrap();
    pixels(
        &virtual_page.image,
        &physical_page.image,
        "virtual RD vs TIFF map",
    );
    virtual_source.reverse = true;
    let reversed = renderer
        .render_with_raster(
            &source,
            info.bounds,
            Some(map_export::Raster::Virtual(&virtual_source)),
            None,
            &cancel,
        )
        .unwrap();
    pixels(
        &reversed.image,
        &physical_page.image,
        "map remains at RD position when profile reverses",
    );
    virtual_source.quality = f64::NAN;
    assert!(renderer
        .render_with_raster(
            &source,
            info.bounds,
            Some(map_export::Raster::Virtual(&virtual_source)),
            None,
            &cancel
        )
        .is_err());
    source.tiff = Some(physical);
    let explicit = renderer
        .render_with_raster(
            &source,
            info.bounds,
            Some(map_export::Raster::Virtual(&virtual_source)),
            None,
            &cancel,
        )
        .unwrap();
    pixels(
        &explicit.image,
        &physical_page.image,
        "explicit map source precedes slot fallback",
    );
    source.include_tiff = false;
    let blank = renderer
        .render_with_raster(
            &source,
            info.bounds,
            Some(map_export::Raster::Virtual(&virtual_source)),
            None,
            &cancel,
        )
        .unwrap();
    assert_ne!(blank.image, physical_page.image);
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
        }]
        .into(),
        ..Default::default()
    };
    assert!(map_export::Renderer::default()
        .render(&source, b, None, None, &cancel)
        .is_err());
    std::sync::Arc::make_mut(&mut source.overlays).clear();
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
