//! Restore the source legend suffix on requested layouts without any INSERT.
use crate::{
    dxf_document::{Change, Document, RecordView},
    input,
    python_values::uppercase,
    template_attributes,
    template_copy::{self as copy, Copies},
};
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::AtomicBool,
};
fn name(r: RecordView<'_>) -> Option<&str> {
    r.tags().filter(|(c, _)| *c == 1).last().map(|(_, v)| v)
}
fn roots(doc: &Document, space: u64) -> Vec<RecordView<'_>> {
    doc.records()
        .filter(|r| {
            matches!(r.section(), Some("BLOCKS" | "ENTITIES"))
                && !matches!(
                    r.kind(),
                    "BLOCK" | "ENDBLK" | "ATTRIB" | "VERTEX" | "SEQEND"
                )
                && copy::owner(*r) == Some(space)
        })
        .collect()
}
fn space(doc: &Document, layout: RecordView<'_>) -> Option<u64> {
    let id = layout.handle()?;
    doc.records()
        .find(|r| {
            r.kind() == "BLOCK_RECORD" && r.first(340).and_then(|v| copy::id(v).ok()) == Some(id)
        })
        .and_then(RecordView::handle)
}
/// Work on an indexed copy so pending symbols/text and newly created pages are
/// visible. Missing layouts/source legend are skipped as in the original.
pub fn restore(
    doc: &mut Document,
    layouts: &[String],
    cancel: &AtomicBool,
) -> Result<usize, String> {
    input::check(cancel)?;
    if layouts.len() > 12_500 {
        return Err("Te veel legendabladen".into());
    }
    if layouts.len() < 2 {
        return Ok(0);
    }
    let mut working = doc.indexed_copy(cancel)?;
    let spaces = crate::template_geometry::insertion_spaces(&working, cancel)?;
    let layout_map = working
        .records()
        .filter(|r| r.kind() == "LAYOUT")
        .filter_map(|r| Some((uppercase(name(r)?), r)))
        .collect::<HashMap<_, _>>();
    let Some(&source) = layout_map.get("BLAD1") else {
        return Ok(0);
    };
    let source_space = space(&working, source).ok_or("Bronlegenda mist papierruimte")?;
    let source_entities = roots(&working, source_space);
    let Some(start) = source_entities
        .iter()
        .position(|r| r.kind() == "INSERT" && r.first(8) == Some("X-XX-AL-LEGENDA"))
    else {
        return Ok(0);
    };
    let source_entities = &source_entities[start..];
    let all = working.records().collect::<Vec<_>>();
    let (mut next, mut seed) = working.seed()?;
    let mut copies = Copies::new(&working, next, cancel);
    let mut changes = Vec::new();
    let mut added = 0;
    let mut planned = HashSet::new();
    let mut blueprints = HashMap::new();
    for name in &layouts[1..] {
        input::check(cancel)?;
        let Some(&layout) = layout_map.get(&uppercase(name)) else {
            continue;
        };
        let owner = space(&working, layout).ok_or("Doellegenda mist papierruimte")?;
        if roots(&working, owner).iter().any(|r| r.kind() == "INSERT") {
            continue;
        }
        if !planned.insert(owner) {
            continue;
        }
        let at = spaces
            .get(&uppercase(name))
            .ok_or("Papierruimte is niet afgesloten")?
            .at;
        let mut output = String::new();
        for &r in source_entities {
            input::check(cancel)?;
            let mut children = Vec::new();
            if matches!(r.kind(), "INSERT" | "POLYLINE") {
                for &child in &all[r.index() + 1..] {
                    if !matches!(child.kind(), "ATTRIB" | "VERTEX" | "SEQEND") {
                        break;
                    }
                    children.push(child);
                    if child.kind() == "SEQEND" {
                        break;
                    }
                }
            }
            if r.kind() == "INSERT" && r.first(8) == Some("X-XX-AL-LEGENDA") {
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    blueprints.entry(r.index())
                {
                    entry.insert(template_attributes::prepare_legend(
                        &working, r, &children, cancel,
                    )?);
                }
                for t in blueprints[&r.index()].instantiate(owner, &mut copies)? {
                    output.push_str(&copy::encode(&t));
                }
            } else {
                let (id, mut t) = copies.entity(r, owner)?;
                copy::scope_set(&mut t, "AcDbEntity", 67, "1".into())?;
                t.retain(|(c, _)| *c != 410);
                output.push_str(&copy::encode(&t));
                for child in children {
                    let (_, mut t) = copies.entity(child, id)?;
                    copy::scope_set(&mut t, "AcDbEntity", 67, "1".into())?;
                    t.retain(|(c, _)| *c != 410);
                    output.push_str(&copy::encode(&t));
                }
            }
        }
        changes.push(Change {
            start: at,
            end: at,
            text: output,
        });
        added += 1;
    }
    if added == 0 {
        return Ok(0);
    }
    changes.extend(copies.changes()?);
    next = copies.next;
    drop(copies);
    seed.text = format!("{next:X}");
    changes.push(seed);
    drop(layout_map);
    working.apply(changes)?;
    let result = working.indexed_copy(cancel)?;
    input::check(cancel)?;
    *doc = result;
    Ok(added)
}
