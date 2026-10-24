//! Extend model sheets and their paper layouts as one transaction. Reindexing
//! makes newly copied slots/layouts visible to later title, viewport and raster
//! stages. Failure/cancellation never changes the caller's document.
use crate::{
    dxf_document::{Change, Document, RecordView},
    input,
    python_values::uppercase,
    template_copy::{self as copy, Copies, Tags},
    template_slots::{self, Slot},
    Bounds,
};
use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
    sync::atomic::AtomicBool,
};
#[derive(Debug, serde::Serialize)]
pub struct Extension {
    pub added_pages: usize,
    pub slots: Vec<Slot>,
}
fn number(t: &Tags, code: i32, default: f64) -> Result<f64, String> {
    let n = copy::first(t, code).map_or(Ok(default), |v| {
        v.trim()
            .parse::<f64>()
            .map_err(|_| "Ongeldige bladcoördinaat".to_string())
    })?;
    if !n.is_finite() {
        return Err("Niet-eindige bladcoördinaat".into());
    }
    Ok(n)
}
fn point(t: &Tags, code: i32) -> Result<[f64; 3], String> {
    Ok([
        number(t, code, 0.)?,
        number(t, code + 10, 0.)?,
        number(t, code + 20, 0.)?,
    ])
}
fn finite(p: [f64; 2]) -> Result<[f64; 2], String> {
    if p.iter().any(|n| !n.is_finite() || n.abs() > 1e12) {
        Err("Ongeldige bladverschuiving".into())
    } else {
        Ok(p)
    }
}
fn entity_anchor(r: RecordView<'_>) -> Result<Option<[f64; 2]>, String> {
    let t = copy::tags(r);
    Ok(match r.kind() {
        "TEXT" | "MTEXT" => Some([number(&t, 10, 0.)?, number(&t, 20, 0.)?]),
        "LINE" => {
            let a = point(&t, 10)?;
            let b = point(&t, 11)?;
            Some([(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5])
        }
        "LWPOLYLINE" => {
            let mut x = None;
            let mut bounds: Option<Bounds> = None;
            let mut active = false;
            for (c, v) in t.iter().take_while(|(c, _)| *c < 1000) {
                if *c == 100 {
                    active = v == "AcDbPolyline";
                    continue;
                }
                if *c == 101 {
                    break;
                }
                if !active {
                    continue;
                }
                if *c == 10 {
                    x = Some(v.trim().parse::<f64>().map_err(|_| "Ongeldig bladvertex")?);
                }
                if *c == 20 {
                    if let Some(x) = x.take() {
                        let y = v.trim().parse::<f64>().map_err(|_| "Ongeldig bladvertex")?;
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
            bounds.map(|b| [(b.min_x + b.max_x) * 0.5, (b.min_y + b.max_y) * 0.5])
        }
        _ => None,
    })
}
fn translate(t: &mut Tags, kind: &str, delta: [f64; 2]) -> Result<(), String> {
    if kind == "MTEXT" {
        let header_end = t
            .iter()
            .position(|(c, _)| *c == 101 || *c >= 1000)
            .unwrap_or(t.len());
        if let Some((_, rotation)) = t[..header_end].iter().find(|(c, _)| *c == 50) {
            let angle = rotation
                .trim()
                .parse::<f64>()
                .map_err(|_| "Ongeldige MTEXT-rotatie")?
                .to_radians();
            let has_direction = t[..header_end].iter().any(|(c, _)| *c == 11)
                || t[header_end..]
                    .iter()
                    .take_while(|(c, _)| *c < 1000)
                    .any(|(c, _)| *c == 10);
            let (s, c) = angle.sin_cos();
            let normal = [
                number(t, 210, 0.)?,
                number(t, 220, 0.)?,
                number(t, 230, 1.)?,
            ];
            let direction = crate::cad_geometry::Affine::ocs(normal)?.vector([c, s, 0.]);
            t.remove(t.iter().position(|(code, _)| *code == 50).unwrap());
            if !has_direction {
                for (code, value) in [11, 21, 31].into_iter().zip(direction) {
                    copy::scope_set(t, "AcDbMText", code, value.to_string())?;
                }
            }
        }
    }
    let mut shift = [delta[0], delta[1], 0.];
    if matches!(kind, "TEXT" | "LWPOLYLINE") {
        let normal = [
            number(t, 210, 0.)?,
            number(t, 220, 0.)?,
            number(t, 230, 1.)?,
        ];
        let ocs = crate::cad_geometry::Affine::ocs(normal)?;
        shift = std::array::from_fn(|i| (0..3).map(|j| ocs.0[j][i] * shift[j]).sum());
    }
    let mut xdata = false;
    let mut appdata_depth = 0;
    let mut embedded = false;
    for (c, v) in t.iter_mut() {
        if *c == 101 {
            embedded = true;
            continue;
        }
        if *c == 102 {
            if v.starts_with('{') {
                appdata_depth += 1;
            } else if v == "}" {
                appdata_depth -= 1;
            }
            continue;
        }
        if appdata_depth > 0 {
            continue;
        }
        if *c >= 1000 {
            xdata = true;
        }
        let amount = if xdata {
            match *c {
                1011 => Some(delta[0]),
                1021 => Some(delta[1]),
                _ => None,
            }
        } else {
            match kind {
                "LINE" => match *c {
                    10 | 11 => Some(shift[0]),
                    20 | 21 => Some(shift[1]),
                    30 | 31 => Some(shift[2]),
                    _ => None,
                },
                "TEXT" => match *c {
                    10 | 11 => Some(shift[0]),
                    20 | 21 => Some(shift[1]),
                    30 | 31 => Some(shift[2]),
                    _ => None,
                },
                "MTEXT" => {
                    if embedded {
                        match *c {
                            11 => Some(delta[0]),
                            21 => Some(delta[1]),
                            _ => None,
                        }
                    } else {
                        match *c {
                            10 => Some(delta[0]),
                            20 => Some(delta[1]),
                            _ => None,
                        }
                    }
                }
                "LWPOLYLINE" => match *c {
                    10 => Some(shift[0]),
                    20 => Some(shift[1]),
                    38 => Some(shift[2]),
                    _ => None,
                },
                _ => None,
            }
        };
        if let Some(amount) = amount {
            let old = v
                .trim()
                .parse::<f64>()
                .map_err(|_| "Ongeldige te verschuiven bladcoördinaat")?;
            let value = old + amount;
            if !value.is_finite() {
                return Err("Bladcoördinaat buiten bereik".into());
            }
            *v = value.to_string();
        }
    }
    if matches!(kind, "MTEXT" | "LWPOLYLINE")
        && [210, 220, 230].iter().any(|c| copy::first(t, *c).is_some())
    {
        let normal = [
            number(t, 210, 0.)?,
            number(t, 220, 0.)?,
            number(t, 230, 1.)?,
        ];
        let ocs = crate::cad_geometry::Affine::ocs(normal)?;
        let subclass = if kind == "MTEXT" {
            "AcDbMText"
        } else {
            "AcDbPolyline"
        };
        for (i, c) in [210, 220, 230].into_iter().enumerate() {
            copy::scope_set(t, subclass, c, ocs.0[i][2].to_string())?;
        }
    }
    if kind == "LWPOLYLINE" && shift[2] != 0. && copy::first(t, 38).is_none() {
        copy::scope_set(t, "AcDbPolyline", 38, shift[2].to_string())?;
    }
    Ok(())
}
fn content(slots: &[Slot]) -> Result<Bounds, String> {
    let first = slots
        .first()
        .ok_or("Geen bestaand proefsleufblad om uit te breiden")?;
    let mut b = first.row_box;
    for s in &slots[1..] {
        b.min_x = b.min_x.min(s.row_box.min_x);
        b.min_y = b.min_y.min(s.row_box.min_y);
        b.max_x = b.max_x.max(s.row_box.max_x);
        b.max_y = b.max_y.max(s.row_box.max_y);
    }
    b.min_x -= 2.;
    b.min_y -= 2.;
    b.max_x += 2.;
    b.max_y += 2.;
    finite([b.min_x, b.min_y])?;
    finite([b.max_x, b.max_y])?;
    Ok(b)
}
fn anchor(slots: &[Slot]) -> [f64; 2] {
    [
        slots
            .iter()
            .map(|s| s.row_box.min_x)
            .fold(f64::INFINITY, f64::min),
        slots
            .iter()
            .map(|s| s.row_box.min_y)
            .fold(f64::INFINITY, f64::min),
    ]
}
fn layout_name(r: RecordView<'_>) -> Option<&str> {
    r.tags().filter(|(c, _)| *c == 1).last().map(|(_, v)| v)
}
fn model_owner(doc: &Document) -> Result<u64, String> {
    doc.records()
        .find(|r| {
            r.kind() == "BLOCK_RECORD"
                && r.first(2)
                    .is_some_and(|n| crate::dxf_symbols::key(n) == "*model_space")
        })
        .and_then(|r| r.handle())
        .ok_or("Modelspace-blokrecord ontbreekt".into())
}
fn table(doc: &Document) -> Result<(RecordView<'_>, usize), String> {
    let mut found = None;
    for r in doc.records() {
        if r.kind() == "TABLE" && r.first(2) == Some("BLOCK_RECORD") {
            found = Some(r);
        } else if r.kind() == "ENDTAB" {
            if let Some(t) = found {
                return Ok((t, doc.record_start(r.index())));
            }
        }
    }
    Err("Bloktabel ontbreekt".into())
}
fn ensure_layout<'a>(
    doc: &'a Document,
    page: usize,
    delta: [f64; 2],
    copies: &mut Copies<'a>,
) -> Result<Vec<Change>, String> {
    let name = format!("Blad{page}");
    let key = uppercase(&name);
    let layouts = doc
        .records()
        .filter(|r| r.kind() == "LAYOUT")
        .collect::<Vec<_>>();
    if layouts
        .iter()
        .any(|&r| layout_name(r).is_some_and(|n| uppercase(n) == key))
    {
        return Ok(Vec::new());
    }
    let source = (1..page)
        .rev()
        .find_map(|i| {
            layouts.iter().copied().find(|&r| {
                layout_name(r).is_some_and(|n| uppercase(n) == uppercase(&format!("Blad{i}")))
            })
        })
        .ok_or("Geen bronlayout voor nieuw blad")?;
    let source_handle = source.handle().ok_or("Bronlayout mist handle")?;
    let source_space = doc
        .records()
        .find(|r| {
            r.kind() == "BLOCK_RECORD"
                && r.first(340).and_then(|v| copy::id(v).ok()) == Some(source_handle)
        })
        .and_then(|r| r.handle())
        .ok_or("Bronlayout mist blokrecord")?;
    let dict_id = copy::owner(source).ok_or("Layoutdictionary ontbreekt")?;
    let dictionary = doc
        .by_handle(&format!("{dict_id:X}"))?
        .filter(|r| r.kind() == "DICTIONARY")
        .ok_or("Layoutdictionary ontbreekt")?;
    let (t, table_end) = table(doc)?;
    let table_handle = t.handle().ok_or("Bloktabelhandle ontbreekt")?;
    let block_names = doc
        .records()
        .filter(|r| r.kind() == "BLOCK")
        .filter_map(|r| r.first(2))
        .map(crate::dxf_symbols::key)
        .collect::<HashSet<_>>();
    let block_name = (0..100_000)
        .map(|i| format!("*Paper_Space{i}"))
        .find(|n| !block_names.contains(&crate::dxf_symbols::key(n)))
        .ok_or("Geen vrije paperspace-naam")?;
    let block_record = copies.allocate()?;
    let block = copies.allocate()?;
    let endblock = copies.allocate()?;
    let layout = copies.allocate()?;
    let mut layout_tags = copy::tags(source);
    // A new LAYOUT copies namespace attributes, not source extension data.
    let mut depth = 0;
    layout_tags.retain(|(c, v)| {
        if *c >= 1000 {
            return false;
        }
        if *c == 102 {
            if v.starts_with('{') {
                depth += 1;
            } else if v == "}" {
                depth -= 1;
            }
            return false;
        }
        depth == 0
    });
    copy::replace(&mut layout_tags, 5, format!("{layout:X}"));
    // Owner before first subclass is the dictionary; code 330 in AcDbLayout
    // is the newly created block record.
    if let Some((_, v)) = layout_tags
        .iter_mut()
        .take_while(|(c, _)| *c != 100)
        .find(|(c, _)| *c == 330)
    {
        *v = format!("{dict_id:X}");
    }
    copy::scope_set(&mut layout_tags, "AcDbLayout", 1, name.clone())?;
    copy::scope_set(
        &mut layout_tags,
        "AcDbLayout",
        71,
        (layouts.len() + 1).to_string(),
    )?;
    copy::scope_set(
        &mut layout_tags,
        "AcDbLayout",
        330,
        format!("{block_record:X}"),
    )?;
    layout_tags.retain(|(c, _)| *c != 331);
    let width = number(&layout_tags, 44, 420.)?;
    let height = number(&layout_tags, 45, 297.)?;
    for (c, v) in [(10, 0.), (20, 0.), (11, width), (21, height)] {
        copy::scope_set(&mut layout_tags, "AcDbLayout", c, v.to_string())?;
    }
    let records = doc.records().collect::<Vec<_>>();
    let mut entities = Vec::<Tags>::new();
    let mut map = HashMap::new();
    let mut i = 0;
    while i < records.len() {
        let r = records[i];
        i += 1;
        input::check(copies_cancel(copies))?;
        if !matches!(r.section(), Some("BLOCKS" | "ENTITIES"))
            || matches!(r.kind(), "BLOCK" | "ENDBLK")
            || copy::owner(r) != Some(source_space)
        {
            continue;
        }
        let (new, mut copied) = copies.entity(r, block_record)?;
        if let Some(old) = r.handle() {
            map.insert(old, new);
        }
        copy::scope_set(&mut copied, "AcDbEntity", 67, "1".into())?;
        copied.retain(|(c, _)| *c != 410);
        entities.push(copied);
        if matches!(r.kind(), "INSERT" | "POLYLINE") {
            while i < records.len() && matches!(records[i].kind(), "ATTRIB" | "VERTEX" | "SEQEND") {
                let child = records[i];
                i += 1;
                let (new, mut copied) = copies.entity(child, new)?;
                if let Some(old) = child.handle() {
                    map.insert(old, new);
                }
                copy::scope_set(&mut copied, "AcDbEntity", 67, "1".into())?;
                copied.retain(|(c, _)| *c != 410);
                entities.push(copied);
                if child.kind() == "SEQEND" {
                    break;
                }
            }
        }
    }
    let mut main = None;
    for (i, e) in entities.iter().enumerate() {
        if copy::first(e, 0) == Some("VIEWPORT") && copy::first(e, 8) == Some("KADER") {
            let key = (number(e, 20, 0.)?, number(e, 41, 0.)?);
            if main.is_none_or(|(_, old)| key > old) {
                main = Some((i, key));
            }
        }
    }
    for e in &mut entities {
        if copy::first(e, 0) == Some("VIEWPORT") {
            if let Some((_, v)) = e.iter_mut().find(|(c, _)| *c == 340) {
                if let Some(&new) = map.get(&copy::id(v)?) {
                    *v = format!("{new:X}");
                }
            }
        }
    }
    if let Some((i, _)) = main {
        if delta[0].abs() > 1e-9 || delta[1].abs() > 1e-9 {
            for (c, d) in [(12, delta[0]), (22, delta[1])] {
                let value = number(&entities[i], c, 0.)? + d;
                if !value.is_finite() {
                    return Err("Viewportverschuiving buiten bereik".into());
                }
                copy::scope_set(&mut entities[i], "AcDbViewport", c, value.to_string())?;
            }
        }
    }
    let mut blocks=format!("0\nBLOCK\n5\n{block:X}\n330\n{block_record:X}\n100\nAcDbEntity\n8\n0\n100\nAcDbBlockBegin\n2\n{block_name}\n70\n0\n10\n0\n20\n0\n30\n0\n3\n{block_name}\n1\n\n");
    for e in &entities {
        blocks.push_str(&copy::encode(e));
    }
    write!(blocks,"0\nENDBLK\n5\n{endblock:X}\n330\n{block_record:X}\n100\nAcDbEntity\n8\n0\n100\nAcDbBlockEnd\n").unwrap();
    let record=format!("0\nBLOCK_RECORD\n5\n{block_record:X}\n330\n{table_handle:X}\n100\nAcDbSymbolTableRecord\n100\nAcDbBlockTableRecord\n2\n{block_name}\n340\n{layout:X}\n70\n0\n280\n1\n281\n0\n");
    let mut dictionary_tags = copy::tags(dictionary);
    dictionary_tags.push((3, name));
    dictionary_tags.push((350, format!("{layout:X}")));
    let block_at = doc.section_end("BLOCKS")?;
    let object_at = doc.section_end("OBJECTS")?;
    Ok(vec![
        Change {
            start: table_end,
            end: table_end,
            text: record,
        },
        doc.tag_change(
            t.index(),
            70,
            &(doc.records().filter(|r| r.kind() == "BLOCK_RECORD").count() + 1).to_string(),
        )?,
        Change {
            start: block_at,
            end: block_at,
            text: blocks,
        },
        Change {
            start: object_at,
            end: object_at,
            text: copy::encode(&layout_tags),
        },
        doc.record_change(dictionary.index(), copy::encode(&dictionary_tags)),
    ])
}
fn copies_cancel<'a>(copies: &Copies<'a>) -> &'a AtomicBool {
    copies.cancel()
}

pub fn extend(
    doc: &mut Document,
    required_slots: usize,
    cancel: &AtomicBool,
) -> Result<Extension, String> {
    extend_with_progress(doc, required_slots, cancel, |_, _| {})
}
/// Progress uses filled capacity clamped to the request. Callbacks run before
/// committing; cancellation or a worker panic leaves the original untouched.
pub fn extend_with_progress(
    doc: &mut Document,
    required_slots: usize,
    cancel: &AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> Result<Extension, String> {
    input::check(cancel)?;
    if required_slots > 100_000 {
        return Err("Te veel gevraagde proefsleufvakken".into());
    }
    let initial = template_slots::detect(doc, cancel)?;
    progress(initial.len().min(required_slots), required_slots);
    input::check(cancel)?;
    if initial.len() >= required_slots {
        return Ok(Extension {
            added_pages: 0,
            slots: initial,
        });
    }
    let mut working = doc.indexed_copy(cancel)?;
    let mut added = 0;
    loop {
        let slots = template_slots::detect(&working, cancel)?;
        let current_pages = slots.len().div_ceil(8).max(1);
        if current_pages >= required_slots.div_ceil(8).max(1) {
            if slots.len() < required_slots {
                return Err("Sjabloonuitbreiding levert onvoldoende proefsleufvakken".into());
            }
            input::check(cancel)?;
            *doc = working;
            return Ok(Extension {
                added_pages: added,
                slots,
            });
        }
        if added >= 12_500 {
            return Err("Te veel toegevoegde sjabloonbladen".into());
        }
        let pages = slots.chunks(8).collect::<Vec<_>>();
        let last = pages.last().unwrap();
        let bounds = content(last)?;
        let delta = if pages.len() >= 2 {
            let a = anchor(last);
            let b = anchor(pages[pages.len() - 2]);
            finite([a[0] - b[0], a[1] - b[1]])?
        } else {
            finite([0., -((bounds.max_y - bounds.min_y) * 2.).max(25.)])?
        };
        if delta[0].abs() <= 1e-9 && delta[1].abs() <= 1e-9 {
            return Err("Bladuitbreiding maakt geen voortgang".into());
        }
        let owner = model_owner(&working)?;
        let (mut next, mut seed) = working.seed()?;
        let mut copies = Copies::new(&working, next, cancel);
        let mut entities = String::new();
        let mut count = 0;
        for r in working.records() {
            if r.section() != Some("ENTITIES") || copy::owner(r) != Some(owner) {
                continue;
            }
            if let Some(a) = entity_anchor(r)? {
                if a[0] >= bounds.min_x
                    && a[0] <= bounds.max_x
                    && a[1] >= bounds.min_y
                    && a[1] <= bounds.max_y
                {
                    let (_, mut t) = copies.entity(r, owner)?;
                    translate(&mut t, r.kind(), delta)?;
                    entities.push_str(&copy::encode(&t));
                    count += 1;
                }
            }
        }
        if count == 0 {
            return Err("Geen modelspace-onderdelen van het laatste proefsleufblad".into());
        }
        let mut changes = ensure_layout(&working, current_pages + 1, delta, &mut copies)?;
        changes.extend(copies.changes()?);
        next = copies.next;
        drop(copies);
        seed.text = format!("{next:X}");
        changes.push(seed);
        let at = working.section_end("ENTITIES")?;
        changes.push(Change {
            start: at,
            end: at,
            text: entities,
        });
        working.apply(changes)?;
        let candidate = working.indexed_copy(cancel)?;
        let after = template_slots::detect(&candidate, cancel)?;
        if after.len() <= slots.len() {
            return Err("Bladuitbreiding levert geen extra proefsleufvakken".into());
        }
        progress(after.len().min(required_slots), required_slots);
        input::check(cancel)?;
        working = candidate;
        added += 1;
    }
}
