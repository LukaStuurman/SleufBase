use sleufbase_rust::{dxf_document::Document, template_pages};
use std::{path::PathBuf, sync::atomic::AtomicBool};

fn bytes(doc: &Document) -> Vec<u8> {
    let mut result = Vec::new();
    doc.write_to(&mut result, &AtomicBool::new(false)).unwrap();
    result
}

#[test]
fn invalid_shared_underlay_definition_and_cancellation_leave_document_unchanged() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/template-underlays/source.dxf");
    for kind in ["PDFUNDERLAY", "DGNUNDERLAY", "DWFUNDERLAY"] {
        for target in ["0", "1"] {
            let cancel = AtomicBool::new(false);
            let source = std::fs::read_to_string(&path).unwrap();
            let mut lines: Vec<String> = source.lines().map(str::to_owned).collect();
            let mut active = false;
            let mut changed = false;
            for pair in lines.chunks_exact_mut(2) {
                if pair[0].trim() == "0" { active = pair[1] == kind; }
                if active && pair[0].trim() == "340" {
                    pair[1] = target.into();
                    changed = true;
                    break;
                }
            }
            assert!(changed);
            let dir = tempfile::tempdir().unwrap();
            let malformed = dir.path().join("invalid-definition.dxf");
            std::fs::write(&malformed, lines.join("\n") + "\n").unwrap();
            let mut doc = Document::load(&malformed, &cancel).unwrap();
            let before = bytes(&doc);
            assert!(template_pages::extend(&mut doc, 9, &cancel).is_err(), "{kind}: {target}");
            assert_eq!(bytes(&doc), before);
        }
    }
    let mut doc = Document::load(&path, &AtomicBool::new(false)).unwrap();
    let before = bytes(&doc);
    assert!(template_pages::extend(&mut doc, 9, &AtomicBool::new(true)).is_err());
    assert_eq!(bytes(&doc), before);
}
#[test]
fn original_supported_underlay_types_survive_repeated_sheet_extension() {
    let cancel = AtomicBool::new(false);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/template-underlays/source.dxf");
    let mut doc = Document::load(&path, &cancel).unwrap();
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/template-underlays/reference.json")).unwrap();
    for step in cases.as_array().unwrap() {
        template_pages::extend(
            &mut doc,
            step["required"].as_u64().unwrap() as usize,
            &cancel,
        )
        .unwrap();
        let count = step["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["entities"].as_array().unwrap().len())
            .sum::<usize>();
        assert_eq!(
            doc.records()
                .filter(|r| matches!(r.kind(), "PDFUNDERLAY" | "DGNUNDERLAY" | "DWFUNDERLAY"))
                .count(),
            count
        );
        assert_eq!(
            doc.records()
                .filter(|r| matches!(
                    r.kind(),
                    "PDFDEFINITION" | "DGNDEFINITION" | "DWFDEFINITION"
                ))
                .count(),
            3
        );
        for e in doc
            .records()
            .filter(|r| matches!(r.kind(), "PDFUNDERLAY" | "DGNUNDERLAY" | "DWFUNDERLAY"))
        {
            let definition = doc.by_handle(e.first(340).unwrap()).unwrap().unwrap();
            assert!(definition
                .tags()
                .any(|(c, v)| c == 330 && Some(v) == e.first(5)));
        }
    }
}
