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
#[test]
fn actual_shape_fonts_fallback_order_family_and_duplicate_folder_priority() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(false);
    let shp = include_bytes!("fixtures/stroke-font/fixture.shp");
    let shx = include_bytes!("fixtures/stroke-font/fixture-1.0.shx");
    for name in ["txt.shp", "iso.shp", "fixture.shp"] {
        std::fs::write(first.path().join(name), shp).unwrap();
    }
    std::fs::write(first.path().join("fixture.shx"), shx).unwrap();
    std::fs::write(
        second.path().join("txt.shp"),
        include_bytes!("fixtures/stroke-font/derived.shp"),
    )
    .unwrap();
    std::fs::write(first.path().join("Arial.ttf"), FONT).unwrap();
    let mut c = Catalog::from_dirs(
        &[first.path().to_owned(), second.path().to_owned()],
        &cancel,
    )
    .unwrap();
    for name in ["missing.shx", "missing.shp"] {
        assert_eq!(
            c.selected_path(name),
            Some(second.path().join("txt.shp").as_path())
        );
    }
    assert_eq!(
        c.selected_path("fixture"),
        Some(first.path().join("fixture.shx").as_path())
    );
    assert_eq!(
        c.selected_path("missing.ttf"),
        Some(first.path().join("Arial.ttf").as_path())
    );
    for name in ["fixture", "fixture.shp", "missing.shx"] {
        let mut f = c.resolve(name, &cancel).unwrap();
        let e = f
            .extent(
                &Text {
                    content: "Axp".into(),
                    ..Default::default()
                },
                &cancel,
            )
            .unwrap();
        assert!(e.width > 0. && e.width.is_finite());
    }
    // Real filesystem paths are protected export inputs, including fallback fonts.
    assert!(c.source_paths().any(|p| p == second.path().join("txt.shp")));
}
#[test]
fn support_folder_limit_excludes_the_builtin_system_folders() {
    let dir = tempfile::tempdir().unwrap();
    let folders = vec![dir.path().join("not-present"); 64];
    assert!(Catalog::system_with_dirs(&folders, &AtomicBool::new(false)).is_ok());
    assert!(
        Catalog::system_with_dirs(&vec![dir.path().to_owned(); 65], &AtomicBool::new(false))
            .is_err()
    );
}
