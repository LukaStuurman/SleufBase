use sleufbase_rust::{
    export_control::Control,
    template_export::{self, Request},
};
use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(serde::Deserialize)]
struct Case {
    name: String,
    request: Request,
}
fn request(name: &str) -> Request {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/template-export/reference.json")).unwrap();
    let mut r = cases.into_iter().find(|c| c.name == name).unwrap().request;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    r.template = root.join(&r.template);
    for s in r.slots.iter_mut().flatten() {
        if !s.tiff_png.as_os_str().is_empty() {
            s.tiff_png = root.join(&s.tiff_png);
        }
        if let Some(source) = &mut s.tiff_source {
            source.path = root.join(&source.path);
        }
        s.map_png = root.join(&s.map_png);
    }
    r
}
fn files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn visit(root: &Path, result: &mut Vec<(PathBuf, Vec<u8>)>) {
        if !root.exists() {
            return;
        }
        for entry in fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, result)
            } else {
                result.push((path.clone(), fs::read(path).unwrap()));
            }
        }
    }
    let mut result = Vec::new();
    visit(root, &mut result);
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}
#[test]
fn completed_exports_keep_their_own_images_after_replacement_and_input_removal() {
    let temp = tempfile::tempdir().unwrap();
    let mut r = request("image-only");
    let tiff = temp.path().join("source-tiff.png");
    let map = temp.path().join("source-map.png");
    let s = r.slots.iter().flatten().next().unwrap();
    fs::copy(&s.tiff_png, &tiff).unwrap();
    fs::copy(&s.map_png, &map).unwrap();
    for s in r.slots.iter_mut().flatten() {
        s.tiff_png = tiff.clone();
        s.map_png = map.clone();
    }
    let output = temp.path().join("export.dxf");
    let first = template_export::export(&output, &r, &Control::default()).unwrap();
    assert_eq!(first.filled_slots, 2);
    let old = files(&first.published.assets);
    assert_eq!(old.len(), 4);
    let copied_drawing = temp.path().join("kept-copy.dxf");
    fs::copy(&output, &copied_drawing).unwrap();
    let second = template_export::export(&output, &r, &Control::default()).unwrap();
    assert_ne!(first.published.assets, second.published.assets);
    assert_eq!(files(&first.published.assets), old);
    fs::remove_file(tiff).unwrap();
    fs::remove_file(map).unwrap();
    for drawing in [output, copied_drawing] {
        let doc = sleufbase_rust::dxf_document::Document::load(
            &drawing,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        let pngs = doc
            .records()
            .filter(|r| r.kind() == "IMAGEDEF")
            .filter_map(|r| r.first(1))
            .filter(|p| p.contains("slot000"))
            .collect::<Vec<_>>();
        assert_eq!(pngs.len(), 4);
        for path in pngs {
            let image = image::open(path).unwrap();
            assert!(image.width() > 0 && image.height() > 0);
        }
    }
}
#[test]
fn cancellation_and_panic_after_writing_preserve_the_previous_drawing_and_assets() {
    let temp = tempfile::tempdir().unwrap();
    let r = request("mixed");
    let output = temp.path().join("export.dxf");
    template_export::export(&output, &r, &Control::default()).unwrap();
    let before = files(temp.path());
    let control = Control::default();
    let mut reached = false;
    assert!(
        template_export::export_with_progress(&output, &r, &control, |stage| {
            if stage == "Export definitief opslaan" {
                reached = true;
                assert!(control.cancel());
            }
        })
        .is_err()
    );
    assert!(reached);
    assert_eq!(files(temp.path()), before);
    let control = Control::default();
    let mut reached = false;
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        template_export::export_with_progress(&output, &r, &control, |stage| {
            if stage == "Export definitief opslaan" {
                reached = true;
                panic!("injected worker panic after final DXF write");
            }
        })
    }))
    .is_err());
    assert!(reached);
    assert_eq!(files(temp.path()), before);
    assert!(template_export::export(&output, &r, &Control::default()).is_ok());
}
#[test]
fn missing_or_corrupt_assets_and_late_profile_errors_never_publish_partial_exports() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("export.dxf");
    let base = request("mixed");
    template_export::export(&output, &base, &Control::default()).unwrap();
    let mut invalid = fs::read(&base.slots[0].as_ref().unwrap().map_png).unwrap();
    // Valid PNG signature/IHDR, broken pixel data: a dimensions-only reader
    // would accept it. The production assembler must decode and reject it.
    invalid.truncate(40);
    let bad = temp.path().join("corrupt.png");
    fs::write(&bad, invalid).unwrap();
    let before = files(temp.path());
    for mode in 0..5 {
        let mut r = request("mixed");
        match mode {
            0 => r.slots[0].as_mut().unwrap().map_png = temp.path().join("missing.png"),
            1 => r.slots[0].as_mut().unwrap().map_png = bad.clone(),
            2 => {
                r.slots[0]
                    .as_mut()
                    .unwrap()
                    .profile
                    .as_mut()
                    .unwrap()
                    .dataset = usize::MAX
            }
            3 => r.settings.export_date = Some([2026, 2, 31]),
            _ => r.slots[2].as_mut().unwrap().comments = "invalid\nraw control".into(),
        }
        assert!(
            template_export::export(&output, &r, &Control::default()).is_err(),
            "mode {mode}"
        );
        assert_eq!(files(temp.path()), before, "mode {mode}");
    }
}
#[test]
fn an_output_alias_cannot_replace_the_input_template_or_png() {
    let temp = tempfile::tempdir().unwrap();
    let mut r = request("image-only");
    let template = temp.path().join("source.dxf");
    fs::copy(&r.template, &template).unwrap();
    r.template = template.clone();
    let png = temp.path().join("source.png");
    let slot = r.slots.iter_mut().flatten().next().unwrap();
    fs::copy(&slot.tiff_png, &png).unwrap();
    slot.tiff_png = png.clone();
    let before = files(temp.path());
    for output in [template, png] {
        assert!(template_export::export(&output, &r, &Control::default()).is_err());
        assert_eq!(files(temp.path()), before);
    }
}
#[test]
fn native_tiff_preparation_uses_the_profile_axis_once_and_keeps_its_own_pixels() {
    let temp = tempfile::tempdir().unwrap();
    let mut r = request("native-tiff");
    let input = temp.path().join("source.tiff");
    std::fs::copy(
        &r.slots[0]
            .as_ref()
            .unwrap()
            .tiff_source
            .as_ref()
            .unwrap()
            .path,
        &input,
    )
    .unwrap();
    for s in r.slots.iter_mut().flatten() {
        s.tiff_source.as_mut().unwrap().path = input.clone();
    }
    let out =
        template_export::export(&temp.path().join("export.dxf"), &r, &Control::default()).unwrap();
    assert_eq!((out.filled_slots, out.profiles), (3, 2));
    std::fs::remove_file(input).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/template-export");
    for i in [0, 2, 3] {
        let native = image::open(
            out.published
                .assets
                .join(format!("slot{:05}_tiff.png", i + 1)),
        )
        .unwrap()
        .into_rgba8();
        let original = image::open(root.join(format!("native-tiff-slot{i:03}.png")))
            .unwrap()
            .into_rgba8();
        assert_eq!(native, original, "slot {i}");
    }
}
#[test]
fn native_tiff_errors_and_ambiguous_inputs_preserve_previous_outputs() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("export.dxf");
    let mut r = request("native-tiff");
    let input = temp.path().join("source.tiff");
    std::fs::copy(
        &r.slots[0]
            .as_ref()
            .unwrap()
            .tiff_source
            .as_ref()
            .unwrap()
            .path,
        &input,
    )
    .unwrap();
    for s in r.slots.iter_mut().flatten() {
        s.tiff_source.as_mut().unwrap().path = input.clone();
    }
    template_export::export(&output, &r, &Control::default()).unwrap();
    let bad = temp.path().join("bad.tiff");
    std::fs::write(&bad, b"broken TIFF").unwrap();
    let before = files(temp.path());
    assert!(template_export::export(&input, &r, &Control::default()).is_err());
    assert_eq!(files(temp.path()), before);
    r.slots[2]
        .as_mut()
        .unwrap()
        .tiff_source
        .as_mut()
        .unwrap()
        .path = bad;
    assert!(template_export::export(&output, &r, &Control::default()).is_err());
    assert_eq!(files(temp.path()), before);
    r.slots[0].as_mut().unwrap().tiff_png = input;
    assert!(template_export::export(&output, &r, &Control::default()).is_err());
    assert_eq!(files(temp.path()), before);
}
#[test]
fn empty_exports_are_rejected_and_manifest_paths_are_relative_to_the_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let mut r = request("image-only");
    r.slots = vec![None, None];
    assert!(
        template_export::export(&temp.path().join("empty.dxf"), &r, &Control::default()).is_err()
    );
    assert!(files(temp.path()).is_empty());
    let path = temp.path().join("request.json");
    fs::write(&path,r#"{"template":"template.dxf","rules":[],"slots":[{"bounds":{"min_x":1,"min_y":2,"max_x":3,"max_y":4},"label":"PS1","title":"PS1","address":null,"comments":"","tiff_png":"tiff.png","map_png":"map.png","profile":null}]}"#).unwrap();
    let r = template_export::read_request(&path, &Control::default()).unwrap();
    assert_eq!(r.template, temp.path().join("template.dxf"));
    assert_eq!(
        r.slots[0].as_ref().unwrap().tiff_png,
        temp.path().join("tiff.png")
    );
    let control = Control::default();
    control.cancel();
    assert!(template_export::read_request(&path, &control).is_err());
}
#[test]
fn native_map_pages_are_published_with_profiles_and_failures_keep_the_old_bundle() {
    use sleufbase_rust::{
        map_export::{Background, Source},
        map_page, map_text,
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/map-export");
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("export.dxf");
    let mut r = request("native-tiff");
    r.slots.truncate(3);
    for slot in r.slots.iter_mut().flatten() {
        slot.bounds = sleufbase_rust::Bounds {
            min_x: 0.,
            min_y: 0.,
            max_x: 11.,
            max_y: 6.,
        };
        slot.map_png = PathBuf::new();
        slot.map_source = Some(Source {
            settings: map_page::Settings {
                dpi: 96,
                scale: 225.,
            },
            background: Background::Solid {
                color: [123, 156, 189, 255],
            },
            fonts: Some(map_text::Paths {
                regular: root.join("Hack-Regular.ttf"),
                bold: root.join("Hack-Regular.ttf"),
            }),
            ..Default::default()
        });
    }
    let exported = template_export::export(&output, &r, &Control::default()).unwrap();
    assert_eq!((exported.filled_slots, exported.profiles), (2, 2));
    let map = image::open(exported.published.assets.join("slot00001_kaart.png"))
        .unwrap()
        .into_rgba8();
    assert_eq!(map.dimensions(), (1123, 794));
    assert!(map.pixels().any(|p| p.0 == [123, 156, 189, 255]));
    let before = files(temp.path());
    let control = Control::default();
    assert!(
        template_export::export_with_progress(&output, &r, &control, |stage| {
            if stage == "Kaartblad 3 opbouwen" {
                control.cancel();
            }
        })
        .is_err()
    );
    assert_eq!(files(temp.path()), before);
    let source = r.slots[2].as_mut().unwrap().map_source.as_mut().unwrap();
    source.fonts.as_mut().unwrap().bold = temp.path().join("missing.ttf");
    assert!(template_export::export(&output, &r, &Control::default()).is_err());
    assert_eq!(files(temp.path()), before);
    let source = r.slots[0].as_mut().unwrap().map_source.as_mut().unwrap();
    source.background = Background::Png {
        path: output.clone(),
    };
    assert!(template_export::export(&output, &r, &Control::default())
        .unwrap_err()
        .contains("verschilt"));
    assert_eq!(files(temp.path()), before);
    r.slots[0].as_mut().unwrap().map_png = output.clone();
    assert!(template_export::export(&output, &r, &Control::default())
        .unwrap_err()
        .contains("kaartbron"));
    assert_eq!(files(temp.path()), before);
}
#[test]
fn map_sources_resolve_all_paths_relative_to_the_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("request.json");
    fs::write(&path,r#"{"template":"template.dxf","rules":[],"slots":[{"bounds":{"min_x":1,"min_y":2,"max_x":3,"max_y":4},"label":"PS1","title":"PS1","address":null,"comments":"","tiff_png":"tiff.png","map_source":{"tiff":{"path":"source.tiff","transform":{"a":1,"b":0,"c":0,"d":0,"e":-1,"f":6},"axis":null},"background":{"kind":"png","path":"background.png"},"fonts":{"regular":"regular.ttf","bold":"bold.ttf"}},"profile":null}]}"#).unwrap();
    let r = template_export::read_request(&path, &Control::default()).unwrap();
    let source = r.slots[0].as_ref().unwrap().map_source.as_ref().unwrap();
    let expected =
        ["background.png", "source.tiff", "regular.ttf", "bold.ttf"].map(|s| temp.path().join(s));
    assert_eq!(
        source.input_paths(),
        expected.iter().map(PathBuf::as_path).collect::<Vec<_>>()
    );
}

#[test]
fn automatic_excel_replacement_keeps_the_previous_workbook_and_protects_sources_and_the_dxf() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("report.dxf");
    let mut r = request("image-only");
    r.settings.discipline_excel = true;
    r.rules = sleufbase_rust::RULES.to_vec();
    let exported = template_export::export(&output, &r, &Control::default()).unwrap();
    let xlsx = output.with_extension("xlsx");
    assert_eq!(exported.companion.as_deref(), Some(xlsx.as_path()));
    assert_eq!(exported.warnings.len(), exported.filled_slots);
    assert!(fs::metadata(&xlsx).unwrap().len() > 1000);
    let workbook = fs::read(&xlsx).unwrap();
    r.slots.iter_mut().flatten().next().unwrap().report_label = Some("PS99".into());
    let repeated = template_export::export(&output, &r, &Control::default()).unwrap();
    assert_eq!(repeated.companion.as_deref(), Some(xlsx.as_path()));
    assert_eq!(
        fs::read(repeated.published.assets.join("previous-disciplines.xlsx")).unwrap(),
        workbook
    );
    let latest = fs::read(&xlsx).unwrap();
    assert_ne!(latest, workbook);
    r.settings.protected_sources.push(xlsx.clone());
    let protected = template_export::export(&output, &r, &Control::default()).unwrap();
    assert!(protected.companion.is_none());
    assert!(protected.warnings.iter().any(|s| s.contains("projectbron")));
    assert_eq!(fs::read(&xlsx).unwrap(), latest);
    assert!(
        template_export::export(&dir.path().join("cancelled.dxf"), &r, &{
            let c = Control::default();
            c.cancel();
            c
        })
        .is_err()
    );
    assert!(!dir.path().join("cancelled.xlsx").exists());
    let same = dir.path().join("unusual.xlsx");
    let unusual = template_export::export(&same, &r, &Control::default()).unwrap();
    assert!(unusual.companion.is_none());
    assert!(fs::read_to_string(&same).unwrap().contains("SECTION"));
}
#[cfg(windows)]
#[test]
fn a_locked_previous_workbook_is_retained_while_the_dxf_finishes_successfully() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("locked.dxf");
    let xlsx = output.with_extension("xlsx");
    fs::write(&xlsx, b"previous workbook").unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&xlsx)
        .unwrap();
    let mut r = request("image-only");
    r.settings.discipline_excel = true;
    r.rules = sleufbase_rust::RULES.to_vec();
    let exported = template_export::export(&output, &r, &Control::default()).unwrap();
    assert!(output.exists() && exported.companion.is_none());
    assert!(exported
        .warnings
        .iter()
        .any(|s| s.contains("DXF opgeslagen")));
    assert!(!exported
        .published
        .assets
        .join("previous-disciplines.xlsx")
        .exists());
    drop(lock);
    assert_eq!(fs::read(xlsx).unwrap(), b"previous workbook");
}
#[test]
fn a_report_panic_or_missing_rules_does_not_claim_the_published_dxf_failed() {
    let dir = tempfile::tempdir().unwrap();
    let mut r = request("image-only");
    r.settings.discipline_excel = true;
    r.rules = sleufbase_rust::RULES.to_vec();
    let output = dir.path().join("panic.dxf");
    let exported = template_export::export_with_progress(&output, &r, &Control::default(), |s| {
        if s == "Excel-disciplineoverzicht opslaan" {
            panic!("report progress observer");
        }
    })
    .unwrap();
    assert!(exported.companion.is_none() && output.exists());
    assert!(exported.warnings[0].contains("DXF opgeslagen"));
    assert!(!output.with_extension("xlsx").exists());
    r.rules.clear();
    let empty = dir.path().join("empty-rules.dxf");
    let exported = template_export::export(&empty, &r, &Control::default()).unwrap();
    assert!(empty.exists() && exported.companion.is_none());
    assert!(exported.warnings[0].contains("geen disciplines"));
}
