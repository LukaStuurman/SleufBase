//! World-space differential tests against ezdxf, not native self-generated data.
use serde::Deserialize;
use sleufbase_rust::{
    cad_geometry::{Affine, Placement, V3},
    dxf_read::{load_dxf, parse_dxf},
    project::{read_project, save_project, NativeProject},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cad-scene")
}
#[derive(Deserialize)]
struct MatrixCase {
    placement: Placement,
    base: V3,
    local: Vec<V3>,
    expected: Vec<Vec<V3>>,
}
fn close(a: V3, b: V3) {
    for i in 0..3 {
        assert!((a[i] - b[i]).abs() < 1e-7, "{a:?} != {b:?}");
    }
}
#[test]
fn ocs_insert_base_scale_reflection_rotation_and_grid_match_72_ezdxf_cases() {
    let cases: Vec<MatrixCase> =
        serde_json::from_str(include_str!("fixtures/cad-scene/matrices.json")).unwrap();
    assert_eq!(cases.len(), 72);
    for case in cases {
        for row in 0..case.placement.rows {
            for column in 0..case.placement.columns {
                let matrix = case.placement.matrix(case.base, row, column).unwrap();
                for (local, expected) in case
                    .local
                    .iter()
                    .zip(&case.expected[(row * case.placement.columns + column) as usize])
                {
                    close(matrix.checked_point(*local).unwrap(), *expected);
                }
            }
        }
    }
}
#[derive(Deserialize)]
struct Primitive {
    id: String,
    path: String,
    layer: String,
    rgb: [u8; 3],
    visible: bool,
    linear: bool,
    points: Vec<V3>,
}
fn xyz(p: &sleufbase_rust::Vertex) -> V3 {
    [p.x, p.y, p.z.unwrap_or(0.)]
}
fn distance(p: V3, a: V3, b: V3) -> f64 {
    let delta = std::array::from_fn::<_, 3, _>(|i| b[i] - a[i]);
    let v = std::array::from_fn::<_, 3, _>(|i| p[i] - a[i]);
    let n = delta.iter().map(|v| v * v).sum::<f64>();
    let t = if n == 0. {
        0.
    } else {
        (v.iter().zip(delta).map(|(a, b)| a * b).sum::<f64>() / n).clamp(0., 1.)
    };
    (0..3)
        .map(|i| (p[i] - a[i] - t * delta[i]).powi(2))
        .sum::<f64>()
        .sqrt()
}
fn verify(overlay: &sleufbase_rust::dxf_read::DxfOverlay) {
    let expected: Vec<Primitive> =
        serde_json::from_str(include_str!("fixtures/cad-scene/expected.json")).unwrap();
    let mut expected: BTreeMap<_, _> = expected
        .into_iter()
        .map(|v| ((v.id.clone(), v.path.clone()), v))
        .collect();
    let indices = overlay
        .features
        .iter()
        .enumerate()
        .filter(|(_, f)| f.entity_type != "INSERT")
        .map(|(i, _)| i)
        .chain(overlay.features.len()..overlay.features.len() + overlay.expanded.features.len());
    let mut count = 0;
    for index in indices {
        let drawing = overlay.inspect_feature(index).unwrap();
        // TEXT now has its own font-independent oracle in cad_text.rs.
        if drawing.source.entity_type=="TEXT" { assert!(!drawing.points.is_empty());continue; }
        let details = overlay.inspect_details(index).unwrap();
        let id = drawing.source.cad.as_deref().unwrap().xdata["SOURCE_ID"][0]
            .value
            .clone();
        let path = details.get("Blokpad").cloned().unwrap_or_default();
        let want = expected
            .remove(&(id.clone(), path.clone()))
            .unwrap_or_else(|| panic!("unexpected {id} in {path}"));
        assert_eq!(drawing.layer, want.layer);
        assert_eq!(drawing.rgb, want.rgb, "{id}: {path}");
        assert_eq!(drawing.visible, want.visible, "{id}: {path}");
        let mut actual: Vec<V3> = drawing.points.iter().map(xyz).collect();
        actual.dedup();
        if want.linear {
            let mut points = want.points;
            points.dedup();
            assert_eq!(actual.len(), points.len(), "{id}");
            for (a, b) in actual.iter().zip(points) {
                close(*a, b);
            }
        } else {
            close(actual[0], want.points[0]);
            close(*actual.last().unwrap(), *want.points.last().unwrap());
            for p in want.points {
                let error = actual
                    .windows(2)
                    .map(|edge| distance(p, edge[0], edge[1]))
                    .fold(f64::INFINITY, f64::min);
                assert!(
                    error <= 0.2500001,
                    "{id} in {path}: WCS chord error {error} exceeds 0.25"
                );
            }
        }
        count += 1;
    }
    assert_eq!(count, 84);
    assert!(expected.is_empty());
    assert!(
        overlay.block_warnings.is_empty(),
        "{:?}",
        overlay.block_warnings
    );
    assert!(!overlay.warnings.iter().any(|w| w.starts_with("TEXT:")));
}
#[test]
fn ascii_and_binary_nested_blocks_bulges_ellipse_and_wcs_geometry_match_ezdxf() {
    for file in ["scene-ascii.dxf", "scene-binary.dxf"] {
        verify(&load_dxf(&root().join(file)).unwrap());
    }
}
#[test]
fn native_project_restores_derived_geometry_and_layer_visibility_without_serializing_cache() {
    let overlay = load_dxf(&root().join("scene-binary.dxf")).unwrap();
    let project = NativeProject {
        overlays: vec![overlay],
        ..Default::default()
    };
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("project.json");
    save_project(&path, &project).unwrap();
    let loaded = read_project(&path).unwrap();
    verify(&loaded.overlays[0]);
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert!(json["overlays"][0].get("expanded").is_none());
    assert!(json["overlays"][0].get("block_visibility").is_none());
    let mut overlay = loaded.overlays[0].clone();
    let cache = overlay.expanded.clone();
    let visible = overlay.rendered_features().filter(|d| d.visible).count();
    overlay.layers.get_mut("Off").unwrap().visible = true;
    overlay.refresh_block_visibility();
    assert_eq!(
        overlay.rendered_features().filter(|d| d.visible).count(),
        visible + 8
    );
    assert!(
        Arc::ptr_eq(&cache, &overlay.expanded),
        "a visibility change must not copy/rebuild geometry"
    );
    for layer in overlay.layers.values_mut() {
        layer.visible = false;
    }
    overlay.layers.get_mut("Off").unwrap().visible = true;
    overlay.refresh_block_visibility();
    let bounds = overlay.bounds().unwrap();
    assert!(bounds.max_x < 1000. && bounds.max_y < 1000.);
    overlay.layers.get_mut("Off").unwrap().visible = false;
    overlay.refresh_block_visibility();
    assert!(overlay.bounds().is_none());
    assert!(Arc::ptr_eq(&cache, &overlay.expanded));
}
#[test]
fn recursive_missing_or_excessive_blocks_report_warnings_without_losing_measurement_sources() {
    let make = |blocks: &str, name: &str, extra: &str| {
        format!("0\nSECTION\n2\nBLOCKS\n{blocks}0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nINSERT\n2\n{name}\n10\n1\n20\n2\n{extra}0\nENDSEC\n0\nEOF\n")
    };
    let recursive =
        "0\nBLOCK\n2\nLoop\n10\n0\n20\n0\n0\nINSERT\n2\nLoop\n10\n0\n20\n0\n0\nENDBLK\n";
    let empty = "0\nBLOCK\n2\nEmpty\n10\n0\n20\n0\n0\nENDBLK\n";
    for text in [
        make(recursive, "Loop", ""),
        make("", "Missing", ""),
        make(empty, "Empty", "70\n65535\n71\n65535\n44\n1\n45\n1\n"),
    ] {
        let overlay = parse_dxf(&text, Path::new("limit.dxf")).unwrap();
        assert_eq!(overlay.features.len(), 1);
        assert_eq!(overlay.features[0].entity_type, "INSERT");
        assert!(!overlay.block_warnings.is_empty());
        assert_eq!(overlay.expanded.features.len(), 1);
    }
    assert!(Affine::ocs([0.; 3]).is_err());
}

