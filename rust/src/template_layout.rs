//! Used-sheet numbering and overview viewports from the production exporter.
//! Layout keys use uppercase (ezdxf Layouts), unlike layer/block lower keys.
use crate::{
    dxf_document::{Document, RecordView},
    python_values::uppercase,
    Bounds,
};
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::AtomicBool,
};

const SLOTS_PER_PAGE: usize = 8;
const SCALES: [u32; 6] = [250, 500, 1000, 2000, 5000, 10000];
#[derive(Debug, serde::Serialize)]
pub struct Page {
    pub layout: String,
    pub number: usize,
    pub scale: Option<u32>,
}
fn owner(r: RecordView<'_>) -> Option<u64> {
    let mut depth = 0;
    for (c, v) in r.tags().take_while(|(c, _)| *c != 100) {
        if c == 102 {
            if v.starts_with('{') {
                depth += 1;
            } else if v == "}" {
                depth -= 1;
            }
        }
        if c == 330 && depth == 0 {
            return u64::from_str_radix(v.trim(), 16).ok();
        }
    }
    None
}
fn value(r: RecordView<'_>, code: i32) -> Result<f64, String> {
    let n = r
        .current_value(code)?
        .unwrap_or("0")
        .trim()
        .parse::<f64>()
        .map_err(|_| "Ongeldige viewportmaat")?;
    if !n.is_finite() {
        return Err("Niet-eindige viewportmaat".into());
    }
    Ok(n)
}
fn valid(b: Bounds) -> bool {
    [b.min_x, b.min_y, b.max_x, b.max_y]
        .into_iter()
        .all(f64::is_finite)
        && b.min_x <= b.max_x
        && b.min_y <= b.max_y
        && (b.max_x - b.min_x).is_finite()
        && (b.max_y - b.min_y).is_finite()
}
pub fn used_layout_names(slot_count: usize) -> Result<Vec<String>, String> {
    if slot_count > 100_000 {
        return Err("Te veel sjabloonvakken".into());
    }
    Ok((1..=slot_count.div_ceil(SLOTS_PER_PAGE).max(1))
        .map(|n| format!("Blad{n}"))
        .collect())
}
/// Empty slots keep their position. Number the first INSERT of each existing
/// used layout and fit its lowest KADER viewport to its filled slot bounds.
/// Missing layouts/viewports are retained/skipped as in the original helper;
/// creating/extending pages is a separate stage of the export pipeline.
pub fn configure(
    doc: &mut Document,
    slots: &[Option<Bounds>],
    label_gap: f64,
    cancel: &AtomicBool,
) -> Result<Vec<Page>, String> {
    crate::input::check(cancel)?;
    let names = used_layout_names(slots.len())?;
    if !label_gap.is_finite() || slots.iter().flatten().any(|b| !valid(*b)) {
        return Err("Ongeldige sjabloonbegrenzing of labelafstand".into());
    }
    let wanted: HashMap<_, _> = names
        .iter()
        .enumerate()
        .map(|(i, n)| (uppercase(n), i))
        .collect();
    let mut layout_ids = HashMap::new();
    let mut seen_pages = HashSet::new();
    for (i, r) in doc.records().enumerate() {
        if i % 1024 == 0 {
            crate::input::check(cancel)?;
        }
        if r.kind() == "LAYOUT" {
            if let Some(name) = r.tags().filter(|(c, _)| *c == 1).last().map(|(_, v)| v) {
                if let (Some(&page), Some(id)) = (wanted.get(&uppercase(name)), r.handle()) {
                    if !seen_pages.insert(page) {
                        return Err("Dubbele sjabloonlayout".into());
                    }
                    layout_ids.insert(id, page);
                }
            }
        }
    }
    let mut spaces = HashMap::new();
    seen_pages.clear();
    for r in doc.records().filter(|r| r.kind() == "BLOCK_RECORD") {
        if let (Some(id), Some(page)) = (
            r.handle(),
            r.first(340)
                .and_then(|v| u64::from_str_radix(v.trim(), 16).ok())
                .and_then(|id| layout_ids.get(&id)),
        ) {
            if !seen_pages.insert(*page) {
                return Err("Dubbele layoutruimte".into());
            }
            spaces.insert(id, *page);
        }
    }
    let mut viewports: Vec<Option<RecordView<'_>>> = vec![None; names.len()];
    let mut titles: Vec<Option<Vec<RecordView<'_>>>> = vec![None; names.len()];
    let mut records = doc.records().peekable();
    while let Some(r) = records.next() {
        if r.index() % 1024 == 0 {
            crate::input::check(cancel)?;
        }
        if !matches!(r.section(), Some("ENTITIES" | "BLOCKS")) {
            continue;
        }
        let Some(&page) = owner(r).and_then(|id| spaces.get(&id)) else {
            continue;
        };
        if r.kind() == "VIEWPORT" && r.first(8) == Some("KADER") {
            let previous = viewports[page];
            let key = (value(r, 20)?, value(r, 41)?);
            if previous.is_none()
                || key < (value(previous.unwrap(), 20)?, value(previous.unwrap(), 41)?)
            {
                viewports[page] = Some(r);
            }
        } else if r.kind() == "INSERT" && titles[page].is_none() {
            let mut attributes = Vec::new();
            while records.peek().is_some_and(|a| a.kind() == "ATTRIB") {
                if attributes.len() >= 10_000 {
                    return Err("Te veel titelblokattributen".into());
                }
                attributes.push(records.next().unwrap());
            }
            titles[page] = Some(attributes);
        }
    }
    let mut changes = Vec::new();
    let mut result = Vec::with_capacity(names.len());
    for (page, name) in names.iter().enumerate() {
        crate::input::check(cancel)?;
        let start = page * SLOTS_PER_PAGE;
        let mut bounds: Option<Bounds> = None;
        for &b in slots
            .get(start..slots.len().min(start + SLOTS_PER_PAGE))
            .unwrap_or(&[])
            .iter()
            .flatten()
        {
            if let Some(current) = &mut bounds {
                current.min_x = current.min_x.min(b.min_x);
                current.min_y = current.min_y.min(b.min_y);
                current.max_x = current.max_x.max(b.max_x);
                current.max_y = current.max_y.max(b.max_y);
            } else {
                bounds = Some(b);
            }
        }
        let mut scale = None;
        if let (Some(viewport), Some(_), Some(b)) = (viewports[page], &titles[page], bounds) {
            let pad = (label_gap + 12.).max(2.);
            let b = Bounds {
                min_x: b.min_x - pad,
                min_y: b.min_y - pad,
                max_x: b.max_x + pad,
                max_y: b.max_y + pad,
            };
            let width = value(viewport, 40)?;
            let height = value(viewport, 41)?;
            if !valid(b) || width <= 0. || height <= 0. {
                return Err("Ongeldige overzichtsviewport".into());
            }
            let denominator = *SCALES
                .iter()
                .find(|&&d| {
                    b.max_x - b.min_x <= width * d as f64 / 1000.
                        && b.max_y - b.min_y <= height * d as f64 / 1000.
                })
                .unwrap_or(SCALES.last().unwrap());
            let fields = [
                (12, (b.min_x + b.max_x) * 0.5),
                (22, (b.min_y + b.max_y) * 0.5),
                (16, 0.),
                (26, 0.),
                (36, 1.),
                (17, 0.),
                (27, 0.),
                (37, 0.),
                (51, 0.),
                (45, height * denominator as f64 / 1000.),
            ];
            if fields.iter().any(|(_, v)| !v.is_finite()) {
                return Err("Viewportcoördinaten lopen over".into());
            }
            changes.push(
                doc.record_scalars(viewport.index(), &fields.map(|(c, v)| (c, v.to_string())))?,
            );
            scale = Some(denominator);
        }
        if let Some(attributes) = &titles[page] {
            for a in attributes {
                let text = match a.first(2).unwrap_or("") {
                    "BLAD" => Some((page + 1).to_string()),
                    "BLADEN" => Some(names.len().to_string()),
                    "SCHAAL" => scale.map(|s| format!("1:{s}")),
                    _ => None,
                };
                if let Some(text) = text {
                    changes.push(doc.tag_change(a.index(), 1, &text)?);
                }
            }
        }
        result.push(Page {
            layout: name.clone(),
            number: page + 1,
            scale,
        });
    }
    crate::input::check(cancel)?;
    drop(records);
    drop(viewports);
    drop(titles);
    doc.apply(changes)?;
    Ok(result)
}
