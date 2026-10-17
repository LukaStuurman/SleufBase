use serde::Deserialize;
use sleufbase_rust::{
    cad::Layer,
    dxf_read::{load_dxf, parse_dxf},
    project::{read_project, save_project, NativeProject},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cad-layers")
}
#[derive(Deserialize)]
struct Expected {
    handle: String,
    raw: String,
    layer: String,
    rgb: [u8; 3],
    visible: bool,
}

#[test]
fn unicode_layer_aliases_use_the_original_table_color_and_visibility_in_both_formats() {
    let expected: Vec<Expected> =
        serde_json::from_str(include_str!("fixtures/cad-layers/expected.json")).unwrap();
    assert_eq!(expected.len(), 11);
    for file in ["layers-ascii.dxf", "layers-binary.dxf"] {
        let overlay = load_dxf(&root().join(file)).unwrap();
        assert!(overlay.block_warnings.is_empty());
        assert_eq!(overlay.layers.len(), 11); // 0, Defpoints, eight named layers, Root
        let drawing: Vec<_> = overlay.rendered_features().collect();
        assert_eq!(drawing.len(), expected.len());
        for (actual, expected) in drawing.iter().zip(&expected) {
            assert_eq!(actual.source.handle, expected.handle);
            assert_eq!(
                actual.source.layer, expected.raw,
                "source spelling must remain intact"
            );
            assert_eq!(actual.layer, expected.layer);
            assert_eq!(actual.rgb, expected.rgb);
            assert_eq!(actual.visible, expected.visible);
        }
        // Unicode lower(), not casefold(): these are separate ezdxf layers.
        assert!(overlay.layers.contains_key("Straße"));
        assert!(overlay.layers.contains_key("STRASSE"));
    }
}

#[test]
fn restored_aliases_and_nested_visibility_share_immutable_geometry() {
    let mut overlay = load_dxf(&root().join("layers-binary.dxf")).unwrap();
    overlay.layers.get_mut("Water").unwrap().visible = false;
    overlay.refresh_block_visibility();
    let before: Vec<_> = overlay.rendered_features().map(|f| f.visible).collect();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("project.json");
    save_project(
        &path,
        &NativeProject {
            overlays: vec![overlay],
            ..Default::default()
        },
    )
    .unwrap();
    let mut loaded = read_project(&path).unwrap();
    let overlay = &mut loaded.overlays[0];
    assert_eq!(
        overlay
            .rendered_features()
            .map(|f| f.visible)
            .collect::<Vec<_>>(),
        before
    );
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(json["overlays"][0].get("layer_aliases").is_none());
    let cache = overlay.expanded.clone();
    let aliases = overlay.layer_aliases.clone();
    let snapshot = overlay.clone();
    assert!(Arc::ptr_eq(&aliases, &snapshot.layer_aliases));
    overlay.layers.get_mut("Root").unwrap().visible = false;
    overlay.refresh_block_visibility();
    assert!(overlay
        .rendered_features()
        .filter(|f| f.points[0].x > 100000.)
        .all(|f| !f.visible));
    overlay.layers.get_mut("Root").unwrap().visible = true;
    overlay.layers.get_mut("Water").unwrap().visible = true;
    overlay.refresh_block_visibility();
    assert!(overlay
        .rendered_features()
        .filter(|f| f.layer == "Water")
        .all(|f| f.visible));
    assert!(Arc::ptr_eq(&cache, &overlay.expanded));
    assert!(Arc::ptr_eq(&aliases, &overlay.layer_aliases));
}

#[test]
fn conflicting_old_project_variants_are_retained_with_a_reload_notice() {
    let mut overlay = load_dxf(&root().join("layers-ascii.dxf")).unwrap();
    overlay.layers.insert(
        "wATER".into(),
        Layer {
            visible: false,
            ..Default::default()
        },
    );
    overlay.rebuild_block_cache().unwrap();
    assert!(overlay
        .block_warnings
        .iter()
        .any(|w| w.contains("laagvarianten") && w.contains("herlaad")));
    let before = serde_json::to_value(&overlay.layers).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("old.json");
    save_project(
        &path,
        &NativeProject {
            overlays: vec![overlay],
            ..Default::default()
        },
    )
    .unwrap();
    let loaded = read_project(&path).unwrap();
    assert_eq!(
        serde_json::to_value(&loaded.overlays[0].layers).unwrap(),
        before
    );
    assert!(!loaded.overlays[0].block_warnings.is_empty());
}

#[test]
fn duplicate_layer_definitions_are_rejected_instead_of_selecting_an_arbitrary_style() {
    let text = "0\nSECTION\n2\nTABLES\n0\nLAYER\n2\nWater\n62\n5\n0\nLAYER\n2\nwATER\n62\n1\n0\nENDSEC\n0\nEOF\n";
    assert!(parse_dxf(text, Path::new("duplicate.dxf"))
        .unwrap_err()
        .contains("dubbele laagnamen"));
}
