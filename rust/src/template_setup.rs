//! Compose page extension, styles/layers, legends, drawing/image units, scale
//! list, title values and overview fitting as one cancelable preparation.
use crate::{
    dxf_document::{Change, Document, RecordView},
    input,
    template_copy::{self as copy},
    template_layout, template_legends,
    template_metadata::{self, Date, Values},
    template_pages, template_prepare, Bounds,
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::atomic::AtomicBool,
};
pub struct Options {
    pub project: Option<String>,
    pub drawn_by: String,
    pub export_date: Option<Date>,
    pub label_gap: f64,
    pub profile_layers: Option<Vec<template_prepare::LayerColor>>,
}
#[derive(Debug, serde::Serialize)]
pub struct Prepared {
    pub added_pages: usize,
    pub restored_legends: usize,
    pub pages: Vec<template_layout::Page>,
    /// Includes spare template capacity, ready for the subsequent drawing phase.
    pub slots: Vec<crate::template_slots::Slot>,
}
fn entries(r: RecordView<'_>) -> HashMap<String, u64> {
    let mut result = HashMap::new();
    let mut key = None;
    let mut active = false;
    let mut depth = 0;
    for (c, v) in r.tags() {
        if c == 102 {
            if v.starts_with('{') {
                depth += 1;
            } else if v == "}" {
                depth -= 1;
            }
            continue;
        }
        if depth != 0 {
            continue;
        }
        if c == 100 {
            active = v == "AcDbDictionary";
            continue;
        }
        if c == 101 || c >= 1000 {
            break;
        }
        if !active {
            continue;
        }
        if c == 3 {
            key = Some(v);
        } else if matches!(c, 350 | 360) {
            if let Some(k) = key.take() {
                if let Ok(id) = copy::id(v) {
                    result.insert(k.into(), id);
                }
            }
        }
    }
    result
}
fn settings(doc: &mut Document, cancel: &AtomicBool) -> Result<(), String> {
    input::check(cancel)?;
    let root = doc
        .records()
        .find(|r| {
            r.kind() == "DICTIONARY" && r.section() == Some("OBJECTS") && copy::owner(*r) == Some(0)
        })
        .ok_or("Hoofddictionary ontbreekt")?;
    let root_id = root.handle().ok_or("Hoofddictionary mist handle")?;
    let root_entries = entries(root);
    let mut changes = vec![doc.header_scalar("$INSUNITS", 70, "6")?];
    if let Some(&id) = root_entries.get("ACAD_IMAGE_VARS") {
        let r = doc
            .by_handle(&format!("{id:X}"))?
            .filter(|r| r.kind() == "RASTERVARIABLES")
            .ok_or("Ongeldige rasterinstellingen")?;
        for (c, v) in [(70, "0"), (71, "1"), (72, "3")] {
            changes.push(doc.tag_change(r.index(), c, v)?);
        }
    } else {
        let (next, mut seed) = doc.seed()?;
        let following = next.checked_add(1).ok_or("Geen vrije DXF-handles")?;
        seed.text = format!("{following:X}");
        changes.push(seed);
        let pointer = if copy::first(&copy::tags(root), 280).is_some_and(|v| v.trim() == "1") {
            360
        } else {
            350
        };
        let mut tags = copy::tags(root);
        tags.push((3, "ACAD_IMAGE_VARS".into()));
        tags.push((pointer, format!("{next:X}")));
        changes.push(doc.record_change(root.index(), copy::encode(&tags)));
        let at = doc.section_end("OBJECTS")?;
        changes.push(Change{start:at,end:at,text:format!("0\nRASTERVARIABLES\n5\n{next:X}\n102\n{{ACAD_REACTORS\n330\n{root_id:X}\n102\n}}\n330\n{root_id:X}\n100\nAcDbRasterVariables\n90\n0\n70\n0\n71\n1\n72\n3\n")});
    }
    if !doc
        .records()
        .any(|r| r.kind() == "CLASS" && r.first(1) == Some("RASTERVARIABLES"))
    {
        let count = doc
            .records()
            .filter(|r| r.kind() == "RASTERVARIABLES")
            .count()
            + usize::from(!root_entries.contains_key("ACAD_IMAGE_VARS"));
        let at = doc.section_end("CLASSES")?;
        changes.push(Change{start:at,end:at,text:format!("0\nCLASS\n1\nRASTERVARIABLES\n2\nAcDbRasterVariables\n3\nISM\n90\n0\n91\n{count}\n280\n0\n281\n0\n")});
    }
    if let Some(&id) = root_entries.get("ACAD_SCALELIST") {
        let d = doc
            .by_handle(&format!("{id:X}"))?
            .filter(|r| matches!(r.kind(), "DICTIONARY" | "ACDBDICTIONARYWDFLT"))
            .ok_or("Ongeldige schaallijst")?;
        let mut scale_changes = BTreeMap::new();
        let desired = [
            ("A0", 2000),
            ("A1", 1000),
            ("A2", 500),
            ("A3", 250),
            ("A8", 5000),
            ("A9", 10000),
        ]
        .into_iter()
        .collect::<HashMap<_, _>>();
        let mut tags = copy::tags(d);
        let mut active = false;
        let mut depth = 0;
        let mut finished = false;
        let mut keep = Vec::new();
        let mut i = 0;
        while i < tags.len() {
            let (c, v) = &tags[i];
            if *c == 102 {
                if v.starts_with('{') {
                    depth += 1;
                } else if v == "}" {
                    depth -= 1;
                }
            }
            if depth == 0 && (*c == 101 || *c >= 1000) {
                finished = true;
            }
            if depth == 0 && !finished && *c == 100 {
                active = v == "AcDbDictionary";
            }
            if depth == 0
                && !finished
                && active
                && *c == 3
                && i + 1 < tags.len()
                && matches!(tags[i + 1].0, 350 | 360)
            {
                if let Some(&denominator) = desired.get(v.as_str()) {
                    let h = copy::id(&tags[i + 1].1)?;
                    let r = doc
                        .by_handle(&format!("{h:X}"))?
                        .filter(|r| r.kind() == "SCALE")
                        .ok_or("Schaalobject ontbreekt of heeft onjuist type")?;
                    let mut scale = copy::tags(r);
                    let mut subclass = 0;
                    let mut depth = 0;
                    for (c, value) in &mut scale {
                        if *c == 102 {
                            if value.starts_with('{') {
                                depth += 1;
                            } else if value == "}" {
                                depth -= 1;
                            }
                            continue;
                        }
                        if depth != 0 {
                            continue;
                        }
                        if *c == 101 || *c >= 1000 {
                            break;
                        }
                        if *c == 100 {
                            subclass += 1;
                            continue;
                        }
                        if subclass == 1 {
                            match *c {
                                300 => *value = format!("1:{denominator}"),
                                140 => *value = "1".into(),
                                141 => *value = (denominator as f64 / 1000.).to_string(),
                                290 => *value = "0".into(),
                                _ => (),
                            }
                        }
                    }
                    scale_changes.insert(r.index(), copy::encode(&scale));
                    keep.push(tags[i].clone());
                    keep.push(tags[i + 1].clone());
                }
                i += 2;
            } else {
                keep.push(tags[i].clone());
                i += 1;
            }
        }
        tags = keep;
        changes.push(doc.record_change(d.index(), copy::encode(&tags)));
        changes.extend(
            scale_changes
                .into_iter()
                .map(|(i, text)| doc.record_change(i, text)),
        );
    }
    input::check(cancel)?;
    doc.apply(changes)
}
pub fn prepare(
    doc: &mut Document,
    slots: &[Option<Bounds>],
    options: &Options,
    cancel: &AtomicBool,
) -> Result<Prepared, String> {
    prepare_with_progress(doc, slots, options, cancel, |_| {})
}
/// Capture the local date once for all sheets; commit only when every phase
/// succeeds. Progress callbacks run on the export worker and may cancel work.
pub fn prepare_with_progress(
    doc: &mut Document,
    slots: &[Option<Bounds>],
    options: &Options,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&'static str),
) -> Result<Prepared, String> {
    input::check(cancel)?;
    let names = template_layout::used_layout_names(slots.len())?;
    let date = match options.export_date {
        Some(date) => date,
        None => Date::today()?,
    };
    let mut working = doc.indexed_copy(cancel)?;
    progress("Bladen uitbreiden");
    input::check(cancel)?;
    let extension = template_pages::extend(&mut working, slots.len(), cancel)?;
    crate::template_geometry::add(
        &mut working,
        &[crate::template_geometry::Layer {
            name: "PROEFSLEUF_TEMPLATE_IMAGES".into(),
            color: 7,
            true_color: None,
            lineweight: None,
        }],
        &[],
        cancel,
    )?;
    progress("Tekst- en profielstijlen voorbereiden");
    input::check(cancel)?;
    if let Some(layers) = &options.profile_layers {
        template_prepare::profile(
            &mut working,
            layers.iter().map(|l| (l.name.as_str(), l.color)),
            cancel,
        )?;
    } else {
        template_prepare::text_styles(&mut working, cancel)?;
    }
    progress("Legenda's herstellen");
    input::check(cancel)?;
    let restored_legends = template_legends::restore(&mut working, &names, cancel)?;
    progress("Tekeninginstellingen voorbereiden");
    input::check(cancel)?;
    settings(&mut working, cancel)?;
    progress("Titelvelden en vensters invullen");
    input::check(cancel)?;
    template_metadata::update(
        &mut working,
        &names,
        &Values {
            date,
            project: options.project.clone(),
            drawn_by: options.drawn_by.clone(),
        },
        cancel,
    )?;
    let pages = template_layout::configure(&mut working, slots, options.label_gap, cancel)?;
    let result = working.indexed_copy(cancel)?;
    input::check(cancel)?;
    *doc = result;
    Ok(Prepared {
        added_pages: extension.added_pages,
        restored_legends,
        pages,
        slots: extension.slots,
    })
}
