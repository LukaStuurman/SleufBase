use serde::Deserialize;
use sleufbase_rust::{
    dxf_read::load_dxf,
    project::{read_project, save_project, NativeProject},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
struct Case {
    handle: String,
    label: String,
    native_rgb: [u8; 3],
    visible: bool,
    details: BTreeMap<String, String>,
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cad-properties")
}
fn cases() -> Vec<Case> {
    serde_json::from_str(include_str!("fixtures/cad-properties/expected.json")).unwrap()
}

#[test]
fn all_aci_colors_layer_rules_attributes_and_xdata_match_original_oracles() {
    let cases = cases();
    assert_eq!(cases.len(), 266);
    for filename in ["properties-ascii.dxf", "properties-binary.dxf"] {
        let path = root().join(filename);
        let overlay = load_dxf(&path).unwrap();
        assert_eq!(overlay.features.len(), cases.len());
        for (feature, want) in overlay.features.iter().zip(&cases) {
            assert_eq!(feature.handle, want.handle, "{}", want.label);
            assert_eq!(
                overlay.feature_rgb(feature),
                want.native_rgb,
                "{}",
                want.label
            );
            assert_eq!(
                overlay.feature_visible(feature),
                want.visible,
                "{}",
                want.label
            );
            let mut details = want.details.clone();
            details.insert("Bestand".into(), filename.into());
            assert_eq!(feature.details(&path), details, "{}", want.label);
        }
        assert!(
            overlay.bounds().unwrap().max_x < 1e6,
            "invisible distant entities must not distort fit"
        );
    }
}

#[test]
fn layer_controls_and_source_metadata_survive_native_project_roundtrip() {
    let mut overlay = load_dxf(&root().join("properties-binary.dxf")).unwrap();
    overlay.layers.get_mut("Water").unwrap().visible = false;
    overlay.layers.get_mut("Off").unwrap().visible = true;
    let before = serde_json::to_value(&overlay).unwrap();
    let project = NativeProject {
        overlays: vec![overlay],
        ..Default::default()
    };
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("project.json");
    save_project(&path, &project).unwrap();
    let loaded = read_project(&path).unwrap();
    let overlay = &loaded.overlays[0];
    assert_eq!(serde_json::to_value(overlay).unwrap(), before);
    assert!(!overlay.feature_visible(&overlay.features[0]));
    assert_eq!(
        overlay
            .features
            .last()
            .unwrap()
            .cad
            .as_ref()
            .unwrap()
            .attributes["Code"],
        "PS 42"
    );
    assert_eq!(
        overlay
            .features
            .last()
            .unwrap()
            .cad
            .as_ref()
            .unwrap()
            .xdata
            .len(),
        2
    );
}

#[test]
fn invalid_colors_and_xdata_never_replace_previous_project() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("project.json");
    let good = NativeProject {
        overlays: vec![load_dxf(&root().join("properties-ascii.dxf")).unwrap()],
        ..Default::default()
    };
    save_project(&path, &good).unwrap();
    let original = std::fs::read(&path).unwrap();
    let mut invalid = Vec::new();
    let mut project = good.clone();
    project.overlays[0]
        .layers
        .get_mut("Water")
        .unwrap()
        .true_color = Some(0x1000000);
    invalid.push(project);
    let mut project = good.clone();
    project.overlays[0]
        .features
        .last_mut()
        .unwrap()
        .cad
        .as_mut()
        .unwrap()
        .true_color = Some(u32::MAX);
    invalid.push(project);
    for tag in [
        sleufbase_rust::cad::Tag {
            code: 1001,
            value: "bad-app".into(),
        },
        sleufbase_rust::cad::Tag {
            code: 1040,
            value: "NaN".into(),
        },
        sleufbase_rust::cad::Tag {
            code: 1004,
            value: "001Z".into(),
        },
        sleufbase_rust::cad::Tag {
            code: 1071,
            value: "NaN".into(),
        },
    ] {
        let mut project = good.clone();
        project.overlays[0]
            .features
            .last_mut()
            .unwrap()
            .cad
            .as_mut()
            .unwrap()
            .xdata
            .insert("INVALID".into(), vec![tag]);
        invalid.push(project);
    }
    for project in invalid {
        assert!(save_project(&path, &project).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}

#[test]
fn attribute_sequence_errors_are_explicit_and_older_overlays_default_safely() {
    use sleufbase_rust::dxf_read::{parse_dxf, DxfOverlay};
    for records in [
        "0\nATTRIB\n2\nCode\n1\nvalue\n",
        "0\nINSERT\n2\nB\n66\n1\n10\n0\n20\n0\n0\nATTRIB\n2\nCode\n1\nvalue\n",
    ] {
        let text = format!("0\nSECTION\n2\nENTITIES\n{records}0\nENDSEC\n0\nEOF\n");
        assert!(parse_dxf(&text, Path::new("bad.dxf")).is_err());
    }
    let mut value =
        serde_json::to_value(load_dxf(&root().join("properties-ascii.dxf")).unwrap()).unwrap();
    value.as_object_mut().unwrap().remove("layers");
    for feature in value["features"].as_array_mut().unwrap() {
        feature.as_object_mut().unwrap().remove("cad");
    }
    let old: DxfOverlay = serde_json::from_value(value).unwrap();
    assert!(old.layers.is_empty());
    assert!(old.features.iter().all(|f| f.cad.is_none()));
    assert!(old.feature_visible(&old.features[0]));
}

#[test]
fn native_v1_v2_upgrade_without_rewriting_source_and_v3_rejects_unknown_cad_fields() {
    let project = NativeProject {
        overlays: vec![load_dxf(&root().join("properties-ascii.dxf")).unwrap()],
        ..Default::default()
    };
    assert_eq!(project.version, 4);
    let mut old = serde_json::to_value(&project).unwrap();
    old["version"] = 1.into();
    old["overlays"][0].as_object_mut().unwrap().remove("layers");
    for feature in old["overlays"][0]["features"].as_array_mut().unwrap() {
        feature.as_object_mut().unwrap().remove("cad");
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("v1.json");
    let bytes = serde_json::to_vec(&old).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let loaded = read_project(&path).unwrap();
    assert_eq!(loaded.version, 4);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(
        loaded.overlays[0].features.len(),
        project.overlays[0].features.len()
    );
    old["version"] = 2.into();
    let v2_bytes = serde_json::to_vec(&old).unwrap();
    std::fs::write(&path, &v2_bytes).unwrap();
    let v2 = read_project(&path).unwrap();
    assert_eq!(v2.version, 4);
    assert_eq!(std::fs::read(&path).unwrap(), v2_bytes);
    let current = temp.path().join("v3.json");
    save_project(&current, &loaded).unwrap();
    let serialized: serde_json::Value =
        serde_json::from_slice(&std::fs::read(current).unwrap()).unwrap();
    assert_eq!(serialized["version"], 4);
    for location in ["overlay", "feature", "properties", "layer"] {
        let mut value = serde_json::to_value(&project).unwrap();
        let object = match location {
            "overlay" => &mut value["overlays"][0],
            "feature" => &mut value["overlays"][0]["features"][0],
            "properties" => value["overlays"][0]["features"]
                .as_array_mut()
                .unwrap()
                .last_mut()
                .unwrap()
                .get_mut("cad")
                .unwrap(),
            _ => &mut value["overlays"][0]["layers"]["Water"],
        };
        object["future_data"] = serde_json::json!({"must_not_drop":true});
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(
            read_project(&path).unwrap_err().contains("future_data"),
            "{location}"
        );
    }
}
