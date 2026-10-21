use serde::Deserialize;
use sleufbase_rust::{
    cad_geometry::{self as g, Affine, V3},
    cad_text::{Renderer, Source},
    cad_text_path::Path as Contours,
    dxf_fonts::Catalog,
    dxf_read::{parse_dxf_bytes_with_catalog, DxfOverlay},
    dxf_text_metrics::Text,
    project::{read_project, save_project, NativeProject},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
#[derive(Clone, Debug, Deserialize)]
struct Command {
    kind: u8,
    points: Vec<V3>,
}
#[derive(Clone, Debug, Deserialize)]
struct Ink {
    start: V3,
    commands: Vec<Command>,
}
#[derive(Deserialize)]
struct Case {
    font: String,
    text: Text,
    paths: Vec<Ink>,
}
#[derive(Deserialize)]
struct Instance {
    handle: String,
    blockpath: String,
    layer: String,
    visible: bool,
    paths: Vec<Ink>,
}
#[derive(Deserialize)]
struct Reference {
    files: Vec<String>,
    cases: Vec<Case>,
    scene: Vec<Instance>,
}
fn reference() -> Reference {
    serde_json::from_str(include_str!("fixtures/cad-text/reference.json")).unwrap()
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}
fn fonts() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for file in reference().files {
        let from = root().join(file);
        std::fs::copy(&from, dir.path().join(from.file_name().unwrap())).unwrap();
    }
    dir
}
fn close(a: V3, b: V3) {
    for i in 0..3 {
        assert!(
            (a[i] - b[i]).abs() <= 1e-10 + 128. * f64::EPSILON * a[i].abs().max(b[i].abs()),
            "{a:?} != {b:?}"
        );
    }
}
fn ink(path: &Contours) -> Vec<Ink> {
    path.validate().unwrap();
    let mut out = Vec::new();
    let mut current = None;
    let mut at = 0;
    for &kind in &path.kinds {
        let n = usize::from(kind.max(1));
        if kind == 0 {
            if let Some(p) = current.take() {
                let p: Ink = p;
                if !p.commands.is_empty() {
                    out.push(p);
                }
            }
            current = Some(Ink {
                start: path.points[at],
                commands: Vec::new(),
            });
        } else {
            current.as_mut().unwrap().commands.push(Command {
                kind,
                points: path.points[at..at + n].to_vec(),
            });
        }
        at += n;
    }
    if let Some(p) = current {
        if !p.commands.is_empty() {
            out.push(p);
        }
    }
    out
}
fn compare(actual: &[Ink], expected: &[Ink], name: &str) {
    assert_eq!(actual.len(), expected.len(), "{name}: contour count");
    for (a, b) in actual.iter().zip(expected) {
        close(a.start, b.start);
        assert_eq!(a.commands.len(), b.commands.len(), "{name}: segment count");
        for (a, b) in a.commands.iter().zip(&b.commands) {
            assert_eq!(a.kind, b.kind, "{name}: curve type");
            assert_eq!(a.points.len(), b.points.len());
            for (&a, &b) in a.points.iter().zip(&b.points) {
                close(a, b);
            }
        }
    }
}
#[test]
fn all_675_ttf_shp_shx_lff_and_monospace_vector_paths_match_independent_original() {
    let dir = fonts();
    let cancel = AtomicBool::new(false);
    let mut catalog = Catalog::from_dirs(&[dir.path().to_owned()], &cancel).unwrap();
    let mut renderer = Renderer::new(&mut catalog, &cancel);
    let r = reference();
    assert_eq!(r.cases.len(), 675);
    for (i, case) in r.cases.into_iter().enumerate() {
        let mut source = Source {
            layout: case.text,
            style: "Standard".into(),
            font: case.font.clone(),
            big_font: String::new(),
            contours: Default::default(),
        };
        renderer
            .prepare(&mut source)
            .unwrap_or_else(|err| panic!("{i} {}: {err}", case.font));
        compare(
            &ink(&source.contours),
            &case.paths,
            &format!("{i} {}", case.font),
        );
    }
}
fn overlay(file: &str, dir: &Path) -> DxfOverlay {
    let cancel = AtomicBool::new(false);
    let mut catalog = Catalog::from_dirs(&[dir.to_owned()], &cancel).unwrap();
    let path = root().join("cad-text").join(file);
    parse_dxf_bytes_with_catalog(&std::fs::read(&path).unwrap(), &path, &cancel, &mut catalog)
        .unwrap()
}
fn distance(p: V3, a: V3, b: V3) -> f64 {
    let delta = g::sub(b, a);
    let v = g::sub(p, a);
    let n = g::dot(delta, delta);
    let t = if n == 0. {
        0.
    } else {
        (g::dot(v, delta) / n).clamp(0., 1.)
    };
    g::length(g::sub(v, g::scale(delta, t)))
}
fn verify_scene(overlay: &DxfOverlay) {
    assert!(overlay.warnings.is_empty(), "{:?}", overlay.warnings);
    assert!(
        overlay.block_warnings.is_empty(),
        "{:?}",
        overlay.block_warnings
    );
    let mut expected: BTreeMap<_, _> = reference()
        .scene
        .into_iter()
        .map(|s| ((s.handle.clone(), s.blockpath.clone()), s))
        .collect();
    assert_eq!(expected.len(), 25);
    let indices = overlay
        .features
        .iter()
        .enumerate()
        .filter(|(_, f)| f.entity_type != "INSERT")
        .map(|(i, _)| i)
        .chain(overlay.features.len()..overlay.features.len() + overlay.expanded.features.len());
    for i in indices {
        let draw = overlay.inspect_feature(i).unwrap();
        let details = overlay.inspect_details(i).unwrap();
        let path = details.get("Blokpad").cloned().unwrap_or_default();
        let expected = expected
            .remove(&(draw.source.handle.clone(), path))
            .unwrap();
        assert_eq!(draw.visible, expected.visible);
        assert_eq!(draw.layer, expected.layer);
        if draw.layer == "Root" {
            assert_eq!(draw.rgb, [0x31, 0x7a, 0xbc]);
        }
        let paths: Vec<_> = draw.paths().collect();
        assert_eq!(paths.len(), expected.paths.len());
        for (actual, want) in paths.iter().zip(&expected.paths) {
            close(g::xyz(actual[0]), want.start);
            let mut from = want.start;
            for c in &want.commands {
                let mut p = vec![from];
                p.extend_from_slice(&c.points);
                for sample in 0..=64 {
                    let mut q = p.clone();
                    let t = sample as f64 / 64.;
                    for n in (1..q.len()).rev() {
                        for j in 0..n {
                            q[j] = g::add(g::scale(q[j], 1. - t), g::scale(q[j + 1], t));
                        }
                    }
                    let error = actual
                        .windows(2)
                        .map(|e| distance(q[0], g::xyz(e[0]), g::xyz(e[1])))
                        .fold(f64::INFINITY, f64::min);
                    assert!(error <= 0.00500001, "WCS glyph chord error {error}");
                }
                from = *c.points.last().unwrap();
            }
            close(g::xyz(*actual.last().unwrap()), from);
        }
    }
    assert!(expected.is_empty());
}
#[test]
fn ascii_binary_nested_nonuniform_rotated_reflected_ocs_and_grid_text_reach_the_scene() {
    let dir = fonts();
    for file in ["text-ascii.dxf", "text-binary.dxf"] {
        verify_scene(&overlay(file, dir.path()));
    }
}
#[test]
fn project_roundtrip_keeps_glyphs_without_the_font_files_and_visibility_keeps_shared_cache() {
    let dir = fonts();
    let mut overlay = overlay("text-binary.dxf", dir.path());
    dir.close().unwrap();
    let before = serde_json::to_value(&overlay).unwrap();
    let cache = overlay.expanded.clone();
    let visible = overlay.rendered_features().filter(|f| f.visible).count();
    overlay.layers.get_mut("Off").unwrap().visible = true;
    overlay.refresh_block_visibility();
    assert_eq!(
        overlay.rendered_features().filter(|f| f.visible).count(),
        visible + 12
    );
    assert!(std::sync::Arc::ptr_eq(&cache, &overlay.expanded));
    overlay.layers.get_mut("Off").unwrap().visible = false;
    overlay.refresh_block_visibility();
    let project = NativeProject {
        overlays: vec![overlay],
        ..Default::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.json");
    save_project(&path, &project).unwrap();
    let project = read_project(&path).unwrap();
    assert_eq!(serde_json::to_value(&project.overlays[0]).unwrap(), before);
    verify_scene(&project.overlays[0]);
}
#[test]
fn curve_flattening_preserves_pen_up_and_checks_world_error_budget_and_cancellation() {
    let path = Contours {
        kinds: vec![0, 3, 0, 1, 0, 0],
        points: vec![
            [0., 0., 0.],
            [0., 1., 0.],
            [1., 1., 0.],
            [1., 0., 0.],
            [10., 0., 0.],
            [11., 0., 0.],
            [500., 0., 0.],
            [600., 0., 0.],
        ],
    };
    let cancel = AtomicBool::new(false);
    let matrix = Affine([[100., 0., 0., 0.], [0., 0.1, 0., 0.], [0., 0., 1., 0.]]);
    let flat = path.flatten(matrix, 0.001, 1000, &cancel).unwrap();
    assert_eq!(flat.breaks.len(), 1);
    assert_eq!(flat.points.last().unwrap().x, 1100.);
    assert!(flat.points.len() > 3);
    assert!(path
        .flatten(matrix, 0.001, 2, &cancel)
        .unwrap_err()
        .contains("budget"));
    assert!(path.flatten(matrix, 0., 1000, &cancel).is_err());
    assert!(Contours {
        kinds: vec![3],
        points: vec![[0.; 3]; 3]
    }
    .validate()
    .is_err());
    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(path.flatten(matrix, 0.001, 1000, &cancel).is_err());
}

#[test]
fn real_map_overlay_contains_only_visible_glyph_segments_and_never_pen_up_bridges() {
    let dir = fonts();
    let mut overlay = overlay("text-ascii.dxf", dir.path());
    overlay.features.retain(|f| f.entity_type == "TEXT");
    overlay.blocks.clear();
    overlay.rebuild_block_cache().unwrap();
    let b = overlay.bounds().unwrap();
    let b = sleufbase_rust::Bounds {
        min_x: b.min_x - 1.,
        min_y: b.min_y - 1.,
        max_x: b.max_x + 1.,
        max_y: b.max_y + 1.,
    };
    let expected: Vec<_> = overlay
        .rendered_features()
        .flat_map(|f| {
            f.paths()
                .flat_map(|p| {
                    p.windows(2)
                        .map(|e| vec![[e[0].x, e[0].y], [e[1].x, e[1].y]])
                })
                .collect::<Vec<_>>()
        })
        .filter(|p| p[0] != p[1])
        .collect();
    let mut out = Vec::new();
    let control = sleufbase_rust::export_control::Control::default();
    sleufbase_rust::template_cad::append_visible(
        std::slice::from_ref(&overlay),
        b,
        &mut out,
        &control,
    )
    .unwrap();
    assert_eq!(
        out.iter().map(|s| s.points.clone()).collect::<Vec<_>>(),
        expected
    );
    overlay.visible = false;
    out.clear();
    sleufbase_rust::template_cad::append_visible(&[overlay], b, &mut out, &control).unwrap();
    assert!(out.is_empty());
}

#[test]
fn invalid_saved_text_never_replaces_a_valid_project_and_cancelled_import_publishes_nothing() {
    let fonts = fonts();
    let overlay = overlay("text-ascii.dxf", fonts.path());
    let mut project = NativeProject {
        overlays: vec![overlay],
        ..Default::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.json");
    save_project(&path, &project).unwrap();
    let before = std::fs::read(&path).unwrap();
    let text = project.overlays[0]
        .features
        .iter_mut()
        .find(|f| f.entity_type == "TEXT")
        .unwrap()
        .cad
        .as_deref_mut()
        .unwrap()
        .text
        .as_deref_mut()
        .unwrap();
    text.contours.kinds[0] = 3;
    assert!(save_project(&path, &project).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let cancel = AtomicBool::new(false);
    let mut catalog = Catalog::from_dirs(&[fonts.path().to_owned()], &cancel).unwrap();
    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    let bytes = std::fs::read(root().join("cad-text/text-ascii.dxf")).unwrap();
    assert!(
        parse_dxf_bytes_with_catalog(&bytes, Path::new("cancel.dxf"), &cancel, &mut catalog)
            .is_err()
    );
}

#[test]
fn invisible_glyph_programs_still_consume_the_drawing_work_budget() {
    // Many cheap aliases execute one expensive subshape that emits no points.
    // A point-only budget cannot bound this input; caching each distinct code
    // cannot hide its interpreter work either.
    let dir = tempfile::tempdir().unwrap();
    let mut data=String::from("*UNIFONT,6,Own invisible-work regression\n21,5,2,0,0,0\n*65,0,A\n8,9,21,8,-9,-21,0\n*120,0,x\n8,8,12,0\n*112,0,p\n8,8,-7,0\n*66,0,Expensive pen-up\n");
    data.push_str(&"3,1,".repeat(35_000));
    data.push_str("2,8,0,0,0\n");
    let mut content = String::new();
    for code in 0x100..0x100 + 600 {
        data.push_str(&format!("*{code},0,Alias\n7,66,0\n"));
        content.push(char::from_u32(code).unwrap());
    }
    std::fs::write(dir.path().join("work.shp"), data).unwrap();
    let cancel = AtomicBool::new(false);
    let mut catalog = Catalog::from_dirs(&[dir.path().to_owned()], &cancel).unwrap();
    let mut renderer = Renderer::new(&mut catalog, &cancel);
    let mut source = Source {
        layout: Text {
            content,
            ..Default::default()
        },
        style: "Standard".into(),
        font: "work.shp".into(),
        big_font: String::new(),
        contours: Default::default(),
    };
    let err = renderer.prepare(&mut source).unwrap_err();
    assert!(err.contains("glyph-werkbudget"), "{err}");
    assert!(
        source.contours.points.is_empty(),
        "failure must not publish partial contours"
    );
}

#[test]
fn omitted_text_height_uses_dxf_default_and_partial_styles_are_reported() {
    let data=b"0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nSTYLE\n0\nSTYLE\n2\nOwn\n3\n*monospace\n4\nextra-big.shx\n70\n4\n0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nTEXT\n5\nF\n7\nOwn\n10\n0\n20\n0\n1\nLabel\n0\nENDSEC\n0\nEOF\n";
    let cancel = AtomicBool::new(false);
    let dir = tempfile::tempdir().unwrap();
    let mut catalog = Catalog::from_dirs(&[dir.path().to_owned()], &cancel).unwrap();
    let overlay =
        parse_dxf_bytes_with_catalog(data, Path::new("style.dxf"), &cancel, &mut catalog).unwrap();
    let text = overlay.features[0]
        .cad
        .as_deref()
        .unwrap()
        .text
        .as_deref()
        .unwrap();
    assert_eq!(text.layout.height, 2.5);
    assert_eq!(text.style, "Own");
    assert_eq!(text.font, "*monospace");
    assert_eq!(text.big_font, "extra-big.shx");
    assert_eq!(overlay.bounds().unwrap().max_y, 2.5);
    assert!(overlay.warnings.iter().any(|w| w.contains("verticale")));
    assert!(overlay.warnings.iter().any(|w| w.contains("BIGFONT")));
    let details = overlay.features[0].details(Path::new("style.dxf"));
    assert_eq!(details["Tekst"], "Label");
    assert_eq!(details["Tekststijl"], "Own");
}
