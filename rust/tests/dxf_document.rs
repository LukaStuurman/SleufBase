use sleufbase_rust::dxf_document::Document;
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn source(newline: &str) -> Vec<u8> {
    "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nTEXT\n5\nA\n330\nF\n100\nAcDbEntity\n8\nKADER\n100\nAcDbText\n10\n1\n20\n2\n40\n0.1\n1\noriginal\n0\nMTEXT\n5\nB\n100\nAcDbEntity\n8\nKADER\n100\nAcDbMText\n10\n0\n20\n0\n40\n0.1\n3\nold chunk\n1\nold tail\n0\nVENDOR_OBJECT\n5\nC\n100\nVendorClass\n1\nopaque content\n310\n00F9FF\n0\nENDSEC\n0\nSECTION\n2\nOBJECTS\n0\nXRECORD\n5\nD\n100\nAcDbXrecord\n5\nA\n1\nopaque reference\n0\nENDSEC\n0\nEOF\n".replace('\n',newline).into_bytes()
}
#[test]
fn unchanged_modern_source_is_byte_identical_including_unknown_records() {
    for bytes in [
        source("\n"),
        source("\r\n"),
        [vec![0xef, 0xbb, 0xbf], source("\n")].concat(),
    ] {
        let doc = Document::from_bytes(bytes.clone(), &AtomicBool::new(false)).unwrap();
        assert_eq!(doc.by_handle("c").unwrap().unwrap().kind(), "VENDOR_OBJECT");
        assert_eq!(
            doc.by_handle("D").unwrap().unwrap().section(),
            Some("OBJECTS")
        );
        let mut output = Vec::new();
        doc.write_to(&mut output, &AtomicBool::new(false)).unwrap();
        assert_eq!(output, bytes);
    }
}
#[test]
fn text_edits_keep_opaque_data_and_cannot_inject_tags() {
    for nl in ["\n", "\r\n"] {
        let bytes = source(nl);
        let mut doc = Document::from_bytes(bytes, &AtomicBool::new(false)).unwrap();
        doc.replace_text("a", "Profiel Ø α").unwrap();
        let text = "漢😀".repeat(125) + "\t" + &"漢😀".repeat(208) + "\\Ptekst\t";
        doc.replace_text("B", &text).unwrap();
        assert!(doc.replace_text("B", "bad\n0\nEOF").is_err());
        assert!(doc.replace_text("B", "bad\0field").is_err());
        assert!(doc.replace_text("A", "bad\tfield").is_err());
        assert!(doc.replace_text("B", &"x".repeat(1024 * 1024 + 1)).is_err());
        assert!(doc.replace_text("C", "wrong type").is_err());
        doc.replace_text("a", "Finale tekst").unwrap();
        let mut output = Vec::new();
        doc.write_to(&mut output, &AtomicBool::new(false)).unwrap();
        let reloaded = Document::from_bytes(output, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            reloaded.by_handle("A").unwrap().unwrap().first(1),
            Some("Finale tekst")
        );
        let tags: Vec<_> = reloaded
            .by_handle("B")
            .unwrap()
            .unwrap()
            .tags()
            .filter(|(c, _)| matches!(c, 1 | 3))
            .collect();
        assert_eq!(tags.iter().map(|(_, s)| *s).collect::<String>(), text);
        assert!(tags.iter().all(|(_, s)| s.chars().count() <= 250));
        assert!(tags[..tags.len() - 1].iter().all(|(c, _)| *c == 3));
        assert_eq!(tags.last().unwrap().0, 1);
        assert_eq!(
            reloaded.by_handle("C").unwrap().unwrap().first(310),
            Some("00F9FF")
        );
        assert_eq!(
            reloaded
                .by_handle("D")
                .unwrap()
                .unwrap()
                .tags()
                .filter(|(c, _)| *c == 5)
                .count(),
            2
        );
    }
}
#[test]
fn actual_template_round_trip_keeps_every_source_byte() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/cadastral_template.dxf");
    let bytes = std::fs::read(path).unwrap();
    let doc = Document::from_bytes(bytes.clone(), &AtomicBool::new(false)).unwrap();
    assert!(doc.records().any(|r| r.kind() == "LAYOUT"));
    assert!(doc.records().any(|r| r.kind() == "ACAD_PROXY_OBJECT"));
    let mut output = Vec::new();
    doc.write_to(&mut output, &AtomicBool::new(false)).unwrap();
    assert_eq!(output, bytes);
}
#[test]
fn corrupt_unsupported_and_cancelled_sources_are_errors() {
    let cancel = AtomicBool::new(false);
    for bytes in [
        b"0\nEOF\n".to_vec(),
        source("\n").split_off(20),
        String::from_utf8(source("\n"))
            .unwrap()
            .replace("5\nB\n", "5\nA\n")
            .into_bytes(),
        String::from_utf8(source("\n"))
            .unwrap()
            .replace("AC1032", "AC1015")
            .into_bytes(),
        b"0\nSECTION\n2\nHEADER\n0\nEOF\n".to_vec(),
        [source("\n"), b"1\ntrailing data\n".to_vec()].concat(),
    ] {
        assert!(Document::from_bytes(bytes, &cancel).is_err());
    }
    assert!(Document::from_bytes(source("\n"), &AtomicBool::new(true)).is_err());
    let doc = Document::from_bytes(source("\n"), &cancel).unwrap();
    let mut output = Vec::new();
    assert!(doc.write_to(&mut output, &AtomicBool::new(true)).is_err());
    assert!(output.is_empty());
}

#[test]
fn actual_template_writes_stop_between_chunks_when_cancelled() {
    use std::{
        io::{self, Write},
        sync::atomic::Ordering,
    };
    struct Sink<'a> {
        cancel: &'a AtomicBool,
        bytes: usize,
    }
    impl Write for Sink<'_> {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            self.bytes += data.len();
            self.cancel.store(true, Ordering::Relaxed);
            Ok(data.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let cancel = AtomicBool::new(false);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/cadastral_template.dxf");
    let document = Document::load(&path, &cancel).unwrap();
    let mut sink = Sink {
        cancel: &cancel,
        bytes: 0,
    };
    assert!(document
        .write_to(&mut sink, &cancel)
        .unwrap_err()
        .contains("geannuleerd"));
    assert_eq!(sink.bytes, 64 * 1024);
}
