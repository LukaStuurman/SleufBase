use sleufbase_rust::{
    dxf_document::Document,
    template_metadata::{self, Date, Values},
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
const SOURCE: &[u8] = include_bytes!("fixtures/template-metadata/sheets.dxf");
fn encoded(doc: &Document) -> Vec<u8> {
    let mut bytes = Vec::new();
    doc.write_to(&mut bytes, &AtomicBool::new(false)).unwrap();
    bytes
}
fn values() -> Values {
    Values {
        date: Date::new(2026, 10, 18).unwrap(),
        project: Some("Project".into()),
        drawn_by: "L. Test".into(),
    }
}
#[test]
fn all_title_fields_match_independent_python_and_other_tags_stay_intact() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/template-metadata/reference.json")).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    for case in cases.as_array().unwrap() {
        let cancel = AtomicBool::new(false);
        let mut doc = Document::load(&root.join(case["path"].as_str().unwrap()), &cancel).unwrap();
        let layouts: Vec<String> = case["layouts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().into())
            .collect();
        let count = template_metadata::update(
            &mut doc,
            &layouts,
            &Values {
                date: values().date,
                project: case["project"].as_str().map(str::to_owned),
                drawn_by: case["drawn_by"].as_str().unwrap().into(),
            },
            &cancel,
        )
        .unwrap();
        assert_eq!(count, case["expected"].as_object().unwrap().len());
        let output = Document::from_bytes(encoded(&doc), &cancel).unwrap();
        for before in doc.records() {
            let Some(handle) = before.handle() else {
                continue;
            };
            let handle = format!("{handle:X}");
            let after = output.by_handle(&handle).unwrap().unwrap();
            let wanted = case["expected"][&handle].as_str();
            let tags: Vec<_> = before
                .tags()
                .map(|(c, v)| (c, if c == 1 { wanted.unwrap_or(v) } else { v }))
                .collect();
            assert_eq!(
                tags,
                after.tags().collect::<Vec<_>>(),
                "{} {handle}",
                case["name"]
            );
        }
    }
}
#[test]
fn invalid_or_cancelled_batch_preserves_every_previous_edit() {
    let cancel = AtomicBool::new(false);
    let layouts = vec!["Layout1".into(), "Blad 2".into()];
    let mut doc = Document::from_bytes(SOURCE.to_vec(), &cancel).unwrap();
    template_metadata::update(&mut doc, &layouts, &values(), &cancel).unwrap();
    let before = encoded(&doc);
    for name in ["wrong\nvalue".into(), "a\u{85}b".into(), "😀".repeat(1025)] {
        let mut invalid = values();
        invalid.project = Some(name);
        assert!(template_metadata::update(&mut doc, &layouts, &invalid, &cancel).is_err());
        assert_eq!(before, encoded(&doc));
    }
    assert!(
        template_metadata::update(&mut doc, &layouts, &values(), &AtomicBool::new(true)).is_err()
    );
    assert_eq!(before, encoded(&doc));
    // A bad target near the end must not commit the valid date fields before it.
    let broken = std::str::from_utf8(SOURCE)
        .unwrap()
        .replace("  1\noriginal 0\n", "");
    let mut broken_doc = Document::from_bytes(broken.as_bytes().to_vec(), &cancel).unwrap();
    assert!(template_metadata::update(&mut broken_doc, &layouts, &values(), &cancel).is_err());
    assert_eq!(encoded(&broken_doc), broken.as_bytes());
}
#[test]
fn repeated_values_replace_previous_staged_metadata_and_empty_project_clears() {
    let cancel = AtomicBool::new(false);
    let mut doc = Document::from_bytes(SOURCE.to_vec(), &cancel).unwrap();
    let layouts = vec!["bLaD 2".into()];
    template_metadata::update(&mut doc, &layouts, &values(), &cancel).unwrap();
    let mut next = values();
    next.project = Some("".into());
    next.date = Date::new(2028, 2, 29).unwrap();
    template_metadata::update(&mut doc, &layouts, &next, &cancel).unwrap();
    let output = String::from_utf8(encoded(&doc)).unwrap();
    assert!(output.contains("29-02-2028"));
    assert!(!output.contains("18-10-2026"));
    assert!(!output.contains("  1\nProject\n"));
}
#[test]
fn calendar_dates_validate_leap_years_and_dutch_format() {
    for (y, m, d) in [
        (0, 1, 1),
        (10000, 1, 1),
        (1900, 2, 29),
        (2100, 2, 29),
        (2026, 2, 29),
        (2026, 4, 31),
        (2026, 0, 1),
        (2026, 1, 0),
    ] {
        assert!(Date::new(y, m, d).is_err());
    }
    for (y, m, d, s) in [
        (2000, 2, 29, "29-02-2000"),
        (2024, 2, 29, "29-02-2024"),
        (2026, 1, 9, "09-01-2026"),
        (9999, 12, 31, "31-12-9999"),
    ] {
        assert_eq!(Date::new(y, m, d).unwrap().to_string(), s);
    }
    #[cfg(windows)]
    assert!(Date::today().is_ok());
}
