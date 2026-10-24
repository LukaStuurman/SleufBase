//! Original cadastral template slot recognition, without rendering the drawing.
use crate::{
    dxf_document::{Document, RecordView},
    model::Bounds,
    python_values::trim,
};
use std::{collections::HashSet, sync::atomic::AtomicBool};
#[derive(Clone, Debug, serde::Serialize)]
pub struct Slot {
    pub label_handle: String,
    pub address_handle: Option<String>,
    pub comments_handle: Option<String>,
    pub row_box: Bounds,
    pub tiff_box: Bounds,
    pub map_box: Bounds,
    pub sort_x: f64,
    pub sort_y: f64,
}
fn cx(b: Bounds) -> f64 {
    (b.min_x + b.max_x) * 0.5
}
fn cy(b: Bounds) -> f64 {
    (b.min_y + b.max_y) * 0.5
}
fn polyline_bounds(r: RecordView<'_>) -> Option<Bounds> {
    let mut x = None;
    let mut bounds: Option<Bounds> = None;
    for (code, value) in r.tags().take_while(|(c, _)| *c < 1000) {
        if code == 10 {
            x = value.trim().parse::<f64>().ok();
        }
        if code == 20 {
            if let (Some(x), Ok(y)) = (x.take(), value.trim().parse::<f64>()) {
                if let Some(b) = &mut bounds {
                    b.min_x = b.min_x.min(x);
                    b.max_x = b.max_x.max(x);
                    b.min_y = b.min_y.min(y);
                    b.max_y = b.max_y.max(y);
                } else {
                    bounds = Some(Bounds {
                        min_x: x,
                        min_y: y,
                        max_x: x,
                        max_y: y,
                    });
                }
            }
        }
    }
    bounds
}
fn owner(r: RecordView<'_>) -> Option<u64> {
    let mut depth = 0;
    for (code, value) in r.tags() {
        if code == 100 {
            break;
        }
        if code == 102 {
            if value.starts_with('{') {
                depth += 1;
            } else if value == "}" {
                depth -= 1;
            }
        }
        if code == 330 && depth == 0 {
            return u64::from_str_radix(value.trim(), 16).ok();
        }
    }
    None
}
struct Text {
    handle: String,
    x: f64,
    y: f64,
}
fn closest(texts: &[Text], used: &HashSet<String>, x: f64, y: f64, kind: u8) -> Option<usize> {
    texts
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            let dx = (t.x - x).abs();
            let dy = (t.y - y).abs();
            !used.contains(&t.handle)
                && match kind {
                    0 => dy <= 1.25 && dx <= 4.5,
                    1 => dy <= 1.25 && t.x < x,
                    _ => dy <= 1.35 && dx <= 3.,
                }
        })
        .min_by(|(_, a), (_, b)| {
            (a.y - y)
                .abs()
                .partial_cmp(&(b.y - y).abs())
                .unwrap()
                .then_with(|| (a.x - x).abs().partial_cmp(&(b.x - x).abs()).unwrap())
                .then_with(|| a.handle.cmp(&b.handle))
        })
        .map(|(i, _)| i)
}

