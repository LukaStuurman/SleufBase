//! Definition sources and glyphs vs the original frontend selection + text2path.
use serde_json::Value;
use sleufbase_rust::{
    cad_geometry::{self as g, V3},
    dxf_fonts::Catalog,
    dxf_read::{parse_dxf_bytes_with_catalog, DxfOverlay},
    project::{read_project, save_project, NativeProject},
};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

fn reference() -> Value {
    serde_json::from_str(include_str!("fixtures/cad-attdef/reference.json")).unwrap()
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}
fn fonts() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    for file in reference()["files"].as_array().unwrap() {
        let p = root().join(file.as_str().unwrap());
        std::fs::copy(&p, d.path().join(p.file_name().unwrap())).unwrap();
    }
    d
}
fn load(name: &str, font_dir: &Path) -> DxfOverlay {
    let cancel = AtomicBool::new(false);
    let mut catalog = Catalog::from_dirs(&[font_dir.to_owned()], &cancel).unwrap();
    let path = root().join("cad-attdef").join(name);
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
    let n = g::dot(d, d);
    let t = if n == 0. {
        0.
    } else {
        (g::dot(g::sub(p, a), d) / n).clamp(0., 1.)
    };
    g::length(g::sub(p, g::add(a, g::scale(d, t))))
}
fn verify(o: &DxfOverlay, case: &Value) {
    assert!(o.block_warnings.is_empty(), "{:?}", o.block_warnings);
    assert_eq!(
        o.warnings.len(),
        usize::from(case["file"].as_str().unwrap().starts_with("R2018"))
    );
    if !o.warnings.is_empty() {
        assert!(o.warnings[0].starts_with("ATTDEF: ingebouwde MTEXT"));
    }
    let sources: Vec<_> = o
        .features
        .iter()
        .chain(o.blocks.values().flat_map(|b| b.features.iter()))
        .filter(|f| f.entity_type == "ATTDEF")
        .collect();
    assert_eq!(sources.len(), case["definitions"].as_array().unwrap().len());
    for w in case["definitions"].as_array().unwrap() {
        let s = sources
            .iter()
            .find(|s| s.handle == w["handle"].as_str().unwrap())
            .unwrap();
        let t = s.cad.as_deref().unwrap().text.as_deref().unwrap();
        let a = t.attribute.as_deref().unwrap();
        assert_eq!(s.raw_text, w["content"]);
        assert_eq!(t.layout.content, s.raw_text);
        assert_eq!(a.tag, w["tag"]);
        assert_eq!(a.prompt.as_deref().unwrap(), w["prompt"]);
        assert_eq!(a.flags as u64, w["flags"].as_u64().unwrap());
        assert_eq!(a.field_length as u64, w["field_length"].as_u64().unwrap());
        assert_eq!(a.lock_position, w["lock_position"].as_bool().unwrap());
        assert_eq!(a.multiline(), w["multiline"].as_bool().unwrap());
        let details = s.details(&o.path);
        assert_eq!(details["Invoerprompt"], a.prompt.as_deref().unwrap());
        assert_eq!(details["Tekst"], s.raw_text);
    }
    for block in o.blocks.values() {
        for f in &block.features {
            if f.entity_type == "ATTDEF" {
                assert!(f.points.is_empty());
                assert!(f
                    .cad
                    .as_deref()
                    .unwrap()
                    .text
                    .as_deref()
                    .unwrap()
                    .contours
                    .points
                    .is_empty());
            }
        }
    }
    let indices = o
        .features
        .iter()
        .enumerate()
        .filter(|(_, f)| f.entity_type != "INSERT")
        .map(|(i, _)| i)
        .chain(o.features.len()..o.features.len() + o.expanded.features.len());
    let expected = case["scene"].as_array().unwrap();
    assert_eq!(indices.clone().count(), expected.len());
    for (i, w) in indices.zip(expected) {
        let d = o.inspect_feature(i).unwrap();
        let detail = o.inspect_details(i).unwrap();
        assert_eq!(d.source.entity_type, w["kind"]);
        assert_eq!(d.source.handle, w["handle"]);
        assert_eq!(
            detail.get("Blokpad").cloned().unwrap_or_default(),
            w["blockpath"]
        );
        assert_eq!(d.layer, w["layer"]);
        assert_eq!(
            d.rgb,
            std::array::from_fn::<_, 3, _>(|j| w["rgb"][j].as_u64().unwrap() as u8)
        );
        assert_eq!(d.visible, w["visible"].as_bool().unwrap());
        if d.source.entity_type == "LINE" {
            for (p, w) in d.points.iter().zip(w["points"].as_array().unwrap()) {
                close(g::xyz(*p), point(w));
            }
            continue;
        }
        let actual: Vec<_> = d.paths().filter(|p| !p.is_empty()).collect();
        let paths = w["paths"].as_array().unwrap();
        assert_eq!(actual.len(), paths.len(), "{}", d.source.handle);
        for (a, w) in actual.iter().zip(paths) {
            let mut from = point(&w["start"]);
            close(g::xyz(a[0]), from);
            for cmd in w["commands"].as_array().unwrap() {
                let mut c = vec![from];
                c.extend(cmd["points"].as_array().unwrap().iter().map(point));
                for sample in 0..=32 {
                    let mut q = c.clone();
                    let t = sample as f64 / 32.;
                    for n in (1..q.len()).rev() {
                        for j in 0..n {
                            q[j] = g::add(g::scale(q[j], 1. - t), g::scale(q[j + 1], t));
                        }
                    }
                    assert!(
                        a.windows(2)
                            .map(|e| distance(q[0], g::xyz(e[0]), g::xyz(e[1])))
                            .fold(f64::INFINITY, f64::min)
                            <= 0.00500001
                    );
                }
                from = *c.last().unwrap();
            }
            close(g::xyz(*a.last().unwrap()), from);
        }
    }
}
#[test]
fn r12_to_r2018_ascii_binary_definitions_match_original_tag_ink_and_insert_exclusion() {
    let fonts = fonts();
    let r = reference();
    let cases = r["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    assert_eq!(
        cases
            .iter()
            .map(|c| c["definitions"].as_array().unwrap().len())
            .sum::<usize>(),
        442
    );
    assert_eq!(
        cases
            .iter()
            .map(|c| c["scene"].as_array().unwrap().len())
            .sum::<usize>(),
        602
    );
    for c in cases {
        verify(&load(c["file"].as_str().unwrap(), fonts.path()), c);
    }
}
#[test]
fn project_roundtrip_preserves_values_prompts_glyphs_and_exclusion_without_fonts() {
    let fonts = fonts();
    let r = reference();
    let c = &r["cases"][7];
    let overlay = load(c["file"].as_str().unwrap(), fonts.path());
    let before = serde_json::to_value(&overlay).unwrap();
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("project.json");
    let project = NativeProject {
        overlays: vec![overlay],
        ..Default::default()
    };
    save_project(&p, &project).unwrap();
    drop(fonts);
    let after = read_project(&p).unwrap();
    assert_eq!(serde_json::to_value(&after.overlays[0]).unwrap(), before);
    verify(&after.overlays[0], c);
    let mut bad = after;
    bad.overlays[0].features[0]
        .cad
        .as_mut()
        .unwrap()
        .text
        .as_mut()
        .unwrap()
        .attribute
        .as_mut()
        .unwrap()
        .prompt = Some("X".repeat(1_048_577));
    let bytes = std::fs::read(&p).unwrap();
    assert!(save_project(&p, &bad).is_err());
    assert_eq!(std::fs::read(&p).unwrap(), bytes);
}
#[test]
fn actual_template_map_contains_definition_tag_strokes_and_respects_layer_visibility() {
    let fonts = fonts();
    let mut o = load("R2018-asc.dxf", fonts.path());
    o.features.retain(|f| f.entity_type == "ATTDEF");
    o.blocks.clear();
    o.rebuild_block_cache().unwrap();
    let b = o.bounds().unwrap();
    let b = sleufbase_rust::Bounds {
        min_x: b.min_x - 1.,
        min_y: b.min_y - 1.,
        max_x: b.max_x + 1.,
        max_y: b.max_y + 1.,
    };
    let expected: Vec<_> = o
        .rendered_features()
        .filter(|d| d.visible)
        .flat_map(|d| {
            d.paths()
                .flat_map(|p| {
                    p.windows(2)
                        .filter(|e| e[0] != e[1])
                        .map(|e| vec![[e[0].x, e[0].y], [e[1].x, e[1].y]])
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let mut strokes = Vec::new();
    sleufbase_rust::template_cad::append_visible(
        std::slice::from_ref(&o),
        b,
        &mut strokes,
        &Default::default(),
    )
    .unwrap();
    assert!(!expected.is_empty());
    assert_eq!(
        strokes.iter().map(|s| s.points.clone()).collect::<Vec<_>>(),
        expected
    );
    o.visible = false;
    strokes.clear();
    sleufbase_rust::template_cad::append_visible(&[o], b, &mut strokes, &Default::default())
        .unwrap();
    assert!(strokes.is_empty());
}
#[test]
fn old_attribute_metadata_without_prompt_remains_readable() {
    let fonts = fonts();
    let o = load("R12-asc.dxf", fonts.path());
    let mut value = serde_json::to_value(&o.features[0]).unwrap();
    value["entity_type"] = "ATTRIB".into();
    value["cad"]["text"]["attribute"]
        .as_object_mut()
        .unwrap()
        .remove("prompt");
    let f: sleufbase_rust::dxf_read::DxfFeature = serde_json::from_value(value).unwrap();
    assert!(f
        .cad
        .as_deref()
        .unwrap()
        .text
        .as_deref()
        .unwrap()
        .attribute
        .as_deref()
        .unwrap()
        .prompt
        .is_none());
}

#[test]
fn many_definitions_and_nested_grid_references_keep_one_source_and_no_duplicate_ink() {
    let mut data = String::from("0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\nDefinitions\n");
    for i in 0..2000 {
        data.push_str(&format!("0\nATTDEF\n5\nD{i}\n2\nFIELD{i}\n3\nKeep prompt\n1\nDefault value\n10\n1000000\n20\n1000000\n"));
    }
    data.push_str("0\nLINE\n5\nLINE\n10\n0\n20\n0\n11\n1\n21\n2\n0\nENDBLK\n0\nBLOCK\n2\nOwner\n0\nINSERT\n5\nINNER\n2\nDefinitions\n10\n0\n20\n0\n0\nENDBLK\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nINSERT\n5\nROOT\n2\nOwner\n10\n0\n20\n0\n70\n20000\n44\n2\n0\nENDSEC\n0\nEOF\n");
    let dir = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(false);
    let mut catalog = Catalog::from_dirs(&[dir.path().to_owned()], &cancel).unwrap();
    let o = parse_dxf_bytes_with_catalog(
        data.as_bytes(),
        Path::new("many-definitions.dxf"),
        &cancel,
        &mut catalog,
    )
    .unwrap();
    assert!(o.warnings.is_empty());
    assert!(o.block_warnings.is_empty());
    assert_eq!(o.expanded.features.len(), 20000);
    assert_eq!(o.block_visibility.len(), 40000);
    assert_eq!(o.blocks["definitions"].features.len(), 2001);
    for d in o.rendered_features() {
        assert!(std::ptr::eq(
            d.source,
            &o.blocks["definitions"].features[2000]
        ));
    }
    assert_eq!(o.bounds().unwrap().max_x, 39999.);
    assert_eq!(o.bounds().unwrap().max_y, 2.);
    assert!(
        serde_json::to_vec(&o).unwrap().len() < 2_000_000,
        "source definitions are stored once, not per instance"
    );
}
