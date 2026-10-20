//! Same-document normal/reverse containers and native AutoCAD visibility graph.
//! Child handles/layers/resources remain intact. One batch and one fresh index
//! suffice; no second template, xref import or intermediate DXF is necessary.
use crate::{
    dxf_document::{AddedBlock, AddedSymbol, Change, Document, RecordView},
    template_copy as copy,
};
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::AtomicBool,
};
type Tags = copy::Tags;
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pair {
    pub label: String,
    pub slot_index: i64,
    pub normal: Vec<u64>,
    pub reverse: Vec<u64>,
    /// Desired visibility grip in block coordinates. Its parameter follows by
    /// the same translation, preserving the donor's parameter/grip offset.
    pub selector: Option<[f64; 3]>,
}
fn space(c: char) -> bool {
    matches!(c,'\u{9}'..='\u{d}'|'\u{1c}'..='\u{20}'|'\u{85}'|'\u{a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}')
}
pub fn names(label: &str, index: i64) -> [String; 3] {
    let mut safe = String::new();
    for c in crate::python_values::uppercase(crate::python_values::trim(label)).chars() {
        let c = if space(c) || "<>/\\\":;?*|=".contains(c) {
            '_'
        } else {
            c
        };
        if c != '_' || !safe.ends_with('_') {
            safe.push(c);
        }
    }
    let safe = safe.trim_matches(['_', '.']);
    let safe = if safe.is_empty() {
        "PS".into()
    } else {
        safe.chars().take(120).collect::<String>()
    };
    let base = format!("SLEUFBASE_{safe}_VAK{:02}", index.max(1));
    [
        format!("{base}_NORMAAL"),
        format!("{base}_REVERSE"),
        format!("{base}_VERSIE"),
    ]
}
fn own(r: RecordView<'_>) -> Option<u64> {
    copy::owner(r)
}
fn ref_code(c: i32) -> bool {
    matches!(c,320..=369|390..=399|480|481|1005)
}
fn handle(tags: &Tags) -> Result<u64, String> {
    copy::id(copy::first(tags, 5).ok_or("Zichtbaarheidsrecord mist handle")?)
}
fn tag(c: i32, v: impl ToString) -> (i32, String) {
    (c, v.to_string())
}
fn idtag(c: i32, v: u64) -> (i32, String) {
    tag(c, format!("{v:X}"))
}
fn allocate(next: &mut u64) -> Result<u64, String> {
    let id = *next;
    *next = next.checked_add(1).ok_or("Geen vrije DXF-handles")?;
    Ok(id)
}
fn record(kind: &str, id: u64, owner: u64) -> Tags {
    vec![tag(0, kind), idtag(5, id), idtag(330, owner)]
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
    Err(format!("DXF mist {name}-tabel"))
}
enum BlockPart {
    Text(String),
    Content(u64),
}
struct Donor {
    block: u64,
    root: u64,
    states: [u64; 2],
    rep: Tags,
    records: Vec<Tags>,
    grip: [f64; 3],
}
fn point(tags: &Tags) -> Result<[f64; 3], String> {
    let mut out = [0f64; 3];
    for (i, c) in [1010, 1020, 1030].into_iter().enumerate() {
        let s = tags
            .iter()
            .find(|(code, _)| *code == c)
            .ok_or("Zichtbaarheid mist parameter-/grippunt")?;
        out[i] =
            s.1.trim()
                .parse()
                .map_err(|_| "Ongeldig zichtbaarheidsgrippunt")?;
    }
    if out.iter().any(|v| !v.is_finite()) {
        return Err("Niet-eindig zichtbaarheidsgrippunt".into());
    }
    Ok(out)
}
fn donor(doc: &Document, cancel: &AtomicBool) -> Result<Donor, String> {
    let mut selected = None;
    let mut children = HashMap::<u64, Vec<RecordView<'_>>>::new();
    for (i, r) in doc.records().enumerate() {
        if i % 1024 == 0 {
            crate::input::check(cancel)?;
        }
        if r.section() != Some("OBJECTS") {
            continue;
        }
        if let Some(owner) = own(r) {
            children.entry(owner).or_default().push(r);
        }
        if r.kind() != "BLOCKVISIBILITYPARAMETER" {
            continue;
        }
        let mut states = Vec::<(&str, Vec<u64>)>::new();
        for (c, v) in r.tags() {
            if c == 303 {
                states.push((v, Vec::new()));
            } else if c == 332 {
                if let Some(s) = states.last_mut() {
                    s.1.push(copy::id(v)?);
                }
            }
        }
        if states.len() != 2 || states.iter().any(|s| s.1.len() != 1) {
            continue;
        }
        let score = if states[0].0 == "Ter goedkeuring" && states[1].0 == "Definitief" {
            100
        } else {
            10
        };
        if selected.as_ref().is_none_or(|(s, _, _)| score > *s) {
            selected = Some((score, r, [states[0].1[0], states[1].1[0]]));
        }
    }
    let (_, parameter, states) =
        selected.ok_or("Sjabloon mist een AutoCAD-donor voor Normaal/Reverse")?;
    let get = |h: u64| {
        doc.by_handle(&format!("{h:X}"))?
            .ok_or_else(|| "Onvolledige zichtbaarheidsdonor".to_owned())
    };
    let graph = get(own(parameter).ok_or("Zichtbaarheidsparameter mist owner")?)?;
    let root = get(own(graph).ok_or("Evaluation graph mist dictionary")?)?;
    if root.kind() != "DICTIONARY" {
        return Err("Zichtbaarheidsdonor heeft geen extension dictionary".into());
    }
    let block = get(own(root).ok_or("Donor-dictionary mist blok")?)?;
    if block.kind() != "BLOCK_RECORD" {
        return Err("Zichtbaarheidsdonor mist BLOCK_RECORD".into());
    }
    let mut rep = Vec::new();
    let mut active = false;
    for (c, v) in block.tags() {
        if c == 1001 {
            if active {
                break;
            }
            active = v.trim() == "AcDbBlockRepETag";
            continue;
        }
        if active {
            if c < 1000 {
                break;
            }
            rep.push(tag(c, v));
        }
    }
    if rep.is_empty() {
        return Err("Donor mist initiële AcDbBlockRepETag".into());
    }
    let root_id = root.handle().ok_or("Donor-dictionary mist handle")?;
    let mut pending = vec![root_id];
    let mut owned = HashSet::new();
    while let Some(h) = pending.pop() {
        crate::input::check(cancel)?;
        if !owned.insert(h) {
            continue;
        }
        if owned.len() > 4096 {
            return Err("Te veel zichtbaarheidsmetadata in donor".into());
        }
        if let Some(rows) = children.get(&h) {
            pending.extend(rows.iter().filter_map(|r| r.handle()));
        }
    }
    if !parameter.handle().is_some_and(|h| owned.contains(&h)) {
        return Err("Parameter valt buiten donor-dictionary".into());
    }
    let records = doc
        .records()
        .filter(|r| {
            r.section() == Some("OBJECTS") && r.handle().is_some_and(|h| owned.contains(&h))
        })
        .map(copy::tags)
        .collect::<Vec<_>>();
    let grip = records
        .iter()
        .find(|r| copy::first(r, 0) == Some("BLOCKVISIBILITYGRIP"))
        .ok_or("Donor mist visibility grip")?;
    Ok(Donor {
        block: block.handle().unwrap(),
        root: root_id,
        states,
        rep,
        grip: point(grip)?,
        records,
    })
}
fn insert(id: u64, owner: u64, name: &str, layer: &str) -> Tags {
    let mut t = record("INSERT", id, owner);
    t.extend([
        tag(100, "AcDbEntity"),
        tag(8, layer),
        tag(100, "AcDbBlockReference"),
        tag(2, name),
        tag(10, 0.),
        tag(20, 0.),
        tag(30, 0.),
    ]);
    t
}
fn state(t: &mut Tags, index: i32, id: u64) {
    if index == 1 {
        let at = t
            .iter()
            .position(|(c, v)| *c == 100 && v == "AcDbBlockReference")
            .unwrap();
        t.insert(at, tag(60, 1));
    }
    t.extend([
        tag(1001, "AcDbBlockRepETag"),
        tag(1070, 1),
        tag(1071, index),
        idtag(1005, id),
    ]);
}
/// Format translated donor coordinates with Python's 12-significant-digit
/// contract; retained scalar spellings are untouched.
fn general12(value: f64) -> String {
    let s = format!("{value:.11e}");
    let (m, e) = s.split_once('e').unwrap();
    let e = e.parse::<i32>().unwrap();
    let m = m.trim_end_matches('0').trim_end_matches('.');
    if !(-4..12).contains(&e) {
        return format!(
            "{m}e{}{:02}",
            if e >= 0 { "+" } else { "-" },
            e.unsigned_abs()
        );
    }
    let sign = if m.starts_with('-') { "-" } else { "" };
    let digits = m.trim_start_matches('-').replace('.', "");
    let at = e + 1;
    let body = if at <= 0 {
        format!("0.{}{}", "0".repeat((-at) as usize), digits)
    } else if at as usize >= digits.len() {
        format!("{}{}", digits, "0".repeat(at as usize - digits.len()))
    } else {
        format!("{}.{}", &digits[..at as usize], &digits[at as usize..])
    };
    format!("{sign}{body}")
}
/// Move each pair of freshly generated model-space entity groups into scoped
/// containers and install the template's own dynamic donor. Failure/cancellation
/// retains the original document, including staged edits.
pub fn assemble(
    doc: &mut Document,
    pairs: &[Pair],
    cancel: &AtomicBool,
) -> Result<Vec<String>, String> {
    assemble_with_progress(doc, pairs, cancel, |_, _| {})
}
pub fn assemble_with_progress(
    doc: &mut Document,
    pairs: &[Pair],
    cancel: &AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> Result<Vec<String>, String> {
    crate::input::check(cancel)?;
    if pairs.is_empty() {
        return Ok(Vec::new());
    }
    if pairs.len() > 5000 {
        return Err("Te veel Normaal/Reverse-vakken".into());
    }
    let mut count = 0usize;
    for p in pairs {
        if p.label.len() > 1_048_576 {
            return Err("Te lange variantnaam".into());
        }
        count = count
            .checked_add(p.normal.len())
            .and_then(|n| n.checked_add(p.reverse.len()))
            .ok_or("Te veel variantobjecten")?;
        if count > 2_000_000 {
            return Err("Te veel variantobjecten".into());
        }
    }
    let mut indexed = doc.indexed_copy(cancel)?;
    let d = donor(&indexed, cancel)?;
    if pairs
        .len()
        .checked_mul(d.records.len())
        .is_none_or(|n| n > 200_000)
    {
        return Err("Te veel zichtbaarheidsmetadata voor één export".into());
    }
    let model = indexed
        .records()
        .find(|r| {
            r.kind() == "BLOCK_RECORD"
                && r.first(2)
                    .is_some_and(|n| n.eq_ignore_ascii_case("*Model_Space"))
        })
        .and_then(|r| r.handle())
        .ok_or("DXF mist modelruimte")?;
    let (bt, bt_at) = table(&indexed, "BLOCK_RECORD")?;
    let (lt, lt_at) = table(&indexed, "LAYER")?;
    let (mut next, mut seed) = indexed.seed()?;
    let mut changes = crate::profile_leader_donor::appids(&indexed, &mut next, cancel)?;
    let mut block_table = String::new();
    let mut blocks = Vec::<BlockPart>::new();
    let mut layers = String::new();
    let mut objects = String::new();
    let mut entities = String::new();
    let mut names_seen = HashSet::new();
    let mut selected = HashMap::new();
    let mut wrappers = Vec::new();
    let mut added_blocks = Vec::new();
    let mut added_layers = Vec::new();
    let existing = indexed
        .records()
        .filter(|r| r.kind() == "BLOCK_RECORD")
        .filter_map(|r| r.first(2))
        .map(crate::dxf_symbols::key)
        .collect::<HashSet<_>>();
    let layer_index = indexed
        .records()
        .filter(|r| r.kind() == "LAYER")
        .filter_map(|r| r.first(2).map(|n| (crate::dxf_symbols::key(n), r)))
        .collect::<HashMap<_, _>>();
    let mut ordered = pairs.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|p| names(&p.label, p.slot_index)[2].clone());
    let mut orders = HashMap::<u64, &Vec<u64>>::new();
    progress(0, pairs.len());
    for (pair_index, p) in ordered.into_iter().enumerate() {
        crate::input::check(cancel)?;
        if p.selector.is_some_and(|q| q.iter().any(|v| !v.is_finite())) {
            return Err("Niet-eindig zichtbaarheidsgrippunt".into());
        }
        let [normal, reverse, wrapper] = names(&p.label, p.slot_index);
        let content = [
            format!("{normal}_CONTENT"),
            format!("{reverse}_CONTENT"),
            wrapper.clone(),
        ];
        for name in &content {
            if name.chars().any(char::is_control)
                || existing.contains(&crate::dxf_symbols::key(name))
                || !names_seen.insert(crate::dxf_symbols::key(name))
            {
                return Err(format!(
                    "Normaal/Reverse-blok bestaat al of is ongeldig: {name}"
                ));
            }
        }
        for name in [&normal, &reverse] {
            if !layer_index.contains_key(&crate::dxf_symbols::key(name)) {
                let h = allocate(&mut next)?;
                let mut t = record("LAYER", h, lt.handle().ok_or("Laagtabel mist handle")?);
                t.extend([
                    tag(100, "AcDbSymbolTableRecord"),
                    tag(100, "AcDbLayerTableRecord"),
                    tag(2, name),
                    tag(70, 0),
                    tag(62, 7),
                    tag(6, "Continuous"),
                ]);
                layers.push_str(&copy::encode(&t));
                added_layers.push((name.clone(), h));
            } else {
                let layer = layer_index[&crate::dxf_symbols::key(name)];
                if let Some(value) = layer.first(62).filter(|v| v.trim().starts_with('-')) {
                    changes.push(indexed.tag_change(
                        layer.index(),
                        62,
                        value.trim().trim_start_matches('-'),
                    )?);
                }
            }
        }
        let br = [
            allocate(&mut next)?,
            allocate(&mut next)?,
            allocate(&mut next)?,
        ];
        let nested = [allocate(&mut next)?, allocate(&mut next)?];
        let outer = allocate(&mut next)?;
        let mut map = HashMap::from([
            (d.block, br[2]),
            (d.states[0], nested[0]),
            (d.states[1], nested[1]),
        ]);
        for r in &d.records {
            let old = handle(r)?;
            if map.insert(old, allocate(&mut next)?).is_some() {
                return Err("Dubbele zichtbaarheidsdonorhandle".into());
            }
        }
        for (i, name) in content.iter().enumerate() {
            let mut r = record(
                "BLOCK_RECORD",
                br[i],
                bt.handle().ok_or("Bloktabel mist handle")?,
            );
            r.extend([
                tag(100, "AcDbSymbolTableRecord"),
                tag(100, "AcDbBlockTableRecord"),
                tag(2, name),
                tag(340, 0),
                tag(70, 0),
                tag(280, 1),
                tag(281, 0),
            ]);
            if i == 2 {
                r.splice(
                    2..2,
                    [
                        tag(102, "{ACAD_XDICTIONARY"),
                        idtag(360, map[&d.root]),
                        tag(102, "}"),
                    ],
                );
                r.push(tag(1001, "AcDbBlockRepETag"));
                r.extend(d.rep.clone());
                r.extend([
                    tag(1001, "AcDbDynamicBlockTrueName"),
                    tag(1000, name),
                    tag(1001, "AcDbDynamicBlockGUID"),
                    tag(1000, crate::profile_leader_donor::guid()?),
                ]);
            }
            block_table.push_str(&copy::encode(&r));
            let mut b = record("BLOCK", allocate(&mut next)?, br[i]);
            b.extend([
                tag(100, "AcDbEntity"),
                tag(8, "0"),
                tag(100, "AcDbBlockBegin"),
                tag(2, name),
                tag(70, 0),
                tag(10, 0.),
                tag(20, 0.),
                tag(30, 0.),
                tag(3, name),
                tag(1, ""),
            ]);
            blocks.push(BlockPart::Text(copy::encode(&b)));
            if i < 2 {
                let ids = if i == 0 { &p.normal } else { &p.reverse };
                if ids.is_empty() {
                    return Err("Normaal/Reverse-paar bevat een lege variant".into());
                }
                for &id in ids {
                    if selected.len() >= 2_000_000 {
                        return Err("Te veel objecten in Normaal/Reverse-vakken".into());
                    }
                    if selected.insert(id, br[i]).is_some() {
                        return Err("Sjabloonobject is in meerdere varianten geselecteerd".into());
                    }
                }
                orders.insert(br[i], ids);
                // Filled after validating all input groups, below.
                blocks.push(BlockPart::Content(br[i]));
            } else {
                for j in 0..2 {
                    let mut t = insert(nested[j], br[2], &content[j], "0");
                    state(&mut t, j as i32, nested[j]);
                    blocks.push(BlockPart::Text(copy::encode(&t)));
                }
            }
            let mut end = record("ENDBLK", allocate(&mut next)?, br[i]);
            end.extend([
                tag(100, "AcDbEntity"),
                tag(8, "0"),
                tag(100, "AcDbBlockEnd"),
            ]);
            blocks.push(BlockPart::Text(copy::encode(&end)));
            added_blocks.push(name.clone());
        }
        for r in &d.records {
            let visibility = copy::first(r, 0) == Some("BLOCKVISIBILITYPARAMETER");
            let grip = copy::first(r, 0) == Some("BLOCKVISIBILITYGRIP");
            let mut t = r.clone();
            let mut state_index = 0;
            let mut moved = HashSet::new();
            for (c, v) in &mut t {
                if *c == 5 || ref_code(*c) {
                    if let Ok(h) = copy::id(v) {
                        if let Some(n) = map.get(&h) {
                            *v = format!("{n:X}");
                        } else if h != 0 && indexed.by_handle(&format!("{h:X}"))?.is_none() {
                            return Err("Donor bevat een ontbrekende handleverwijzing".into());
                        }
                    } else {
                        return Err("Ongeldige donorhandleverwijzing".into());
                    }
                }
                if visibility && *c == 301 {
                    *v = "Versie".into();
                }
                if visibility && *c == 303 {
                    *v = if state_index == 0 {
                        "Normaal"
                    } else {
                        "Reverse"
                    }
                    .into();
                    state_index += 1;
                }
                if (visibility || grip) && matches!(*c, 1010 | 1020 | 1030) && moved.insert(*c) {
                    if let Some(q) = p.selector {
                        let axis = ((*c - 1010) / 10) as usize;
                        let base: f64 = v.trim().parse().map_err(|_| "Ongeldig donorgrippunt")?;
                        let translated = base + (q[axis] - d.grip[axis]);
                        if !translated.is_finite() {
                            return Err("Zichtbaarheidsgrippunt valt buiten getalbereik".into());
                        }
                        *v = general12(translated);
                    }
                }
            }
            objects.push_str(&copy::encode(&t));
        }
        entities.push_str(&copy::encode(&insert(outer, model, &wrapper, "0")));
        wrappers.push(wrapper);
        progress(pair_index + 1, pairs.len());
    }
    if selected.len() > 2_000_000 {
        return Err("Te veel objecten in Normaal/Reverse-vakken".into());
    }
    let mut moved = HashMap::<u64, String>::new();
    let mut active = None;
    let mut found = HashSet::new();
    for r in indexed
        .records()
        .filter(|r| r.section() == Some("ENTITIES"))
    {
        crate::input::check(cancel)?;
        let child = matches!(r.kind(), "ATTRIB" | "VERTEX" | "SEQEND");
        if !child {
            active = None;
            if let Some(h) = r.handle().filter(|h| selected.contains_key(h)) {
                if own(r) != Some(model) {
                    return Err("Variant bevat een object buiten modelruimte".into());
                }
                active = Some((h, selected[&h]));
                found.insert(h);
            }
        } else if r.handle().is_some_and(|h| selected.contains_key(&h)) {
            return Err("Selecteer de hoofdentiteit van een variant".into());
        }
        if let Some((parent, owner)) = active {
            let mut t = copy::tags(r);
            if child {
                // ezdxf's move_to_layout binds linked records to their parent,
                // including sources serialized with a layout owner. Reading
                // through ezdxf alone silently hides those stale owners.
                copy::set_owner(&mut t, parent);
            } else if !child {
                copy::set_owner(&mut t, owner);
            }
            moved.entry(parent).or_default().push_str(&copy::encode(&t));
            changes.push(indexed.record_change(r.index(), String::new()));
        }
    }
    if found.len() != selected.len() {
        return Err("Variant verwijst naar een ontbrekend modelobject".into());
    }
    let mut block_text = String::new();
    for part in blocks {
        match part {
            BlockPart::Text(t) => block_text.push_str(&t),
            BlockPart::Content(owner) => {
                for id in orders.get(&owner).ok_or("Variant mist inhoud")?.iter() {
                    block_text.push_str(&moved.remove(id).ok_or("Variant mist hoofdentiteit")?);
                }
            }
        }
    }
    changes.extend([
        Change {
            start: bt_at,
            end: bt_at,
            text: block_table,
        },
        Change {
            start: lt_at,
            end: lt_at,
            text: layers,
        },
        Change {
            start: indexed.section_end("BLOCKS")?,
            end: indexed.section_end("BLOCKS")?,
            text: block_text,
        },
        Change {
            start: indexed.section_end("OBJECTS")?,
            end: indexed.section_end("OBJECTS")?,
            text: objects,
        },
        Change {
            start: indexed.section_end("ENTITIES")?,
            end: indexed.section_end("ENTITIES")?,
            text: entities,
        },
        indexed.tag_change(
            bt.index(),
            70,
            &(indexed
                .records()
                .filter(|r| r.kind() == "BLOCK_RECORD")
                .count()
                + added_blocks.len())
            .to_string(),
        )?,
        indexed.tag_change(
            lt.index(),
            70,
            &(indexed.layer_count() + added_layers.len()).to_string(),
        )?,
    ]);
    seed.text = format!("{next:X}");
    changes.push(seed);
    crate::input::check(cancel)?;
    indexed.apply(changes)?;
    for name in added_blocks {
        indexed.added_blocks.insert(
            crate::dxf_symbols::key(&name),
            AddedBlock {
                base: [0.; 3],
                attributes: Vec::new(),
            },
        );
    }
    for (name, handle) in added_layers {
        indexed.added_layers.insert(
            crate::dxf_symbols::key(&name),
            AddedSymbol { handle, at: lt_at },
        );
    }
    crate::input::check(cancel)?;
    *doc = indexed;
    Ok(wrappers)
}
