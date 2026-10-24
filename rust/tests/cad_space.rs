use serde::Deserialize;
use sleufbase_rust::{
    dxf_fonts::Catalog,
    dxf_read::{parse_dxf, parse_dxf_bytes_with_catalog, DxfOverlay, SpacePolicy},
    project::{read_project, save_project, NativeProject},
};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
#[derive(Deserialize)]
struct Entity {
    handle: String,
    kind: String,
}
#[derive(Deserialize)]
struct Points {
    handle: String,
    kind: String,
    points: Vec<[f64; 2]>,
}
#[derive(Deserialize)]
struct Case {
    file: String,
    selected: Vec<Entity>,
    paths: Vec<Points>,
    bounds: [f64; 4],
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cad-space")
}
fn cases() -> Vec<Case> {
    serde_json::from_str(include_str!("fixtures/cad-space/reference.json")).unwrap()
}
fn load(file: &str) -> DxfOverlay {
    let cancel = AtomicBool::new(false);
    let mut catalog = Catalog::from_dirs(&[], &cancel).unwrap();
    let path = root().join(file);
    parse_dxf_bytes_with_catalog(&std::fs::read(&path).unwrap(), &path, &cancel, &mut catalog)
        .unwrap()
}
fn verify(overlay: &DxfOverlay, want: &Case) {
    assert_eq!(overlay.space_policy, SpacePolicy::Modelspace);
    assert!(
        overlay.warnings.is_empty(),
        "{} {:?}",
        want.file,
        overlay.warnings
    );
    assert!(
        overlay.block_warnings.is_empty(),
        "{:?}",
        overlay.block_warnings
    );
    assert_eq!(overlay.features.len(), want.selected.len(), "{}", want.file);
    for (actual, expected) in overlay.features.iter().zip(&want.selected) {
        assert_eq!(actual.handle, expected.handle, "{}", want.file);
        assert_eq!(actual.entity_type, expected.kind);
    }
    for path in &want.paths {
        let source = overlay
            .features
            .iter()
            .find(|f| f.handle == path.handle)
            .unwrap();
        assert_eq!(source.entity_type, path.kind);
        assert_eq!(source.points.len(), path.points.len());
        for (a, b) in source.points.iter().zip(&path.points) {
            assert!(
                (a.x - b[0]).abs() < 1e-8 && (a.y - b[1]).abs() < 1e-8,
                "{}",
                want.file
            );
        }
    }
    assert_eq!(overlay.blocks.keys().cloned().collect::<Vec<_>>(), ["box"]);
    let b = overlay.bounds().unwrap();
    assert_eq!(
        [b.min_x, b.min_y, b.max_x, b.max_y],
        want.bounds,
        "{}",
        want.file
    );
}
#[test]
fn r12_to_r2018_ascii_binary_owners_flags_appdata_and_section_order_match_original_importer() {
    let cases = cases();
    assert_eq!(cases.len(), 24);
    assert_eq!(cases.iter().map(|c| c.selected.len()).sum::<usize>(), 168);
    for case in &cases {
        verify(&load(&case.file), case);
    }
}
#[test]
fn selected_space_roundtrips_without_changing_source_and_unlabelled_old_caches_require_reload() {
    let case = cases()
        .into_iter()
        .find(|c| c.file == "R2018-moved-model-binary.dxf")
        .unwrap();
    let overlay = load(&case.file);
    let project = NativeProject {
        overlays: vec![overlay],
        ..Default::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.json");
    save_project(&path, &project).unwrap();
    let loaded = read_project(&path).unwrap();
    verify(&loaded.overlays[0], &case);
    let mut old = serde_json::to_value(&project).unwrap();
    old["overlays"][0]
        .as_object_mut()
        .unwrap()
        .remove("space_policy");
    let mut paper = old["overlays"][0]["features"][0].clone();
    paper["handle"] = "old cached paper".into();
    paper["points"][0]["x"] = 1e9.into();
    paper["points"][1]["x"] = (1e9 + 10.).into();
    old["overlays"][0]["features"]
        .as_array_mut()
        .unwrap()
        .push(paper);
    let bytes = serde_json::to_vec(&old).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let old = read_project(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(old.overlays[0].space_policy, SpacePolicy::Legacy);
    assert_eq!(old.overlays[0].features.len(), case.selected.len() + 1);
    assert!(old.overlays[0]
        .block_warnings
        .iter()
        .any(|w| w.contains("Herlaad de DXF-bron")));
    assert!(
        old.overlays[0].bounds().unwrap().max_x >= 1e9,
        "never guess/remove old source geometry without space metadata"
    );
    let mut block_only = serde_json::to_value(&project).unwrap();
    block_only["overlays"][0]
        .as_object_mut()
        .unwrap()
        .remove("space_policy");
    block_only["overlays"][0]["features"] = serde_json::json!([]);
    std::fs::write(&path, serde_json::to_vec(&block_only).unwrap()).unwrap();
    let block_only = read_project(&path).unwrap();
    assert!(
        block_only.overlays[0]
            .block_warnings
            .iter()
            .any(|w| w.contains("Herlaad de DXF-bron")),
        "old caches may have stored model geometry only as block content"
    );
}
#[test]
fn ignored_heavy_paper_geometry_and_linked_records_are_not_tessellated_or_reported_unsupported() {
    let data="0\nSECTION\n2\nENTITIES\n0\nCIRCLE\n67\n1\n10\n0\n20\n0\n40\n1e100\n0\nPOLYLINE\n67\n1\n0\nVERTEX\n67\n0\n10\n1e100\n20\n1e100\n0\nSEQEND\n0\nINSERT\n67\n1\n66\n1\n2\nMissing\n10\n1e100\n20\n1e100\n0\nATTRIB\n67\n0\n2\nTAG\n1\nIgnored\n10\n0\n20\n0\n0\nSEQEND\n0\nMTEXT\n67\n1\n1\nIgnored\n0\nENDSEC\n0\nEOF\n";
    let overlay = parse_dxf(data, Path::new("only-paper.dxf")).unwrap();
    assert!(overlay.features.is_empty());
    assert!(overlay.bounds().is_none());
    assert!(overlay.warnings.is_empty());
    assert!(overlay.block_warnings.is_empty());
}
#[test]
fn skipped_sequences_and_layouts_stay_structurally_checked_and_bad_appdata_is_explicit() {
    for data in [
        "0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n67\n1\n0\nLINE\n10\n0\n20\n0\n11\n1\n21\n1\n0\nENDSEC\n0\nEOF\n",
        "0\nSECTION\n2\nENTITIES\n0\nINSERT\n67\n1\n66\n1\n0\nENDSEC\n0\nEOF\n",
        "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\n*Paper_Space\n0\nPOLYLINE\n0\nENDBLK\n0\nENDSEC\n0\nEOF\n",
        "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\n*Model_Space\n0\nENDBLK\n0\nBLOCK\n2\n$Model_Space\n0\nENDBLK\n0\nENDSEC\n0\nEOF\n",
        "0\nSECTION\n2\nENTITIES\n0\nLINE\n102\n{unclosed\n10\n0\n20\n0\n11\n1\n21\n1\n0\nENDSEC\n0\nEOF\n",
        "0\nSECTION\n2\nTABLES\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n0\nENDSEC\n0\nEOF\n",
    ] {assert!(parse_dxf(data,Path::new("bad.dxf")).is_err());}
}
#[test]
fn metadata_scan_honors_cancellation_before_reading_a_drawing() {
    let cancel = AtomicBool::new(true);
    let mut catalog = Catalog::from_dirs(&[], &AtomicBool::new(false)).unwrap();
    assert!(parse_dxf_bytes_with_catalog(
        b"0\nEOF\n",
        Path::new("cancel.dxf"),
        &cancel,
        &mut catalog
    )
    .is_err());
}
