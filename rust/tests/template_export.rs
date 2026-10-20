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
