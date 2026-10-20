use sleufbase_rust::{dxf_fonts::Catalog, dxf_text_metrics::Text};
use std::sync::atomic::AtomicBool;
const FONT: &[u8] = include_bytes!("fixtures/dxf-text-metrics/SleufBaseFixture.ttf");
#[test]
fn filenames_family_prefix_fallback_and_duplicate_directory_priority() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    for path in [
        a.path().join("ArialUni.ttf"),
        a.path().join("specific.ttf"),
        b.path().join("specific.ttf"),
    ] {
        std::fs::write(path, FONT).unwrap();
    }
    std::fs::write(a.path().join("corrupt.ttf"), b"not a font").unwrap();
    let cancel = AtomicBool::new(false);
    let mut c = Catalog::from_dirs(&[a.path().to_owned(), b.path().to_owned()], &cancel).unwrap();
    assert_eq!(
        c.selected_path("SPECIFIC.TTF"),
        Some(b.path().join("specific.ttf").as_path())
    );
    for name in [
        "missing.ttf",
        "corrupt.ttf",
        "missing.shx",
        "unknown.invalid",
    ] {
        assert_eq!(
            c.selected_path(name),
            Some(a.path().join("ArialUni.ttf").as_path())
        );
    }
    assert!(c.selected_path("SleufBase Extent").is_some());
    assert_eq!(c.selected_path("*monospace"), None);
    for _ in 0..2 {
        assert!(
            c.resolve("specific.ttf", &cancel)
                .unwrap()
                .extent(
                    &Text {
                        content: "Axp".into(),
                        ..Default::default()
                    },
                    &cancel
                )
                .unwrap()
                .width
                > 0.
        );
    }
}
#[test]
fn known_shape_fonts_are_not_silently_replaced_and_cancellation_is_immediate() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("txt.shx"), b"fixture shape metadata only").unwrap();
    std::fs::write(dir.path().join("Arial.ttf"), FONT).unwrap();
    let mut c = Catalog::from_dirs(&[dir.path().to_owned()], &AtomicBool::new(false)).unwrap();
    assert!(c.resolve("txt", &AtomicBool::new(false)).is_err());
    assert!(c.resolve("Arial.ttf", &AtomicBool::new(true)).is_err());
    assert!(Catalog::from_dirs(&[dir.path().to_owned()], &AtomicBool::new(true)).is_err());
    assert!(Catalog::from_dirs(&[], &AtomicBool::new(false))
        .unwrap()
        .resolve("missing.ttf", &AtomicBool::new(false))
        .is_ok());
}
