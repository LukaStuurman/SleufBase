//! Attached attribute geometry vs original ezdxf and an explicit grid fix.
use serde_json::Value;
use sleufbase_rust::{
    cad_geometry::{self as g, V3},
    dxf_fonts::Catalog,
    dxf_read::{parse_dxf_bytes_with_catalog, DxfOverlay},
    project::{read_project, save_project, NativeProject},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};
fn reference() -> Value {
    serde_json::from_str(include_str!("fixtures/cad-attrib/reference.json")).unwrap()
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}
fn fonts() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for file in reference()["files"].as_array().unwrap() {
        let p = root().join(file.as_str().unwrap());
        std::fs::copy(&p, dir.path().join(p.file_name().unwrap())).unwrap();
    }
    dir
}
fn load(file: &str, fonts: &Path) -> DxfOverlay {
    let cancel = AtomicBool::new(false);
    let mut catalog = Catalog::from_dirs(&[fonts.to_owned()], &cancel).unwrap();
    let path = root().join("cad-attrib").join(file);
    parse_dxf_bytes_with_catalog(&std::fs::read(&path).unwrap(), &path, &cancel, &mut catalog)
        .unwrap()
}
fn point(v: &Value) -> V3 {
    std::array::from_fn(|i| v[i].as_f64().unwrap())
}
fn close(a: V3, b: V3) {
    for i in 0..3 {
        assert!(
            (a[i] - b[i]).abs() <= 1e-8 + 128. * f64::EPSILON * a[i].abs().max(b[i].abs()),
            "{a:?} != {b:?}"
        );
    }
}
fn distance(p: V3, a: V3, b: V3) -> f64 {
    let d = g::sub(b, a);
    let v = g::sub(p, a);
    let n = g::dot(d, d);
    let t = if n == 0. {
        0.
    } else {
        (g::dot(v, d) / n).clamp(0., 1.)
    };
    g::length(g::sub(v, g::scale(d, t)))
}
fn verify(overlay: &DxfOverlay) {
    assert!(
        overlay.block_warnings.is_empty(),
        "{:?}",
        overlay.block_warnings
    );
    assert_eq!(overlay.warnings.len(), 1);
    assert!(overlay.warnings[0].contains("MTEXT"));
    let r = reference();
    let mut expected: BTreeMap<_, _> = r["scene"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                (
                    s["handle"].as_str().unwrap().to_owned(),
                    s["blockpath"].as_str().unwrap().to_owned(),
                ),
                s,
            )
        })
        .collect();
    assert_eq!(expected.len(), 774);
    assert_eq!(r["grid_corrections"].as_array().unwrap().len(), 240);
    let mut attrs = 0;
    for i in overlay.features.len()..overlay.features.len() + overlay.expanded.features.len() {
        let draw = overlay.inspect_feature(i).unwrap();
        let details = overlay.inspect_details(i).unwrap();
        let key = (
            draw.source.handle.clone(),
            details.get("Blokpad").cloned().unwrap_or_default(),
        );
        let want = expected
            .remove(&key)
            .unwrap_or_else(|| panic!("Unexpected {key:?}"));
        assert_eq!(draw.source.entity_type, want["kind"].as_str().unwrap());
        assert_eq!(draw.layer, want["layer"].as_str().unwrap());
        assert_eq!(draw.visible, want["visible"].as_bool().unwrap(), "{key:?}");
        assert_eq!(
            draw.rgb,
            std::array::from_fn::<_, 3, _>(|j| want["rgb"][j].as_u64().unwrap() as u8)
        );
        if draw.source.entity_type == "LINE" {
            for (p, w) in draw.points.iter().zip(want["points"].as_array().unwrap()) {
                close(g::xyz(*p), point(w));
            }
            continue;
        }
        attrs += 1;
        let text = draw.source.cad.as_deref().unwrap().text.as_deref().unwrap();
        let a = text.attribute.as_deref().unwrap();
        assert_eq!(a.tag, want["tag"]);
        assert_eq!(a.flags as u64, want["flags"].as_u64().unwrap());
        assert_eq!(
            a.field_length as u64,
            want["field_length"].as_u64().unwrap()
        );
        assert_eq!(a.lock_position, want["lock_position"].as_bool().unwrap());
        assert_eq!(draw.source.raw_text, want["content"]);
        assert_eq!(details["Attribuutnaam"], a.tag);
        let paths: Vec<_> = draw.paths().filter(|p| !p.is_empty()).collect();
        let ink = want["paths"].as_array().unwrap();
        assert_eq!(paths.len(), ink.len(), "{key:?}");
        if a.multiline() {
            assert!(draw.points.is_empty());
            assert!(a
                .extra
                .iter()
                .any(|t| t.code == 101 && t.value == "Embedded Object"));
        }
        for (actual, want) in paths.iter().zip(ink) {
            let mut from = point(&want["start"]);
            close(g::xyz(actual[0]), from);
            for c in want["commands"].as_array().unwrap() {
                let mut controls = vec![from];
                controls.extend(c["points"].as_array().unwrap().iter().map(point));
                for sample in 0..=32 {
                    let mut p = controls.clone();
                    let t = sample as f64 / 32.;
                    for n in (1..p.len()).rev() {
                        for j in 0..n {
                            p[j] = g::add(g::scale(p[j], 1. - t), g::scale(p[j + 1], t));
                        }
                    }
                    let err = actual
                        .windows(2)
                        .map(|e| distance(p[0], g::xyz(e[0]), g::xyz(e[1])))
                        .fold(f64::INFINITY, f64::min);
                    assert!(err <= 0.00500001, "{key:?}: WCS chord error {err}");
                }
                from = *controls.last().unwrap();
            }
            close(g::xyz(*actual.last().unwrap()), from);
        }
    }
    assert_eq!(attrs, 471);
    assert!(expected.is_empty());
    let mut parents: BTreeMap<_, _> = overlay
        .features
        .iter()
        .chain(overlay.blocks.values().flat_map(|b| b.features.iter()))
        .filter(|f| !f.linked_attributes().is_empty())
        .map(|f| (f.handle.as_str(), f))
        .collect();
    for p in r["parents"].as_array().unwrap() {
        let parent = parents.remove(p["handle"].as_str().unwrap()).unwrap();
        assert_eq!(
            parent.linked_attributes().len() as u64,
            p["count"].as_u64().unwrap()
        );
        assert_eq!(
            serde_json::to_value(&parent.cad.as_deref().unwrap().attributes).unwrap(),
            p["attributes"]
        );
    }
    assert!(parents.is_empty());
    let b = overlay.bounds().unwrap();
    assert!(
        [b.min_x, b.min_y, b.max_x, b.max_y]
            .iter()
            .all(|v| v.abs() < 1e6),
        "Hidden distant ATTRIB must not distort fit"
    );
}
#[test]
fn ascii_binary_471_attached_attributes_match_all_alignments_fonts_ocs_and_parent_spaces() {
    let fonts = fonts();
    for file in ["attributes-ascii.dxf", "attributes-binary.dxf"] {
        verify(&load(file, fonts.path()));
    }
}
#[test]
fn project_preserves_order_duplicates_raw_multiline_data_and_geometry_without_font_files() {
    let fonts = fonts();
    let overlay = load("attributes-binary.dxf", fonts.path());
    fonts.close().unwrap();
    let before = serde_json::to_value(&overlay).unwrap();
    let project = NativeProject {
        overlays: vec![overlay],
        ..Default::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.json");
    save_project(&path, &project).unwrap();
    let restored = read_project(&path).unwrap();
    assert_eq!(serde_json::to_value(&restored.overlays[0]).unwrap(), before);
    verify(&restored.overlays[0]);
}
#[test]
fn attribute_layer_case_and_all_visibility_flags_use_the_shared_parent_instance_chain() {
    let fonts = fonts();
    let mut overlay = load("attributes-ascii.dxf", fonts.path());
    let cache = overlay.expanded.clone();
    let visible = overlay.rendered_features().filter(|f| f.visible).count();
    assert!(visible > 100);
    let attr = overlay.features[0]
        .cad
        .as_deref_mut()
        .unwrap()
        .linked_attributes
        .first_mut()
        .unwrap();
    let handle = attr.handle.clone();
    let cad = attr.cad.as_deref_mut().unwrap();
    cad.invisible = false;
    cad.text
        .as_deref_mut()
        .unwrap()
        .attribute
        .as_deref_mut()
        .unwrap()
        .flags |= 1;
    overlay.refresh_block_visibility();
    assert!(
        overlay
            .rendered_features()
            .filter(|f| f.source.handle == handle)
            .all(|f| !f.visible),
        "attribute flags remain authoritative even if generic visibility differs"
    );
    assert!(overlay
        .rendered_features()
        .any(|f| f.source.layer == "water" && f.layer == "Water" && f.rgb == [0x12, 0x83, 0xdd]));
    for layer in overlay.layers.values_mut() {
        layer.visible = false;
    }
    overlay.refresh_block_visibility();
    assert!(overlay.rendered_features().all(|f| !f.visible));
    assert!(overlay.bounds().is_none());
    assert!(Arc::ptr_eq(&cache, &overlay.expanded));
}

#[test]
fn zero_spacing_large_minsert_counts_once_and_map_export_includes_attribute_ink() {
    let data=b"0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\nB\n10\n0\n20\n0\n0\nENDBLK\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nINSERT\n5\nA\n2\nB\n66\n1\n10\n10000\n20\n20000\n41\n-2\n42\n3\n50\n37\n70\n65535\n71\n65535\n0\nATTRIB\n5\nC\n2\nCODE\n1\nValue\n10\n2\n20\n3\n11\n12\n21\n3\n40\n2.5\n72\n5\n73\n12345\n74\n0\n0\nSEQEND\n0\nENDSEC\n0\nEOF\n";
    let dir = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(false);
    let mut catalog = Catalog::from_dirs(&[dir.path().to_owned()], &cancel).unwrap();
    let overlay =
        parse_dxf_bytes_with_catalog(data, Path::new("grid.dxf"), &cancel, &mut catalog).unwrap();
    assert!(overlay.block_warnings.is_empty());
    assert_eq!(overlay.expanded.features.len(), 1);
    let draw = overlay.rendered_features().next().unwrap();
    assert_eq!(draw.source.entity_type, "ATTRIB");
    assert_eq!(draw.points.len(), 5);
    close(g::xyz(draw.points[0]), [2., 3., 0.]);
    close(g::xyz(draw.points[1]), [12., 3., 0.]);
    let b = sleufbase_rust::Bounds {
        min_x: 1.,
        min_y: 2.,
        max_x: 13.,
        max_y: 7.,
    };
    let mut strokes = Vec::new();
    sleufbase_rust::template_cad::append_visible(&[overlay], b, &mut strokes, &Default::default())
        .unwrap();
    assert_eq!(strokes.len(), 4);
    assert_eq!(strokes[0].points, vec![[2., 3.], [12., 3.]]);
}

#[test]
fn ten_thousand_attribute_instances_share_their_large_source_metadata() {
    let body = "X".repeat(1_000_000);
    let data=format!("0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\nLeaf\n10\n0\n20\n0\n0\nENDBLK\n0\nBLOCK\n2\nOwner\n10\n0\n20\n0\n0\nINSERT\n5\nINNER\n2\nLeaf\n66\n1\n10\n1000\n20\n2000\n0\nATTRIB\n5\nATTR\n2\nCODE\n1\nA\n10\n2\n20\n3\n1001\nBIG\n1000\n{body}\n0\nSEQEND\n0\nENDBLK\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nINSERT\n5\nROOT\n2\nOwner\n10\n0\n20\n0\n70\n10000\n44\n2\n0\nENDSEC\n0\nEOF\n");
    let dir = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(false);
    let mut catalog = Catalog::from_dirs(&[dir.path().to_owned()], &cancel).unwrap();
    let overlay = parse_dxf_bytes_with_catalog(
        data.as_bytes(),
        Path::new("shared.dxf"),
        &cancel,
        &mut catalog,
    )
    .unwrap();
    assert!(overlay.block_warnings.is_empty());
    assert_eq!(overlay.expanded.features.len(), 10000);
    let source = &overlay.blocks["owner"].features[0].linked_attributes()[0];
    assert_eq!(
        source.cad.as_deref().unwrap().xdata["BIG"][0].value.len(),
        body.len()
    );
    for draw in overlay.rendered_features() {
        assert!(std::ptr::eq(source, draw.source));
    }
    let encoded = serde_json::to_vec(&overlay).unwrap();
    assert!(
        encoded.len() < 1_100_000,
        "project data contains one source, not one metadata copy per instance"
    );
}
#[test]
fn malformed_linked_attribute_project_keeps_the_previous_file() {
    let fonts = fonts();
    let overlay = load("attributes-binary.dxf", fonts.path());
    let project = NativeProject {
        overlays: vec![overlay],
        ..Default::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.json");
    save_project(&path, &project).unwrap();
    let original = std::fs::read(&path).unwrap();
    for mode in 0..4 {
        let mut bad = project.clone();
        let cad = bad.overlays[0].features[0].cad.as_deref_mut().unwrap();
        let attr = &mut cad.linked_attributes[0];
        match mode {
            0 => attr.entity_type = "INSERT".into(),
            1 => attr.points[0].x = f64::NAN,
            2 => attr
                .cad
                .as_deref_mut()
                .unwrap()
                .linked_attributes
                .push(project.overlays[0].features[0].linked_attributes()[0].clone()),
            _ => {
                attr.cad
                    .as_deref_mut()
                    .unwrap()
                    .text
                    .as_deref_mut()
                    .unwrap()
                    .attribute
                    .as_deref_mut()
                    .unwrap()
                    .attribute_type = 99
            }
        }
        assert!(save_project(&path, &bad).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}
#[test]
fn missing_block_still_shows_its_independently_positioned_attribute_and_keeps_metadata() {
    let text=b"0\nSECTION\n2\nENTITIES\n0\nINSERT\n5\nA\n2\nMissing\n66\n1\n10\n10000\n20\n20000\n41\n-2\n42\n3\n50\n37\n0\nATTRIB\n5\nB\n2\nCODE\n1\nValue\n10\n2\n20\n3\n40\n2.5\n73\n12345\n74\n0\n0\nSEQEND\n0\nENDSEC\n0\nEOF\n";
    let fonts = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(false);
    let mut catalog = Catalog::from_dirs(&[fonts.path().to_owned()], &cancel).unwrap();
    let overlay =
        parse_dxf_bytes_with_catalog(text, Path::new("missing.dxf"), &cancel, &mut catalog)
            .unwrap();
    assert!(overlay
        .block_warnings
        .iter()
        .any(|w| w.contains("Blokdefinitie ontbreekt")));
    let drawn = overlay
        .rendered_features()
        .find(|f| f.source.entity_type == "ATTRIB")
        .unwrap();
    assert!(!drawn.points.is_empty());
    close(g::xyz(drawn.points[0]), [2., 3., 0.]);
    assert_eq!(
        overlay.features[0].cad.as_deref().unwrap().attributes["CODE"],
        "Value"
    );
}
