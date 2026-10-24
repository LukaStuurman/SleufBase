//! Sequential original page-number/viewport reference audit.
use sleufbase_rust::{
    dxf_document::Document,
    storage,
    template_geometry::{self, Entity, Paint, Shape},
    template_images::{self, Image},
    template_layout,
    template_metadata::{self, Date, Values},
    Bounds,
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
#[derive(serde::Deserialize)]
struct Case {
    name: String,
    path: String,
    steps: Vec<Step>,
}
#[derive(serde::Deserialize)]
struct Step {
    slots: Vec<Option<Bounds>>,
    label_gap: f64,
}
fn main() -> Result<(), String> {
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("fresh output directory required")?,
    );
    if output.exists() {
        return Err("Output already exists".into());
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let cancel = AtomicBool::new(false);
    let cases: Vec<Case> = serde_json::from_str(include_str!(
        "../tests/fixtures/template-layout/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    for case in cases {
        let mut doc = Document::load(&root.join(case.path), &cancel)?;
        for (i, step) in case.steps.into_iter().enumerate() {
            let pages = template_layout::configure(&mut doc, &step.slots, step.label_gap, &cancel)?;
            storage::atomic_write(
                &output.join(format!("{}-{i}.dxf", case.name)),
                false,
                |out| doc.write_to(out, &cancel),
            )?;
            storage::atomic_write(
                &output.join(format!("{}-{i}.json", case.name)),
                false,
                |out| serde_json::to_writer(out, &pages).map_err(|e| e.to_string()),
            )?;
        }
    }
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/template-layout/unicode-reference.json"
    ))
    .map_err(|e| e.to_string())?;
    let aliases = fixture["aliases"]
        .as_array()
        .ok_or("aliases")?
        .iter()
        .map(|n| n.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let mut doc = Document::load(
        &root.join("rust/tests/fixtures/template-layout/sheets.dxf"),
        &cancel,
    )?;
    let raster = output.join("unicode.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([12, 34, 56, 255]))
        .save(&raster)
        .map_err(|e| e.to_string())?;
    template_metadata::update(
        &mut doc,
        &aliases,
        &Values {
            date: Date::new(2026, 10, 19)?,
            project: Some("Unicode layouts".into()),
            drawn_by: String::new(),
        },
        &cancel,
    )?;
    let entities = aliases
        .iter()
        .enumerate()
        .map(|(i, name)| Entity {
            layer: "0".into(),
            paint: Paint::default(),
            layout: Some(name.clone()),
            shape: Shape::Circle {
                center: [i as f64, 2.],
                radius: 1.,
            },
        })
        .collect::<Vec<_>>();
    template_geometry::add(&mut doc, &[], &entities, &cancel)?;
    let images = aliases
        .iter()
        .enumerate()
        .map(|(i, name)| Image {
            path: raster.clone(),
            bounds: Bounds {
                min_x: i as f64,
                min_y: 2.,
                max_x: i as f64 + 2.,
                max_y: 4.,
            },
            definition_name: format!("unicode_{i}"),
            inset: 0.,
            preserve_aspect: false,
            layout: Some(name.clone()),
        })
        .collect::<Vec<_>>();
    template_images::add_images(&mut doc, &images, &cancel)?;
    storage::atomic_write(&output.join("unicode.dxf"), false, |out| {
        doc.write_to(out, &cancel)
    })?;
    Ok(())
}
