//! Install or retain approved AutoCAD profile leader geometry and parameters.
//! Native parameter/action objects remain intact. Template prototype INSERT
//! cloning is a separate stage; dynamic blocks are never flattened here.
use crate::dxf_document::{AddedBlock, Change, Document, RecordView};
use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
    sync::atomic::AtomicBool,
};

pub const BLOCK_NAME: &str = "SAL-VERWIJZING_LEIDING_BOVENKANT-SOD";
pub const DONOR_SHA256: &str = "f975ccf4cbe8d98aba66dc52300c7c255e9c2d0a86aff4f78f31b7c4c0dc4c3b";
const GEOMETRY: [(&str, &str, i32); 4] = [
    ("CIRCLE", "2F6", 0),
    ("OMSCHRIJVING", "2F7", 1),
    ("LWPOLYLINE", "2FB", 2),
    ("HOOGTE", "2FC", 3),
];
type Tags = Vec<(i32, String)>;
fn id(value: &str) -> Result<u64, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 16 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Ongeldige profielverwijzingshandle".into());
    }
    u64::from_str_radix(value, 16).map_err(|_| "Ongeldige profielverwijzingshandle".into())
}
fn handle_reference(code: i32) -> bool {
    matches!(code, 320..=369 | 390..=399 | 480 | 481 | 1005)
}
fn own(record: RecordView<'_>) -> Option<u64> {
    // Reactor/extension dictionary groups can precede the actual owner.
    let mut nested = false;
    for (code, value) in record.tags().take_while(|(c, _)| *c < 1000) {
        if code == 102 {
            nested = value.trim() != "}";
        }
        if code == 330 && !nested {
            return id(value).ok();
        }
    }
    None
}
fn xdict(record: RecordView<'_>) -> Result<Option<u64>, String> {
    let mut dictionary = false;
    let mut result = None;
    for (code, value) in record.tags().take_while(|(c, _)| *c < 1000) {
        if code == 102 {
            dictionary = value.trim().eq_ignore_ascii_case("{ACAD_XDICTIONARY");
        }
        if code == 360 && dictionary && result.replace(id(value)?).is_some() {
            return Err("Dubbele blok-extensiedictionary".into());
        }
    }
    Ok(result)
}
fn remove_apps(record: RecordView<'_>, apps: &[&str]) -> Tags {
    let mut remove = false;
    record
        .tags()
        .filter_map(|(c, v)| {
            if c == 1001 {
                remove = apps.contains(&v.trim());
            }
            (!remove).then(|| (c, v.to_owned()))
        })
        .collect()
}
fn encoded(tags: &[(i32, String)]) -> String {
    let mut text = String::new();
    for (code, value) in tags {
        writeln!(text, "{code:>3}\n{value}").unwrap();
    }
    text
}
pub(crate) fn guid() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| format!("Dynamische blokidentiteit: {e}"))?;
    bytes[6] = (bytes[6] & 15) | 0x40;
    bytes[8] = (bytes[8] & 63) | 0x80;
    let h = bytes.iter().map(|b| format!("{b:02X}")).collect::<String>();
    Ok(format!(
        "{{{}-{}-{}-{}-{}}}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    ))
}
fn existing_dynamic(document: &Document, root: u64, cancel: &AtomicBool) -> Result<bool, String> {
    let mut descendants: HashMap<u64, Vec<RecordView<'_>>> = HashMap::new();
    for (i, record) in document.records().enumerate() {
        if i % 1024 == 0 {
            crate::input::check(cancel)?;
        }
        if record.section() == Some("OBJECTS") {
            if let Some(owner) = own(record) {
                descendants.entry(owner).or_default().push(record);
            }
        }
    }
    let mut visited = HashSet::new();
    let mut pending = vec![root];
    let mut kinds = HashSet::new();
    while let Some(handle) = pending.pop() {
        crate::input::check(cancel)?;
        if !visited.insert(handle) {
            continue;
        }
        if let Some(children) = descendants.get(&handle) {
            for r in children {
                kinds.insert(r.kind());
                if let Some(h) = r.handle() {
                    pending.push(h);
                }
            }
        }
    }
    Ok([
        "BLOCKPOLARPARAMETER",
        "BLOCKPOLARGRIP",
        "BLOCKSTRETCHACTION",
        "BLOCKMOVEACTION",
        "BLOCKSCALEACTION",
    ]
    .iter()
    .all(|t| kinds.contains(t)))
}
fn verify_geometry(records: &[RecordView<'_>; 4]) -> Result<(), String> {
    fn number(r: RecordView<'_>, code: i32, expected: f64) -> Result<(), String> {
        let n = r
            .first(code)
            .unwrap_or("0")
            .trim()
            .parse::<f64>()
            .map_err(|_| "Ongeldige donorgeometrie")?;
        if !n.is_finite() || (n - expected).abs() > 1e-9 {
            return Err("Profielverwijzingsgeometrie wijkt af van de goedgekeurde donor".into());
        }
        Ok(())
    }
    for r in records {
        for (code, value) in [(210, 0.), (220, 0.)] {
            number(*r, code, value)?;
        }
        if r.first(230).is_some() {
            number(*r, 230, 1.)?;
        }
    }
    for (code, value) in [(10, 0.), (20, -0.5), (30, 0.), (40, 0.5)] {
        number(records[0], code, value)?;
    }
    for (r, x) in [(records[1], -0.5), (records[3], 2.)] {
        for (code, value) in [(10, x), (20, 10.), (30, 0.), (40, 1.5), (50, 90.)] {
            number(r, code, value)?;
        }
    }
    let vertices: Vec<_> = records[2]
        .tags()
        .take_while(|(c, _)| *c < 1000 && *c != 101)
        .filter(|(c, _)| matches!(c, 10 | 20))
        .collect();
    for (code, value) in [(70, 0.), (38, 0.), (39, 0.)] {
        number(records[2], code, value)?;
    }
    if records[2]
        .tags()
        .take_while(|(c, _)| *c < 1000)
        .any(|(c, v)| c == 42 && v.trim().parse::<f64>().ok() != Some(0.))
    {
        return Err("Donor-verwijslijn mag geen boogsegment bevatten".into());
    }
    if vertices.len() != 4 {
        return Err("Donor-verwijslijn moet twee punten hebben".into());
    }
    for ((code, value), (wanted, number)) in
        vertices
            .into_iter()
            .zip([(10, 0.), (20, 1.), (10, 0.), (20, 10.)])
    {
        if code != wanted || value.trim().parse::<f64>().ok() != Some(number) {
            return Err("Donor-verwijslijn wijkt af van de goedgekeurde donor".into());
        }
    }
    Ok(())
}
fn metadata(
    block: u64,
    geometry: [u64; 4],
    next: &mut u64,
    cancel: &AtomicBool,
) -> Result<(u64, String), String> {
    let donor: Vec<Tags> = serde_json::from_str(include_str!("data/profile-leader-donor.json"))
        .map_err(|e| format!("Ongeldige ingebouwde donor: {e}"))?;
    let mut map = HashMap::from([(id("2E5")?, block)]);
    for (handle, (_, old, _)) in geometry.into_iter().zip(GEOMETRY) {
        map.insert(id(old)?, handle);
    }
    for tags in &donor {
        let old = id(&tags
            .iter()
            .find(|(c, _)| *c == 5)
            .ok_or("Donor-metadata mist handle")?
            .1)?;
        if map.insert(old, *next).is_some() {
            return Err("Dubbele donorhandle".into());
        }
        *next = next.checked_add(1).ok_or("Geen vrije DXF-handles")?;
    }
    let root = map[&id("2E6")?];
    let mut objects = String::new();
    for tags in donor {
        crate::input::check(cancel)?;
        objects.push_str(&encoded(&remap(tags, &map)?));
    }
    Ok((root, objects))
}
fn remap(tags: Tags, map: &HashMap<u64, u64>) -> Result<Tags, String> {
    tags.into_iter()
        .map(|(code, value)| {
            if code == 5 || handle_reference(code) {
                let old = id(&value)?;
                if old != 0 {
                    return Ok((
                        code,
                        format!(
                            "{:X}",
                            map.get(&old)
                                .ok_or("Donor bevat een externe handleverwijzing")?
                        ),
                    ));
                }
            }
            Ok((code, value))
        })
        .collect()
}
fn attach_block(mut tags: Tags, root: u64) -> Result<Tags, String> {
    let at = tags
        .iter()
        .position(|(c, _)| *c == 5)
        .ok_or("Blokhandle ontbreekt")?
        + 1;
    tags.splice(
        at..at,
        [
            (102, "{ACAD_XDICTIONARY".into()),
            (360, format!("{root:X}")),
            (102, "}".into()),
        ],
    );
    for code in [280, 281] {
        if let Some((_, v)) = tags.iter_mut().find(|(c, _)| *c == code) {
            *v = "     1".into();
        } else {
            tags.push((code, "     1".into()));
        }
    }
    tags.extend([
        (1001, "AcDbDynamicBlockTrueName".into()),
        (1000, BLOCK_NAME.into()),
        (1001, "AcDbDynamicBlockGUID".into()),
        (1000, guid()?),
        (1001, "AcDbBlockRepETag".into()),
        (1070, "     1".into()),
        (1071, format!("{:9}", 4)),
    ]);
    Ok(tags)
}
fn rep_tag(mut tags: Tags, index: i32, handle: u64) -> Tags {
    tags.extend([
        (1001, "AcDbBlockRepETag".into()),
        (1070, "     1".into()),
        (1071, format!("{index:9}")),
        (
            1005,
            if index == 0 || index == 2 {
                format!("{handle:X}")
            } else {
                "0".into()
            },
        ),
    ]);
    tags
}
#[derive(serde::Deserialize)]
struct GeometryDefinition {
    record: Tags,
    geometry: Vec<Tags>,
}
pub(crate) fn appids(document: &Document, next: &mut u64, cancel: &AtomicBool) -> Result<Vec<Change>, String> {
    let names = [
        "AcDbBlockRepETag",
        "AcDbDynamicBlockTrueName",
        "AcDbDynamicBlockGUID",
    ];
    let missing = names
        .into_iter()
        .filter(|name| {
            !document.records().any(|r| {
                r.kind() == "APPID" && r.first(2).is_some_and(|v| v.eq_ignore_ascii_case(name))
            })
        })
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(Vec::new());
    }
    let mut table = None;
    let mut end = None;
    for r in document.records() {
        if r.kind() == "TABLE" && r.first(2) == Some("APPID") {
            table = Some(r);
        } else if table.is_some() && r.kind() == "ENDTAB" {
            end = Some(r);
            break;
        }
    }
    let table = table.ok_or("DXF mist APPID-tabel")?;
    let owner = table.handle().ok_or("APPID-tabel mist handle")?;
    let at = document.record_start(end.ok_or("APPID-tabel is niet afgesloten")?.index());
    let mut text = String::new();
    let count = document.records().filter(|r| r.kind() == "APPID").count() + missing.len();
    for name in missing {
        crate::input::check(cancel)?;
        let handle = *next;
        *next = next.checked_add(1).ok_or("Geen vrije DXF-handles")?;
        text.push_str(&encoded(&[
            (0, "APPID".into()),
            (5, format!("{handle:X}")),
            (330, format!("{owner:X}")),
            (100, "AcDbSymbolTableRecord".into()),
            (100, "AcDbRegAppTableRecord".into()),
            (2, name.into()),
            (70, "0".into()),
        ]));
    }
    Ok(vec![
        Change {
            start: at,
            end: at,
            text,
        },
        document.tag_change(table.index(), 70, &count.to_string())?,
    ])
}
/// Install approved geometry AND its complete native parameter/action graph in
/// one edit batch. Existing authored blocks/graphs are retained. Required text
/// styles/linetypes belong to template preparation; they are never substituted.
/// ATTDEFs are cached for later native INSERT batches without document reparse.
pub fn install(document: &mut Document, cancel: &AtomicBool) -> Result<bool, String> {
    crate::input::check(cancel)?;
    let key = crate::dxf_symbols::key(BLOCK_NAME);
    if document.added_blocks.contains_key(&key) {
        return if document.profile_leader_promoted {
            Ok(false)
        } else {
            Err("Nieuwe bloknaam is al in gebruik".into())
        };
    }
    let existing = document.records().any(|r| {
        matches!(r.kind(), "BLOCK" | "BLOCK_RECORD")
            && r.first(2)
                .is_some_and(|v| v.eq_ignore_ascii_case(BLOCK_NAME))
    });
    if existing {
        return promote(document, cancel);
    }
    if document.added_blocks.len() >= 10_000 {
        return Err("Te veel nieuwe sjabloonblokken".into());
    }
    if !document.has_layer("0") || !document.has_style("NLCS-ISO") {
        return Err("Profielsjabloon mist laag 0 of tekststijl NLCS-ISO".into());
    }
    for name in ["ByBlock", "Continuous"] {
        if !document.records().any(|r| {
            r.kind() == "LTYPE" && r.first(2).is_some_and(|v| v.eq_ignore_ascii_case(name))
        }) {
            return Err(format!("Profielsjabloon mist lijntype {name}"));
        }
    }
    let mut table = None;
    let mut end = None;
    for r in document.records() {
        if r.kind() == "TABLE" && r.first(2) == Some("BLOCK_RECORD") {
            table = Some(r);
        } else if table.is_some() && r.kind() == "ENDTAB" {
            end = Some(r);
            break;
        }
    }
    let table = table.ok_or("DXF mist bloktabel")?;
    let table_owner = table.handle().ok_or("Bloktabel mist handle")?;
    let table_end = document.record_start(end.ok_or("Bloktabel is niet afgesloten")?.index());
    let definition: GeometryDefinition =
        serde_json::from_str(include_str!("data/profile-leader-geometry.json"))
            .map_err(|e| format!("Ongeldige ingebouwde donorgeometrie: {e}"))?;
    let (mut next, mut seed) = document.seed()?;
    let block = next;
    let mut map = HashMap::new();
    for tags in std::iter::once(&definition.record).chain(&definition.geometry) {
        let handle = id(&tags
            .iter()
            .find(|(c, _)| *c == 5)
            .ok_or("Donorgeometrie mist handle")?
            .1)?;
        if map.insert(handle, next).is_some() {
            return Err("Dubbele donorgeometriehandle".into());
        }
        next = next.checked_add(1).ok_or("Geen vrije DXF-handles")?;
    }
    let mut record = definition.record;
    // The source block-table owner belongs to the frozen envelope, not the
    // donor's entity graph. Retarget it explicitly before remapping internals.
    for (code, value) in &mut record {
        if *code == 330 {
            *value = "0".into();
        }
    }
    let mut record = remap(record, &map)?;
    for (code, value) in &mut record {
        if *code == 330 {
            *value = format!("{table_owner:X}");
        }
    }
    let mut geometry = definition
        .geometry
        .into_iter()
        .map(|r| remap(r, &map))
        .collect::<Result<Vec<_>, _>>()?;
    let mut handles = [0u64; 4];
    let mut attributes = Vec::new();
    for tags in &geometry {
        let kind = tags.first().map(|(_, v)| v.as_str()).unwrap_or("");
        let key = if kind == "ATTDEF" {
            tags.iter()
                .find(|(c, _)| *c == 2)
                .map(|(_, v)| v.as_str())
                .unwrap_or("")
        } else {
            kind
        };
        if let Some(index) = GEOMETRY.iter().position(|(k, _, _)| *k == key) {
            handles[index] = id(&tags
                .iter()
                .find(|(c, _)| *c == 5)
                .ok_or("Donorhandle ontbreekt")?
                .1)?;
        }
        if kind == "ATTDEF" {
            attributes.push(tags.clone());
        }
    }
    if handles.contains(&0) {
        return Err("Ingebouwde donorgeometrie is onvolledig".into());
    }
    let (root, objects) = metadata(block, handles, &mut next, cancel)?;
    record = attach_block(record, root)?;
    let mut block_text = String::new();
    for tags in &mut geometry {
        let handle = id(&tags
            .iter()
            .find(|(c, _)| *c == 5)
            .ok_or("Donorhandle ontbreekt")?
            .1)?;
        if let Some(index) = handles.iter().position(|h| *h == handle) {
            *tags = rep_tag(std::mem::take(tags), index as i32, handle);
        }
        block_text.push_str(&encoded(tags));
    }
    let count = document
        .records()
        .filter(|r| r.kind() == "BLOCK_RECORD")
        .count()
        + document.added_blocks.len()
        + 1;
    let count = document.tag_change(table.index(), 70, &count.to_string())?;
    let block_end = document.section_end("BLOCKS")?;
    let objects_end = document.section_end("OBJECTS")?;
    let mut changes = appids(document, &mut next, cancel)?;
    seed.text = format!("{next:X}");
    crate::input::check(cancel)?;
    changes.extend([
        Change {
            start: table_end,
            end: table_end,
            text: encoded(&record),
        },
        Change {
            start: block_end,
            end: block_end,
            text: block_text,
        },
        Change {
            start: objects_end,
            end: objects_end,
            text: objects,
        },
        count,
        seed,
    ]);
    document.apply(changes)?;
    document.added_blocks.insert(
        key,
        AddedBlock {
            base: [0.; 3],
            attributes,
        },
    );
    document.profile_leader_promoted = true;
    Ok(true)
}
/// Initial approved donor placement. Native parameters retain their authored
/// vertical state; text_insert from the legacy static collision solver is not
/// applied to this block. The user moves its top using the AutoCAD Polar grip.
pub fn reference(
    description: String,
    depth: String,
    start: [f64; 2],
    scale: f64,
    color: Option<u16>,
) -> Result<crate::template_geometry::Entity, String> {
    if !start.into_iter().chain([scale]).all(f64::is_finite) || color.is_some_and(|c| c > 256) {
        return Err("Ongeldige profielverwijzingsplaatsing".into());
    }
    let scale = scale.max(0.005);
    let insert = [start[0], start[1] - scale];
    if !insert[1].is_finite() {
        return Err("Profielverwijzing valt buiten coördinatenbereik".into());
    }
    Ok(crate::template_geometry::Entity {
        layer: "X-XX-AL-VERWIJZING-SD".into(),
        paint: crate::template_geometry::Paint {
            color,
            ..Default::default()
        },
        layout: None,
        shape: crate::template_geometry::Shape::Block {
            name: BLOCK_NAME.into(),
            insert,
            scale,
            attributes: std::collections::BTreeMap::from([
                ("OMSCHRIJVING".into(), description),
                ("HOOGTE".into(), depth),
            ]),
            attribute_placements: Default::default(),
        },
    })
}
/// Attach the exact approved donor metadata, remapping only handles. The batch
/// is atomic, cancellable and idempotent, including before serialization.
/// Existing complete dynamic graphs are retained verbatim. An unrelated or
/// incomplete extension dictionary is rejected instead of orphaning its data.
/// Geometry installation and prototype INSERT cloning are separate stages.
pub fn promote(document: &mut Document, cancel: &AtomicBool) -> Result<bool, String> {
    crate::input::check(cancel)?;
    if document.profile_leader_promoted {
        return Ok(false);
    }
    let mut matches = document.records().filter(|r| {
        r.kind() == "BLOCK_RECORD"
            && r.first(2)
                .is_some_and(|n| n.eq_ignore_ascii_case(BLOCK_NAME))
    });
    let block = matches
        .next()
        .ok_or("Goedgekeurd profielverwijzingsblok ontbreekt")?;
    if matches.next().is_some() {
        return Err("Dubbel profielverwijzingsblok".into());
    }
    drop(matches);
    let block_handle = block.handle().ok_or("Profielverwijzingsblok mist handle")?;
    if let Some(root) = xdict(block)? {
        let dictionary = document
            .by_handle(&format!("{root:X}"))?
            .ok_or("Blok-extensiedictionary ontbreekt")?;
        if dictionary.kind() != "DICTIONARY" || own(dictionary) != Some(block_handle) {
            return Err("Blok-extensiedictionary heeft een ongeldige eigenaar of type".into());
        }
        if existing_dynamic(document, root, cancel)? {
            return Ok(false);
        }
        return Err("Bestaande profielblok-dictionary is onvolledig of bevat andere gegevens; behouden zonder wijzigingen".into());
    }
    let mut geometry = [None; 4];
    for (i, r) in document.records().enumerate() {
        if i % 1024 == 0 {
            crate::input::check(cancel)?;
        }
        if r.section() != Some("BLOCKS") || own(r) != Some(block_handle) {
            continue;
        }
        let key = if r.kind() == "ATTDEF" {
            r.first(2).unwrap_or("")
        } else {
            r.kind()
        };
        if let Some(slot) = GEOMETRY
            .iter()
            .position(|(kind, _, _)| key.eq_ignore_ascii_case(kind))
        {
            if geometry[slot].replace(r).is_some() {
                return Err("Dubbele donor-geometrieentity".into());
            }
        }
    }
    let geometry: [RecordView<'_>; 4] = geometry
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or("Profielverwijzingsblok mist donorgeometrie")?
        .try_into()
        .map_err(|_| "Ongeldige donorgeometrie")?;
    verify_geometry(&geometry)?;
    let (mut next, mut seed) = document.seed()?;
    let mut handles = [0u64; 4];
    for (i, r) in geometry.iter().enumerate() {
        handles[i] = r.handle().ok_or("Donor-geometrie mist handle")?;
    }
    let (root, objects) = metadata(block_handle, handles, &mut next, cancel)?;
    let tags = attach_block(
        remove_apps(
            block,
            &[
                "AcDbBlockRepETag",
                "AcDbDynamicBlockTrueName",
                "AcDbDynamicBlockGUID",
            ],
        ),
        root,
    )?;
    let mut changes = vec![document.record_change(block.index(), encoded(&tags))];
    for (i, r) in geometry.into_iter().enumerate() {
        let tags = rep_tag(remove_apps(r, &["AcDbBlockRepETag"]), i as i32, handles[i]);
        changes.push(document.record_change(r.index(), encoded(&tags)));
    }
    let at = document.section_end("OBJECTS")?;
    changes.push(Change {
        start: at,
        end: at,
        text: objects,
    });
    changes.extend(appids(document, &mut next, cancel)?);
    seed.text = format!("{next:X}");
    changes.push(seed);
    crate::input::check(cancel)?;
    document.apply(changes)?;
    document.profile_leader_promoted = true;
    Ok(true)
}
