//! Independent map audit driver. Writes fresh output, never changes fixtures.
use image::{Rgba, RgbaImage};
use serde_json::{json, Value};
use sleufbase_rust::{map_export, map_page, map_raster, map_text};
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> T {
    serde_json::from_value(v.clone()).unwrap()
}
fn main() -> Result<(), String> {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("fresh output directory required")?,
    );
    if out.exists() {
        return Err("Output already exists".into());
    }
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/map-export");
    let cases: Value = serde_json::from_slice(
        &std::fs::read(root.join("reference.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let cancel = AtomicBool::new(false);
    let load = |c: &Value| {
        image::open(root.join(c["input"].as_str().unwrap()))
            .unwrap()
            .into_rgba8()
    };
    for c in cases["resizes"].as_array().unwrap() {
        map_raster::resize(&load(c), parse(&c["crop"]), parse(&c["size"]), &cancel)?
            .save(out.join(c["expected"].as_str().unwrap()))
            .map_err(|e| e.to_string())?;
    }
    for c in cases["paints"].as_array().unwrap() {
        let l: map_page::Layout = parse(&c["layout"]);
        let mut image =
            RgbaImage::from_pixel(l.map_size[0], l.map_size[1], Rgba(parse(&c["background"])));
        map_raster::paint(&mut image, load(c), parse(&c["transform"]), &l, &cancel)?;
        image
            .save(out.join(c["expected"].as_str().unwrap()))
            .map_err(|e| e.to_string())?;
    }
    let paths = map_text::Paths {
        regular: root.join("Hack-Regular.ttf"),
        bold: root.join("Hack-Regular.ttf"),
    };
    let mut fonts = map_text::Fonts::load(Some(&paths), &cancel)?;
    let mut output = json!({"pages":[],"texts":[]});
    for c in cases["pages"].as_array().unwrap() {
        let layout = map_page::layout(parse(&c["bounds"]), parse(&c["settings"]))?;
        let ops = map_page::plan(
            &layout,
            c["attribution"].as_str(),
            parse(&c["reference"]),
            &mut |t, s, b| {
                let m = c["measures"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|m| {
                        m["text"] == t && m["size"].as_f64() == Some(f64::from(s)) && m["bold"] == b
                    })
                    .unwrap();
                Ok(parse(&m["bbox"]))
            },
            &cancel,
        )?;
        output["pages"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":c["name"],"layout":layout,"ops":ops}));
        if c["name"].as_str().unwrap().starts_with("rd") {
            let source = map_export::Source {
                background: map_export::Background::Solid {
                    color: [190, 210, 180, 255],
                },
                include_tiff: false,
                attribution: parse(&c["attribution"]),
                reference: parse(&c["reference"]),
                fonts: Some(paths.clone()),
                ..Default::default()
            };
            map_export::Renderer::default()
                .render(&source, parse(&c["bounds"]), None, None, &cancel)?
                .image
                .save(out.join(format!("{}.png", c["name"].as_str().unwrap())))
                .map_err(|e| e.to_string())?;
        }
    }
    for c in cases["texts"].as_array().unwrap() {
        output["texts"].as_array_mut().unwrap().push(json!({"text":c["text"],"size":c["size"],"bold":c["bold"],"bbox":fonts.measure(c["text"].as_str().unwrap(),parse(&c["size"]),parse(&c["bold"]))?}));
    }
    std::fs::write(
        out.join("audit.json"),
        serde_json::to_vec_pretty(&output).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