pub fn detect(document: &Document, cancel: &AtomicBool) -> Result<Vec<Slot>, String> {
    crate::input::check(cancel)?;
    let model = document
        .records()
        .find(|r| {
            r.kind() == "BLOCK_RECORD"
                && r.first(2)
                    .is_some_and(|v| v.eq_ignore_ascii_case("*Model_Space"))
        })
        .and_then(RecordView::handle);
    let mut small = Vec::new();
    let mut large = Vec::new();
    let mut rows = Vec::new();
    let mut labels = Vec::new();
    let mut addresses = Vec::new();
    let mut comments = Vec::new();
    for (i, r) in document.records().enumerate() {
        if i % 256 == 0 {
            crate::input::check(cancel)?;
        }
        if r.section() != Some("ENTITIES")
            || !matches!(r.kind(), "LWPOLYLINE" | "TEXT" | "MTEXT")
            || r.first(67).is_some_and(|v| v.trim() == "1")
            || r.first(410)
                .is_some_and(|v| !v.eq_ignore_ascii_case("Model"))
            || model.is_some_and(|m| owner(r).is_some_and(|o| o != m))
        {
            continue;
        }
        let layer = r.first(8).unwrap_or("0");
        if r.kind() == "LWPOLYLINE" {
            let Some(b) = polyline_bounds(r) else {
                continue;
            };
            let width = b.max_x - b.min_x;
            let height = b.max_y - b.min_y;
            if layer == "Hulplijn kader" {
                if (1.95..=2.2).contains(&width) && (1.45..=1.65).contains(&height) {
                    small.push(b);
                } else if (2.35..=2.7).contains(&width) && (1.7..=1.9).contains(&height) {
                    large.push(b);
                }
            } else if layer == "KADER"
                && (28.5..=29.5).contains(&width)
                && (2.5..=3.).contains(&height)
            {
                rows.push(b);
            }
        } else {
            let mut text = String::new();
            for (code, value) in r.tags().take_while(|(c, _)| *c < 1000 && *c != 101) {
                if code == 1 || (r.kind() == "MTEXT" && code == 3) {
                    text.push_str(value);
                }
            }
            let text = if r.kind() == "MTEXT" {
                text.replace("\\P", " ")
            } else {
                text
            };
            let text = trim(&text).to_lowercase();
            let target = if r.kind() == "TEXT" && layer == "KADER" && text.starts_with("proefsleuf")
            {
                &mut labels
            } else if r.kind() == "MTEXT" && layer == "KADER" && text.starts_with("adres") {
                &mut addresses
            } else if r.kind() == "MTEXT" && layer == "0" && text.starts_with("opmerkingen") {
                &mut comments
            } else {
                continue;
            };
            if let (Some(handle), Some(x), Some(y)) = (
                r.first(5),
                r.first(10).and_then(|v| v.trim().parse().ok()),
                r.first(20).and_then(|v| v.trim().parse().ok()),
            ) {
                target.push(Text {
                    handle: handle.into(),
                    x,
                    y,
                });
            }
        }
    }
    if small.is_empty() || large.is_empty() || rows.is_empty() {
        return Err("Kon in het DXF-sjabloon geen proefsleufvakken herkennen.".into());
    }
    let mut centers: Vec<_> = large.iter().map(|b| cx(*b)).collect();
    centers.sort_by(f64::total_cmp);
    let mut clusters: Vec<Vec<f64>> = Vec::new();
    for x in centers {
        if clusters.last().is_none_or(|c| x - c.last().unwrap() > 5.) {
            clusters.push(vec![x]);
        } else {
            clusters.last_mut().unwrap().push(x);
        }
    }
    let limit = if clusters.len() >= 2 {
        clusters[1].last().unwrap() + 2.
    } else {
        clusters[0].last().unwrap() + 1.
    };
    small.retain(|b| b.max_x <= limit);
    large.retain(|b| b.max_x <= limit);
    rows.retain(|b| b.max_x <= limit);
    labels.retain(|t| t.x <= limit);
    addresses.retain(|t| t.x <= limit);
    comments.retain(|t| t.x <= limit);
    small.sort_by(|a, b| {
        cy(*b)
            .partial_cmp(&cy(*a))
            .unwrap()
            .then_with(|| cx(*a).partial_cmp(&cx(*b)).unwrap())
    });
    let mut used_large = HashSet::new();
    let mut used_labels = HashSet::new();
    let mut used_addresses = HashSet::new();
    let mut used_comments = HashSet::new();
    let mut slots = Vec::new();
    for s in small {
        crate::input::check(cancel)?;
        let pair = large
            .iter()
            .enumerate()
            .filter(|(i, b)| {
                !used_large.contains(i)
                    && cx(**b) - cx(s) > 0.
                    && cx(**b) - cx(s) <= 4.
                    && (cy(**b) - cy(s)).abs() <= 0.35
            })
            .min_by(|(ia, a), (ib, b)| {
                (cy(**a) - cy(s))
                    .abs()
                    .partial_cmp(&(cy(**b) - cy(s)).abs())
                    .unwrap()
                    .then_with(|| (cx(**a) - cx(s)).partial_cmp(&(cx(**b) - cx(s))).unwrap())
                    .then_with(|| ia.cmp(ib))
            });
        let Some((large_index, &l)) = pair else {
            continue;
        };
        let row = rows
            .iter()
            .filter(|r| r.min_x <= s.min_x && r.max_x >= l.max_x && (cy(**r) - cy(s)).abs() <= 0.45)
            .min_by(|a, b| {
                (cy(**a) - cy(s))
                    .abs()
                    .partial_cmp(&(cy(**b) - cy(s)).abs())
                    .unwrap()
                    .then_with(|| {
                        (a.min_x - s.min_x)
                            .abs()
                            .partial_cmp(&(b.min_x - s.min_x).abs())
                            .unwrap()
                    })
            });
        let Some(&row_box) = row else {
            continue;
        };
        let x = (cx(s) + cx(l)) * 0.5;
        let y = (cy(s) + cy(l)) * 0.5;
        let Some(label) = closest(&labels, &used_labels, x, y, 0) else {
            continue;
        };
        let label_handle = labels[label].handle.clone();
        used_labels.insert(label_handle.clone());
        used_large.insert(large_index);
        let address_handle =
            closest(&addresses, &used_addresses, x, y, 1).map(|i| addresses[i].handle.clone());
        let comments_handle =
            closest(&comments, &used_comments, x, y, 2).map(|i| comments[i].handle.clone());
        if let Some(h) = &address_handle {
            used_addresses.insert(h.clone());
        }
        if let Some(h) = &comments_handle {
            used_comments.insert(h.clone());
        }
        slots.push(Slot {
            label_handle,
            address_handle,
            comments_handle,
            row_box,
            tiff_box: s,
            map_box: l,
            sort_x: cx(s),
            sort_y: cy(s),
        });
    }
    if slots.is_empty() {
        return Err("Kon geen geldige proefsleufvakken uit het DXF-sjabloon afleiden.".into());
    }
    slots.sort_by(|a, b| b.sort_y.partial_cmp(&a.sort_y).unwrap());
    let mut blocks: Vec<Vec<Slot>> = Vec::new();
    let mut previous_y = None;
    for slot in slots {
        if previous_y.is_none_or(|y| y - slot.sort_y > 6.) {
            blocks.push(Vec::new());
        }
        previous_y = Some(slot.sort_y);
        blocks.last_mut().unwrap().push(slot);
    }
    let mut ordered = Vec::new();
    for mut block in blocks {
        block.sort_by(|a, b| a.sort_x.partial_cmp(&b.sort_x).unwrap());
        let mut columns: Vec<Vec<Slot>> = Vec::new();
        for slot in block {
            if columns
                .last()
                .is_none_or(|col| slot.sort_x - col.last().unwrap().sort_x > 5.)
            {
                columns.push(Vec::new());
            }
            columns.last_mut().unwrap().push(slot);
        }
        for mut column in columns {
            column.sort_by(|a, b| b.sort_y.partial_cmp(&a.sort_y).unwrap());
            ordered.extend(column);
        }
    }
    crate::input::check(cancel)?;
    Ok(ordered)
}
