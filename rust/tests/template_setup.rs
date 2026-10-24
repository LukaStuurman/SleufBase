use sleufbase_rust::{
    dxf_document::Document,
    template_geometry::{self, Entity, Paint, Shape},
    template_images::{self, Image},
    template_legends,
    template_metadata::Date,
    template_prepare::LayerColor,
    template_setup::{self, Options},
    Bounds,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}
fn source() -> Document {
    Document::load(
        &root().join("rust/tests/fixtures/template-legends/affine-2.dxf"),
        &AtomicBool::new(false),
    )
    .unwrap()
}
fn bytes(d: &Document) -> Vec<u8> {
    let mut b = Vec::new();
    d.write_to(&mut b, &AtomicBool::new(false)).unwrap();
    b
}
fn slots(n: usize) -> Vec<Option<Bounds>> {
    vec![
        Some(Bounds {
            min_x: 120000.,
            min_y: 450000.,
            max_x: 120030.,
            max_y: 450040.
        });
        n
    ]
}
fn base_options() -> Options {
    Options {
        project: Some("Project Ø 漢".into()),
        drawn_by: "User".into(),
        export_date: Some(Date::new(2026, 10, 19).unwrap()),
        label_gap: 18.,
        profile_layers: None,
    }
}
#[test]
fn duplicate_layout_aliases_and_repeated_legend_restoration_do_not_duplicate_content() {
    let mut doc = source();
    let cancel = AtomicBool::new(false);
    let names = ["BLAD1", "Blad2", "BLAD2", "Blad3", "Blad4", "missing"].map(String::from);
    assert_eq!(
        template_legends::restore(&mut doc, &names, &cancel).unwrap(),
        2
    );
    let before = bytes(&doc);
    assert_eq!(
        template_legends::restore(&mut doc, &names, &cancel).unwrap(),
        0
    );
    assert_eq!(bytes(&doc), before);
}
#[test]
fn completed_preparation_is_idempotent_and_retains_live_slot_handles() {
    let mut doc = source();
    let cancel = AtomicBool::new(false);
    let bounds = slots(17);
    let options = base_options();
    let result = template_setup::prepare(&mut doc, &bounds, &options, &cancel).unwrap();
    assert!(result.slots.len() >= 17);
    assert_eq!(result.pages.len(), 3);
    for slot in result.slots {
        assert!(doc.by_handle(&slot.label_handle).unwrap().is_some());
    }
    let before = bytes(&doc);
    let result = template_setup::prepare(&mut doc, &bounds, &options, &cancel).unwrap();
    assert_eq!(result.added_pages, 0);
    assert_eq!(result.restored_legends, 0);
    assert_eq!(bytes(&doc), before);
}
#[test]
fn late_cancellation_panic_and_invalid_options_roll_back_every_completed_phase() {
    let mut doc = source();
    let before = bytes(&doc);
    let cancel = AtomicBool::new(false);
    let bounds = slots(17);
    let options = base_options();
    let mut calls = 0;
    assert!(
        template_setup::prepare_with_progress(&mut doc, &bounds, &options, &cancel, |_| {
            calls += 1;
            if calls == 5 {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .is_err()
    );
    assert_eq!(calls, 5);
    assert_eq!(bytes(&doc), before);
    cancel.store(false, Ordering::Relaxed);
    let mut calls = 0;
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        template_setup::prepare_with_progress(&mut doc, &bounds, &options, &cancel, |_| {
            calls += 1;
            if calls == 5 {
                panic!("worker panic after page/style/legend/settings work");
            }
        })
    }))
    .is_err());
    assert_eq!(bytes(&doc), before);
    let bad = Options {
        project: Some("bad\nproject".into()),
        ..options
    };
    assert!(template_setup::prepare(&mut doc, &bounds, &bad, &cancel).is_err());
    assert_eq!(bytes(&doc), before);
    let bad = Options {
        profile_layers: Some(vec![LayerColor {
            name: "Invalid".into(),
            color: 0,
        }]),
        ..base_options()
    };
    assert!(template_setup::prepare(&mut doc, &bounds, &bad, &cancel).is_err());
    assert_eq!(bytes(&doc), before);
    assert!(template_setup::prepare(&mut doc, &bounds, &base_options(), &cancel).is_ok());
}
#[test]
fn later_geometry_and_rasters_reuse_new_layouts_layers_styles_and_image_variables() {
    let cancel = AtomicBool::new(false);
    let mut doc = Document::load(&root().join("assets/cadastral_template.dxf"), &cancel).unwrap();
    template_setup::prepare(&mut doc, &slots(40), &base_options(), &cancel).unwrap();
    let variable_count = doc
        .records()
        .filter(|r| r.kind() == "RASTERVARIABLES")
        .count();
    assert_eq!(variable_count, 1);
    template_geometry::add(
        &mut doc,
        &[],
        &[Entity {
            layout: Some("Blad5".into()),
            layer: "0".into(),
            paint: Paint::default(),
            shape: Shape::Circle {
                center: [2., 3.],
                radius: 1.,
            },
        }],
        &cancel,
    )
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("image.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]))
        .save(&path)
        .unwrap();
    template_images::add_images(
        &mut doc,
        &[Image {
            path,
            bounds: Bounds {
                min_x: 3.,
                min_y: 4.,
                max_x: 5.,
                max_y: 6.,
            },
            definition_name: "AFTER_PREPARATION".into(),
            inset: 0.,
            preserve_aspect: false,
            layout: Some("Blad5".into()),
        }],
        &cancel,
    )
    .unwrap();
    let reloaded = Document::from_bytes(bytes(&doc), &cancel).unwrap();
    assert_eq!(
        reloaded
            .records()
            .filter(|r| r.kind() == "RASTERVARIABLES")
            .count(),
        variable_count
    );
    assert_eq!(
        reloaded.records().filter(|r| r.kind() == "IMAGE").count(),
        1
    );
}
#[cfg(windows)]
#[test]
fn automatic_local_date_is_captured_once_for_every_used_title() {
    let mut doc = source();
    let cancel = AtomicBool::new(false);
    let before = Date::today().unwrap().to_string();
    let options = Options {
        export_date: None,
        ..base_options()
    };
    template_setup::prepare(&mut doc, &slots(9), &options, &cancel).unwrap();
    let after = Date::today().unwrap().to_string();
    let dates = doc
        .records()
        .filter(|r| r.kind() == "ATTRIB" && r.first(2) == Some("DATUM1"))
        .map(|r| r.first(1).unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(dates.len(), 2);
    assert!(dates[0] == before || dates[0] == after);
    assert!(dates.iter().all(|d| d == &dates[0]));
}
