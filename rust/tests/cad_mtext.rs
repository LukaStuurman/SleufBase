#[path = "common/mtext_render.rs"]
#[allow(dead_code)]
mod common;
use serde_json::{json, Value};
use sleufbase_rust::{
    cad_text_path::Path as Contours,
    dxf_read::{parse_dxf_bytes_with_catalog, DxfOverlay},
    project::{read_project, save_project, NativeProject},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cad-mtext")
}
fn reference() -> Value {
    serde_json::from_str(include_str!("fixtures/cad-mtext/reference.json")).unwrap()
}
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
fn overlay(name: &str, fonts: &Path) -> DxfOverlay {
    let cancel = AtomicBool::new(false);
    let mut catalog = common::catalog(fonts, &cancel).unwrap();
    let path = root().join(name);
    parse_dxf_bytes_with_catalog(&std::fs::read(&path).unwrap(), &path, &cancel, &mut catalog)
        .unwrap()
}
fn segments(path: &Contours) -> Value {
    path.validate().unwrap();
    let mut output = Vec::new();
    let mut at = 0usize;
    let mut current = [0.; 3];
    for &kind in &path.kinds {
        let n = usize::from(kind.max(1));
        if kind != 0 {
            output.push(json!({"kind":kind,"start":current,"points":path.points[at..at+n]}));
        }
        current = path.points[at + n - 1];
        at += n;
    }
    json!(output)
}
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert!(
                (a - b).abs() <= 1e-9 + 64. * f64::EPSILON * a.abs().max(b.abs()),
                "{path}: {a} != {b}"
            );
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, b) in b {
                compare(&a[k], b, &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
fn verify(overlay: &DxfOverlay) {
    let r = reference();
    let expected: BTreeMap<_, _> = r["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["handle"].as_str().unwrap(), e))
        .collect();
    let mut found = 0;
    for f in overlay
        .features
        .iter()
        .chain(overlay.blocks.values().flat_map(|b| b.features.iter()))
    {
        let Some(source) = f.cad.as_deref().and_then(|p| p.mtext.as_deref()) else {
            continue;
        };
        found += 1;
        let expected = expected[f.handle.as_str()];
        assert_eq!(
            source.request.content,
            expected["content"].as_str().unwrap()
        );
        assert_eq!(source.style, expected["style"].as_str().unwrap());
        assert!(source.entity.is_some());
        compare(
            &segments(&source.contours),
            &expected["segments"],
            &format!("MTEXT {}", f.handle),
        );
    }
    assert_eq!(found, 129);
    let corrected = r["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["nbsp_correction"] == true)
        .collect::<Vec<_>>();
    assert_eq!(corrected.len(), 4);
    for c in corrected {
        assert_ne!(c["legacy_segments"], c["segments"]);
    }
    assert_eq!(overlay.expanded.features.len(), 12);
    assert_eq!(overlay.rendered_features().count(), 138);
    for (n, (found, expected)) in overlay
        .rendered_features()
        .zip(r["scene"].as_array().unwrap())
        .enumerate()
    {
        assert_eq!(found.source.handle, expected["handle"].as_str().unwrap());
        for (name, p) in [
            ("first", found.points.first()),
            ("last", found.points.last()),
        ] {
            let value = p
                .copied()
                .map(sleufbase_rust::cad_geometry::xyz)
                .map(|p| json!(p))
                .unwrap_or(Value::Null);
            compare(&value, &expected[name], &format!("scene {n} {name}"));
        }
    }
    assert!(overlay
        .warnings
        .iter()
        .any(|w| w.contains("achtergrond")));
}
#[test]
fn ascii_binary_real_import_uses_both_routes_style_width_oblique_and_automatic_font_width() {
    let fonts = fonts();
    for file in ["mtext-ascii.dxf", "mtext-binary.dxf"] {
        verify(&overlay(file, fonts.path()));
    }
}
#[test]
fn real_source_project_roundtrip_is_font_independent_and_invalid_updates_preserve_saved_bytes() {
    let fonts = fonts();
    let overlay = overlay("mtext-binary.dxf", fonts.path());
    fonts.close().unwrap();
    let mut project = NativeProject {
        overlays: vec![overlay],
        ..Default::default()
    };
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("project.json");
    save_project(&path, &project).unwrap();
    let before = std::fs::read(&path).unwrap();
    let restored = read_project(&path).unwrap();
    verify(&restored.overlays[0]);
    let f = project.overlays[0]
        .features
        .iter_mut()
        .find(|f| f.cad.as_deref().is_some_and(|p| p.mtext.is_some()))
        .unwrap();
    f.cad
        .as_deref_mut()
        .unwrap()
        .mtext
        .as_deref_mut()
        .unwrap()
        .request
        .matrix[0][0] = f64::INFINITY;
    assert!(save_project(&path, &project).is_err());
    assert_eq!(std::fs::read(path).unwrap(), before);
}
