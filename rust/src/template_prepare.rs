//! Active template text styles and profile layer updates. Source and pending
//! records share the same symbol rules and commit as one atomic preparation.
use crate::{
    dxf_document::{AddedSymbol, Change, Document, InsertedValues, RecordView},
    dxf_symbols::key,
};
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Write,
    sync::atomic::AtomicBool,
};
const STYLES: [&str; 2] = ["PROEFSLEUVEN_LIBMONO", "KADASTRALE_LABELS"];
#[derive(serde::Deserialize)]
pub struct LayerColor {
    pub name: String,
    pub color: u16,
}
fn table<'a>(doc: &'a Document, name: &str) -> Result<(RecordView<'a>, usize), String> {
    let mut found = None;
    for r in doc.records() {
        if r.kind() == "TABLE" && r.first(2) == Some(name) {
            found = Some(r);
        } else if found.is_some() && r.kind() == "ENDTAB" {
            return Ok((found.unwrap(), doc.record_start(r.index())));
        }
    }
    Err(format!("DXF-tabel {name} ontbreekt of is niet afgesloten"))
}
fn dictionary_ref(record: RecordView<'_>, name: &str) -> Option<u64> {
    let mut active = false;
    for (c, v) in record.tags() {
        if c == 3 {
            active = v == name;
        } else if active && matches!(c, 350 | 360) {
            return u64::from_str_radix(v.trim(), 16).ok();
        }
    }
    None
}
fn layer_defaults(doc: &Document) -> Result<(u64, u64), String> {
    let material = doc
        .records()
        .find(|r| {
            r.kind() == "MATERIAL" && r.first(1).is_some_and(|v| v.eq_ignore_ascii_case("Global"))
        })
        .and_then(RecordView::handle)
        .ok_or("Sjabloon mist Global-materiaal")?;
    let plot = doc
        .records()
        .find_map(|r| {
            if matches!(r.kind(), "DICTIONARY" | "ACDBDICTIONARYWDFLT") {
                dictionary_ref(r, "ACAD_PLOTSTYLENAME")
            } else {
                None
            }
        })
        .ok_or("Sjabloon mist plotstijldictionary")?;
    let plot = doc
        .by_handle(&format!("{plot:X}"))?
        .and_then(|r| dictionary_ref(r, "Normal"))
        .ok_or("Sjabloon mist Normal-plotstijl")?;
    Ok((material, plot))
}
/// Mirrors setup_text_styles + ensure_template_profile_layers: keep existing
/// font/style properties; enforce only ACI and lineweight on requested layers.
/// Repeated names retain their first spelling and last requested color.
pub fn profile<'a>(
    doc: &mut Document,
    points: impl IntoIterator<Item = (&'a str, u16)>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    crate::input::check(cancel)?;
    let mut wanted = Vec::<(String, u16)>::new();
    let mut positions: HashMap<String, usize> = HashMap::new();
    for (name, color) in [
        ("B-WE-OG-TERREIN_PROFIELLIJN-GD", 8),
        ("X-XX-AL-PROFIELBALK-SD", 7),
        ("X-XX-AL-VERWIJZING-SD", 7),
    ]
    .into_iter()
    .chain(points.into_iter().filter(|(name, _)| *name != "0"))
    {
        crate::input::check(cancel)?;
        crate::object_dxf::layer_name(name)?;
        if !(1..=255).contains(&color) {
            return Err("Profiel-laagkleur moet tussen 1 en 255 liggen".into());
        }
        let k = key(name);
        if let Some(&i) = positions.get(&k) {
            wanted[i].1 = color;
        } else {
            if wanted.len() >= 10_000 {
                return Err("Te veel profiel-laagnamen".into());
            }
            positions.insert(k, wanted.len());
            wanted.push((name.to_owned(), color));
        }
    }
    let mut existing = HashMap::new();
    for (i, r) in doc.records().enumerate() {
        if i % 1024 == 0 {
            crate::input::check(cancel)?;
        }
        if r.kind() == "LAYER" {
            let name = r.first(2).ok_or("Laagnaam ontbreekt")?;
            if existing.insert(key(name), r).is_some() {
                return Err("Dubbele DXF-laagnaam".into());
            }
        }
    }
    let missing_styles = STYLES
        .into_iter()
        .filter(|s| !doc.has_style(s))
        .collect::<Vec<_>>();
    let new_layer_count = wanted
        .iter()
        .filter(|(name, _)| {
            !existing.contains_key(&key(name)) && !doc.added_layers.contains_key(&key(name))
        })
        .count();
    let allocation = if !missing_styles.is_empty() || new_layer_count > 0 {
        Some(doc.seed()?)
    } else {
        None
    };
    let (mut next, mut seed) = allocation.map_or((0, None), |(n, s)| (n, Some(s)));
    let mut allocate = || {
        let id = next;
        next = next.checked_add(1).ok_or("Geen vrije DXF-handles")?;
        Ok::<_, String>(id)
    };
    let mut changes = Vec::new();
    let mut layers_added = Vec::new();
    let mut styles_added = Vec::new();
    if !missing_styles.is_empty() {
        let (t, at) = table(doc, "STYLE")?;
        let owner = t.handle().ok_or("Stijltabel mist handle")?;
        let mut text = String::new();
        for name in &missing_styles {
            let handle = allocate()?;
            write!(text,"0\nSTYLE\n5\n{handle:X}\n330\n{owner:X}\n100\nAcDbSymbolTableRecord\n100\nAcDbTextStyleTableRecord\n2\n{name}\n70\n0\n40\n0.0\n41\n1.0\n50\n0.0\n71\n0\n42\n2.5\n3\nLiberationMono-Regular.ttf\n4\n\n").unwrap();
            styles_added.push((key(name), AddedSymbol { handle, at }));
        }
        let count = doc.records().filter(|r| r.kind() == "STYLE").count()
            + doc.added_styles.len()
            + missing_styles.len();
        changes.push(doc.tag_change(t.index(), 70, &count.to_string())?);
        changes.push(Change {
            start: at,
            end: at,
            text,
        });
    }
    let layer_table = if new_layer_count > 0 {
        let (t, at) = table(doc, "LAYER")?;
        Some((t, at, layer_defaults(doc)?))
    } else {
        None
    };
    let mut layer_text = String::new();
    let mut updates: BTreeMap<usize, InsertedValues> = BTreeMap::new();
    for (name, color) in wanted {
        crate::input::check(cancel)?;
        let k = key(&name);
        if let Some(r) = existing.get(&k) {
            for (code, wanted) in [(62, color.to_string()), (370, "0".into())] {
                let current = r.current_value(code)?;
                if current
                    .is_some_and(|s| s.trim().parse::<i32>().ok() == wanted.parse::<i32>().ok())
                {
                    continue;
                }
                if r.first(code).is_some() {
                    changes.push(doc.tag_change(r.index(), code, &wanted)?);
                } else {
                    updates
                        .entry(doc.before_xdata(r.index()))
                        .or_default()
                        .entry(None)
                        .or_default()
                        .push((code, wanted));
                }
            }
        } else if let Some(symbol) = doc.added_layers.get(&k) {
            updates.entry(symbol.at).or_default().insert(
                Some(symbol.handle),
                vec![(62, color.to_string()), (370, "0".into())],
            );
        } else {
            let (t, at, (material, plot)) =
                layer_table.ok_or("Nieuwe laag mist tabelvoorbereiding")?;
            let owner = t.handle().ok_or("Laagtabel mist handle")?;
            let handle = allocate()?;
            write!(layer_text,"0\nLAYER\n5\n{handle:X}\n330\n{owner:X}\n100\nAcDbSymbolTableRecord\n100\nAcDbLayerTableRecord\n2\n{name}\n70\n0\n62\n{color}\n6\nContinuous\n370\n0\n290\n1\n390\n{plot:X}\n347\n{material:X}\n").unwrap();
            layers_added.push((k, AddedSymbol { handle, at }));
        }
    }
    if let Some((t, at, _)) = layer_table {
        changes.push(doc.tag_change(
            t.index(),
            70,
            &(doc.layer_count() + new_layer_count).to_string(),
        )?);
        changes.push(Change {
            start: at,
            end: at,
            text: layer_text,
        });
    }
    let mut replacements = BTreeMap::new();
    let mut prefixes = Vec::new();
    for (at, values) in updates {
        let text = doc.patch_inserted_values(at, &values, cancel)?;
        if doc.has_insertion(at) {
            replacements.insert(at, text);
        } else {
            prefixes.push(Change {
                start: at,
                end: at,
                text,
            });
        }
    }
    if let Some(seed) = &mut seed {
        seed.text = format!("{next:X}");
    }
    if let Some(seed) = seed {
        changes.push(seed);
    }
    crate::input::check(cancel)?;
    drop(existing);
    prefixes.extend(changes);
    doc.apply_with_insertions(prefixes, replacements)?;
    doc.added_layers.extend(layers_added);
    doc.added_styles.extend(styles_added);
    Ok(())
}
