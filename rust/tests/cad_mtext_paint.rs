#[path = "common/mtext_render.rs"]
#[allow(dead_code)]
mod common;
use serde_json::Value;
use sleufbase_rust::{
    cad_geometry::Affine,
    cad_mtext::Paint,
    cad_text_path::Path as Contours,
    dxf_read::{parse_dxf_bytes_with_catalog, DxfOverlay},
    export_control::Control,
    project::{read_project, save_project, NativeProject},
    Bounds,
};
use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

fn fonts() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    for entry in std::fs::read_dir(common::root().join("fonts")).unwrap() {
        let entry = entry.unwrap();
        let folder = temp.path().join("fonts").join(entry.file_name());
        std::fs::create_dir_all(&folder).unwrap();
        for file in std::fs::read_dir(entry.path()).unwrap() {
            let file = file.unwrap();
            std::fs::copy(file.path(), folder.join(file.file_name())).unwrap();
        }
    }
    temp
}
fn overlay(file: &str, fonts: &Path) -> DxfOverlay {
    let cancel = AtomicBool::new(false);
    let mut catalog = common::catalog(fonts, &cancel).unwrap();
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cad-mtext-paint")
        .join(file);
    parse_dxf_bytes_with_catalog(&std::fs::read(&path).unwrap(), &path, &cancel, &mut catalog)
        .unwrap()
}
fn rgb(v: &Value) -> [u8; 3] {
    serde_json::from_value(v.clone()).unwrap()
}
fn contour(value: &Value) -> Contours {
    let mut path = Contours {
        kinds: vec![0],
        points: vec![serde_json::from_value(value["start"].clone()).unwrap()],
    };
    for c in value["commands"].as_array().unwrap() {
        path.kinds.push(c["kind"].as_u64().unwrap() as u8);
        path.points
            .extend(serde_json::from_value::<Vec<[f64; 3]>>(c["points"].clone()).unwrap());
    }
    path
}
fn verify(overlay: &DxfOverlay) {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/cad-mtext-paint/reference.json")).unwrap();
    let expected = reference["scene"].as_array().unwrap();
    assert_eq!(expected.len(), 141);
    assert_eq!(overlay.rendered_features().count(), expected.len());
    assert!(
        overlay.block_warnings.is_empty(),
        "{:?}",
        overlay.block_warnings
    );
    let cancel = AtomicBool::new(false);
    let mut count = 0;
    for (n, (actual, expected)) in overlay.rendered_features().zip(expected).enumerate() {
        assert_eq!(actual.source.handle, expected["handle"].as_str().unwrap());
        assert_eq!(
            actual.source.raw_text,
            expected["content"].as_str().unwrap()
        );
        assert_eq!(actual.layer, expected["layer"].as_str().unwrap());
        assert_eq!(actual.rgb, rgb(&expected["rgb"]));
        assert_eq!(actual.visible, expected["visible"].as_bool().unwrap());
        let paths = actual.painted_paths().collect::<Vec<_>>();
        let wanted = expected["paths"].as_array().unwrap();
        assert_eq!(paths.len(), wanted.len(), "scene {n}");
        for ((points, color), expected) in paths.into_iter().zip(wanted) {
            assert_eq!(color, rgb(&expected["rgb"]), "scene {n}");
            let flat = contour(expected)
                .flatten(Affine::IDENTITY, 0.005, 100_000, &cancel)
                .unwrap();
            assert_eq!(points.len(), flat.points.len(), "scene {n}");
            for (a, b) in points.iter().zip(&flat.points) {
                for (a, b) in sleufbase_rust::cad_geometry::xyz(*a)
                    .into_iter()
                    .zip(sleufbase_rust::cad_geometry::xyz(*b))
                {
                    assert!((a - b).abs() < 1e-8, "scene {n}: {a} != {b}");
                }
            }
            count += 1;
        }
    }
    assert_eq!(count, 1607);
}

#[test]
fn real_ascii_binary_all_indexed_rgb_nested_contexts_and_decorations_match_original_callbacks() {
    let fonts = fonts();
    for file in ["paint-ascii.dxf", "paint-binary.dxf"] {
        verify(&overlay(file, fonts.path()));
    }
}

#[test]
fn saved_paints_need_no_fonts_and_visibility_keeps_shared_source_and_scene() {
    let fonts = fonts();
    let overlay = overlay("paint-binary.dxf", fonts.path());
    fonts.close().unwrap();
    let project = NativeProject {
        overlays: vec![overlay],
        ..Default::default()
    };
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("project.json");
    save_project(&path, &project).unwrap();
    let mut restored = read_project(&path).unwrap();
    let overlay = &mut restored.overlays[0];
    verify(overlay);
    let scene = overlay.expanded.clone();
    let block = overlay.blocks["paint"].clone();
    // Every grid copy borrows the same source, despite its own flattened range ends.
    let sources = overlay
        .rendered_features()
        .filter(|d| d.source.handle == block.features[0].handle)
        .map(|d| d.source as *const _)
        .collect::<Vec<_>>();
    assert_eq!(sources.len(), 16);
    assert!(sources.iter().all(|p| std::ptr::eq(*p, &block.features[0])));
    overlay.layers.get_mut("Blue").unwrap().visible = false;
    overlay.refresh_block_visibility();
    assert!(Arc::ptr_eq(&scene, &overlay.expanded));
    assert!(Arc::ptr_eq(&block, &overlay.blocks["paint"]));
    assert!(overlay
        .rendered_features()
        .filter(|d| d.layer == "Blue")
        .all(|d| !d.visible));
    overlay.layers.get_mut("Blue").unwrap().visible = true;
    overlay.refresh_block_visibility();
    verify(overlay);
}

