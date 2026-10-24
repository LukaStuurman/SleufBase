use serde::Deserialize;
use serde_json::Value;
use sleufbase_rust::{
    rule_settings,
    settings_compat::{self, MigratedSettings},
    summarize, Dataset, Rule,
};
#[derive(Deserialize)]
struct Case {
    name: String,
    rows: Option<Value>,
    rules: Vec<Rule>,
    datasets: Vec<Dataset>,
    expected: Value,
}
#[test]
fn original_rule_normalization_and_report_counts_match_including_corrected_casefold_groups() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/disciplines/workbooks.json")).unwrap();
    assert_eq!(cases.len(), 10);
    for case in cases {
        let rules = rule_settings::from_legacy(case.rows.as_ref()).unwrap();
        assert_eq!(rules, case.rules, "{}", case.name);
        for (dataset, row) in case
            .datasets
            .iter()
            .zip(case.expected["rows"].as_array().unwrap().iter().skip(1))
        {
            let summary = summarize(dataset, &rules);
            assert_eq!(
                summary.distinct_count() as u64,
                row["cells"][1]["value"].as_u64().unwrap(),
                "{}",
                case.name
            );
            assert_eq!(
                u64::from(summary.total_count()),
                row["cells"][2]["value"].as_u64().unwrap(),
                "{}",
                case.name
            );
        }
    }
}
#[test]
fn legacy_import_and_old_native_settings_preserve_defaults_and_explicit_empty_rules() {
    let legacy = r#"{"kickthemap_object_layer_rules":[{"keywords":"water; WAT?R","target_layer":"DRINKWATER","color":5,"profile_label":"Drinkwater"}],"password":"SECRET"}"#;
    let migrated = settings_compat::from_legacy_json(legacy).unwrap();
    assert_eq!(migrated.object_rules[0].keywords[1], "WAT?R");
    assert_eq!(migrated.object_rules[0].label, "Drinkwater");
    assert!(!serde_json::to_string(&migrated).unwrap().contains("SECRET"));
    assert_eq!(
        settings_compat::from_legacy_json("{}")
            .unwrap()
            .object_rules,
        rule_settings::defaults()
    );
    assert!(
        settings_compat::from_legacy_json(r#"{"kickthemap_object_layer_rules":[]}"#)
            .unwrap()
            .object_rules
            .is_empty()
    );
    let mut old = serde_json::to_value(MigratedSettings::default()).unwrap();
    old.as_object_mut().unwrap().remove("object_rules");
    assert_eq!(
        serde_json::from_value::<MigratedSettings>(old)
            .unwrap()
            .object_rules,
        rule_settings::defaults()
    );
}
#[test]
fn rejected_settings_never_replace_a_valid_file_and_missing_fields_do_not_reset_preferences() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.json");
    let mut settings = MigratedSettings::default();
    settings.object_rules.clear();
    settings_compat::save_rust_settings(&file, &settings).unwrap();
    let bytes = std::fs::read(&file).unwrap();
    assert!(settings_compat::read_rust_settings(&file)
        .unwrap()
        .object_rules
        .is_empty());
    settings.object_rules = vec![Rule::new(
        vec!["water".into()],
        "bad\nlayer".into(),
        5,
        "Water".into(),
    )];
    assert!(settings_compat::save_rust_settings(&file, &settings).is_err());
    assert_eq!(std::fs::read(&file).unwrap(), bytes);
    settings.object_rules = vec![rule_settings::defaults().remove(0); 257];
    assert!(settings_compat::save_rust_settings(&file, &settings).is_err());
    assert_eq!(std::fs::read(&file).unwrap(), bytes);
    std::fs::write(&file, r#"{"object_rules":"invalid"}"#).unwrap();
    assert!(settings_compat::read_rust_settings(&file).is_err());
}

#[test]
fn cli_rejects_invalid_custom_settings_without_publishing_a_report_or_drawing() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("invalid.json");
    let mut config = MigratedSettings::default();
    config.object_rules[0].color_aci = 0;
    std::fs::write(&settings, serde_json::to_vec(&config).unwrap()).unwrap();
    let input =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/jobFeatures.json");
    for ext in ["xlsx", "dxf"] {
        let output = dir.path().join(format!("result.{ext}"));
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_sleufbase-rust"))
            .arg("--settings")
            .arg(&settings)
            .arg(format!("--{ext}"))
            .arg(&output)
            .arg(&input)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("ACI-kleur"));
        assert!(!output.exists());
    }
}

#[test]
fn application_defaults_and_label_gap_follow_the_original_app_rather_than_bare_exporter() {
    let reference: Value = serde_json::from_str(include_str!(
        "fixtures/disciplines/application-settings.json"
    ))
    .unwrap();
    let settings = MigratedSettings::default();
    assert_eq!(
        settings.dxf_label_gap,
        reference["label_gap"].as_f64().unwrap()
    );
    assert_eq!(
        serde_json::to_value(&settings.object_rules).unwrap(),
        reference["rules"]
    );
    for case in reference["gap_cases"].as_array().unwrap() {
        let imported = settings_compat::from_legacy_json(
            &serde_json::json!({"dxf_label_gap":case["value"]}).to_string(),
        )
        .unwrap();
        assert_eq!(
            imported.dxf_label_gap,
            case["expected"].as_f64().unwrap(),
            "{}",
            case["value"]
        );
    }
    let mut old = serde_json::to_value(settings).unwrap();
    old.as_object_mut().unwrap().remove("dxf_label_gap");
    assert_eq!(
        serde_json::from_value::<MigratedSettings>(old)
            .unwrap()
            .dxf_label_gap,
        3.5
    );
}

#[test]
fn invalid_native_label_gap_does_not_replace_saved_settings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let mut settings = MigratedSettings::default();
    settings_compat::save_rust_settings(&path, &settings).unwrap();
    let before = std::fs::read(&path).unwrap();
    for invalid in [f64::NAN, f64::INFINITY, -1., 101.] {
        settings.dxf_label_gap = invalid;
        assert!(settings_compat::save_rust_settings(&path, &settings).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}
