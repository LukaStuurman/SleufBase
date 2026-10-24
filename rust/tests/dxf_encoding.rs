use serde::Deserialize;
use sleufbase_rust::{
    dxf_read::{load_dxf, parse_dxf, parse_dxf_bytes},
    marxact::parse_overlay,
};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Case {
    ascii: String,
    binary: String,
    layer: String,
    start: [f64; 3],
    end: [f64; 3],
    features: usize,
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn ascii_binary_r12_modern_codepages_and_xdata_keep_the_same_geometry() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/dxf-encoding/manifest.json")).unwrap();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let ascii = load_dxf(&root().join("dxf-encoding").join(&case.ascii)).unwrap();
        let binary = load_dxf(&root().join("dxf-encoding").join(&case.binary)).unwrap();
        assert_eq!(ascii.features.len(), case.features, "{}", case.ascii);
        assert_eq!(
            serde_json::to_value(&ascii.features).unwrap(),
            serde_json::to_value(&binary.features).unwrap(),
            "{}",
            case.binary
        );
        for feature in &binary.features {
            assert_eq!(feature.layer, case.layer);
        }
        for (vertex, expected) in binary.features[0].points.iter().zip([case.start, case.end]) {
            assert_eq!([vertex.x, vertex.y, vertex.z.unwrap()], expected);
        }
    }
}

#[test]
fn binary_nested_marxact_has_identical_labels_and_measured_objects() {
    let dir = root().join("marxact-cad");
    let ascii = load_dxf(&dir.join("nested-leaders.dxf")).unwrap();
    let binary = load_dxf(&dir.join("nested-leaders-binary.dxf")).unwrap();
    assert_eq!(
        serde_json::to_value(&ascii.features).unwrap(),
        serde_json::to_value(&binary.features).unwrap()
    );
    let result = parse_overlay(&binary, &Default::default()).unwrap();
    assert_eq!(result.leader_count, 11);
    assert_eq!(result.trenches.len(), 1);
    assert_eq!(result.assigned_insert_count, 2);
    assert_eq!(result.trenches[0].objects[0].position.z, Some(9.3));
}

#[test]
fn binary_truncation_cannot_return_a_plausible_partial_drawing() {
    for file in ["r12-cp1252-binary.dxf", "r2018-utf8-binary.dxf"] {
        let path = root().join("dxf-encoding").join(file);
        let bytes = std::fs::read(&path).unwrap();
        for end in (0..bytes.len()).step_by(97).chain([bytes.len() - 1]) {
            assert!(
                parse_dxf_bytes(&bytes[..end], &path).is_err(),
                "{file} at byte {end}"
            );
        }
    }
}

#[test]
fn utf8_errors_unknown_codepages_and_broken_sections_are_explicit() {
    let path = Path::new("bad.dxf");
    for text in [
        "0\nSECTION\n2\nENTITIES\n0\nEOF\n",
        "0\nSECTION\n2\nENTITIES\n0\nENDSEC\n",
        "0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n0\nLINE\n",
        "0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n0\nVERTEX\n10\n1\n20\n2\n0\nENDSEC\n0\nEOF\n",
        "0\nSECTION\n2\nENTITIES\n0\nPOLYLINE\n0\nPOLYLINE\n0\nSEQEND\n0\nENDSEC\n0\nEOF\n",
        "0\nSECTION\n2\nENTITIES\n0\nLINE\n10\nNaN\n20\n1\n11\n2\n21\n3\n0\nENDSEC\n0\nEOF\n",
    ] {
        assert!(parse_dxf(text, path).is_err(), "{text}");
    }
    let text=b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1021\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n\xff\n10\n1\n20\n2\n11\n3\n21\n4\n0\nENDSEC\n0\nEOF\n";
    assert!(parse_dxf_bytes(text, path).unwrap_err().contains("UTF-8"));
    let text = b"0\nSECTION\n2\nHEADER\n9\n$DWGCODEPAGE\n3\nANSI_1361\n0\nENDSEC\n0\nEOF\n";
    assert!(parse_dxf_bytes(text, path)
        .unwrap_err()
        .contains("codepagina"));
}

#[test]
fn utf8_bom_crlf_numeric_spaces_and_text_whitespace_are_preserved() {
    let src="\u{feff}0\r\nSECTION\r\n2\r\nENTITIES\r\n0\r\nLINE\r\n8\r\në\r\n10\r\n 1.25 \r\n20\r\n2\r\n11\r\n3\r\n21\r\n4\r\n0\r\nMULTILEADER\r\n304\r\n Name=PS1 \r\n10\r\n1\r\n20\r\n2\r\n0\r\nENDSEC\r\n0\r\nEOF\r\n";
    let drawing = parse_dxf_bytes(src.as_bytes(), Path::new("bom.dxf")).unwrap();
    assert_eq!(drawing.features[0].points[0].x, 1.25);
    assert_eq!(drawing.features[0].layer, "ë");
    assert_eq!(drawing.features[1].raw_text, " Name=PS1 ");
}