#[test]
fn corrupt_paint_ranges_never_replace_saved_project_and_older_uniform_sources_still_read() {
    let fonts = fonts();
    let project = NativeProject {
        overlays: vec![overlay("paint-ascii.dxf", fonts.path())],
        ..Default::default()
    };
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("project.json");
    save_project(&path, &project).unwrap();
    let before = std::fs::read(&path).unwrap();
    for case in 0..5 {
        let mut changed = project.clone();
        let f = &mut changed.overlays[0].features[0];
        let source = f.cad.as_mut().unwrap().mtext.as_mut().unwrap();
        assert!(source.paints.len() > 2);
        match case {
            0 => source.paints[0].paint = Paint::Indexed(257),
            1 => source.paints[0].end_kind += 1,
            2 => source.vertex_ends[0] = 1,
            3 => {
                source.vertex_ends.pop();
            }
            _ => *source.vertex_ends.last_mut().unwrap() += 1,
        }
        assert!(save_project(&path, &changed).is_err(), "case {case}");
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    // Previous project files omitted both fields, retaining their uniform cache.
    let mut old: Value = serde_json::from_slice(&before).unwrap();
    fn remove(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                fields.remove("paints");
                fields.remove("vertex_ends");
                for child in fields.values_mut() {
                    remove(child);
                }
            }
            Value::Array(children) => {
                for child in children {
                    remove(child);
                }
            }
            _ => (),
        }
    }
    remove(&mut old);
    let legacy = temp.path().join("legacy.json");
    std::fs::write(&legacy, serde_json::to_vec(&old).unwrap()).unwrap();
    let old = read_project(&legacy).unwrap();
    assert!(old.overlays[0]
        .rendered_features()
        .all(|d| d.painted_paths().all(|(_, rgb)| rgb == d.rgb)));
}

#[test]
fn clipped_template_map_strokes_keep_each_visible_path_color() {
    let fonts = fonts();
    let overlay = overlay("paint-ascii.dxf", fonts.path());
    let expected = overlay
        .rendered_features()
        .filter(|d| d.visible)
        .flat_map(|d| {
            d.painted_paths()
                .flat_map(|(p, c)| {
                    p.windows(2)
                        .filter(|w| w[0].x != w[1].x || w[0].y != w[1].y)
                        .map(move |_| c)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut strokes = Vec::new();
    sleufbase_rust::template_cad::append_visible(
        &[overlay],
        Bounds {
            min_x: 160000.,
            min_y: 390000.,
            max_x: 180000.,
            max_y: 410000.,
        },
        &mut strokes,
        &Control::default(),
    )
    .unwrap();
    assert_eq!(
        strokes.iter().map(|s| s.color).collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn desktop_template_preparation_and_written_dxf_preserve_each_path_true_color() {
    use sleufbase_rust::{
        dxf_document::Document, export_worker::Inputs, geo::GeoTransform, geotiff::GeoTiffInfo,
        template_geometry, template_order::State, template_workflow,
    };
    let fonts = fonts();
    let overlay = overlay("paint-binary.dxf", fonts.path());
    let expected = overlay
        .rendered_features()
        .filter(|d| d.visible)
        .flat_map(|d| {
            d.painted_paths()
                .filter(|(p, _)| p.len() >= 2)
                .map(|(_, c)| c)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let transform = GeoTransform {
        a: 1.,
        b: 0.,
        c: 168000.,
        d: 0.,
        e: -1.,
        f: 401000.,
    };
    let inputs = Inputs {
        overlays: vec![overlay],
        rasters: vec![GeoTiffInfo {
            path: "source.tif".into(),
            width: 100,
            height: 100,
            transform,
            bounds: transform.bounds(100, 100).unwrap(),
            epsg: Some(28992),
            kickthemap_job_id: None,
            kickthemap_job_title: String::new(),
            kickthemap_job_sidecar: None,metadata:Default::default(),
        }],
        ..Default::default()
    };
    let mut state = State {
        aerial: false,
        auto_bgt: false,
        ..Default::default()
    };
    state.sync(&inputs.rasters).unwrap();
    let control = Control::default();
    let request = template_workflow::request_with_bgt(&state, &inputs, &[], &control, |_| {
        panic!("unexpected network")
    })
    .unwrap();
    let colors = request.slots[0]
        .as_ref()
        .unwrap()
        .map_source
        .as_ref()
        .unwrap()
        .overlays
        .iter()
        .map(|s| s.color)
        .collect::<Vec<_>>();
    assert_eq!(colors, expected);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/templates/empty.dxf");
    let cancel = AtomicBool::new(false);
    let mut doc = Document::load(&path, &cancel).unwrap();
    template_geometry::add(&mut doc, &request.layers, &request.geometry, &cancel).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("colored-export.dxf");
    let mut file = std::fs::File::create(&output).unwrap();
    doc.write_to(&mut file, &cancel).unwrap();
    drop(file);
    let doc = Document::load(&output, &cancel).unwrap();
    let colors = doc
        .records()
        .filter(|r| r.first(8) == Some("PROEFSLEUVEN_KADASTER"))
        .map(|r| r.first(420).unwrap().parse::<u32>().unwrap())
        .collect::<Vec<_>>();
    let wanted = expected
        .into_iter()
        .map(|c| (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]))
        .collect::<Vec<_>>();
    assert_eq!(colors, wanted);
}