#[test]
fn ten_thousand_instances_share_large_source_metadata_and_immutable_snapshot_cache() {
    let mut text="0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\nB\n10\n0\n20\n0\n0\nLINE\n10\n0\n20\n0\n11\n1\n21\n1\n1001\nPAYLOAD\n".to_owned();
    for _ in 0..256 {
        text.push_str("1000\n");
        text.push_str(&"x".repeat(255));
        text.push('\n');
    }
    text.push_str("0\nENDBLK\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    for index in 0..10_000 {
        use std::fmt::Write;
        writeln!(&mut text, "0\nINSERT\n2\nB\n10\n{index}\n20\n0").unwrap();
    }
    text.push_str("0\nENDSEC\n0\nEOF\n");
    let overlay = parse_dxf(&text, Path::new("shared.dxf")).unwrap();
    assert_eq!(overlay.expanded.features.len(), 10_000);
    assert!(overlay.block_warnings.is_empty());
    let pointer = overlay.blocks["b"].features[0].cad.as_ref().unwrap().xdata["PAYLOAD"][0]
        .value
        .as_ptr();
    for drawing in overlay.rendered_features() {
        assert_eq!(
            drawing.source.cad.as_ref().unwrap().xdata["PAYLOAD"][0]
                .value
                .as_ptr(),
            pointer
        );
    }
    let snapshot = overlay.clone();
    assert!(Arc::ptr_eq(&snapshot.expanded, &overlay.expanded));
    assert!(Arc::ptr_eq(&snapshot.blocks["b"], &overlay.blocks["b"]));
}

#[test]
fn polyface_face_indices_without_coordinates_are_rejected_instead_of_inventing_vertices() {
    let text = "0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n70\n64\n0\nVERTEX\n70\n128\n71\n1\n72\n2\n73\n3\n0\nSEQEND\n0\nLINE\n10\n1\n20\n2\n11\n3\n21\n4\n0\nENDSEC\n0\nEOF\n";
    assert!(parse_dxf(text, Path::new("polyface.dxf"))
        .unwrap_err()
        .contains("vlakindices"));
}
