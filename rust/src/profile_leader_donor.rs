//! Preserve the approved AutoCAD profile leader's parameter/action graph.
//! This final raw-DXF stage expects the approved geometry to exist already;
//! it neither reconstructs template prototypes nor flattens dynamic blocks.
use crate::dxf_document::{Change, Document, RecordView};
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
fn reference(code: i32) -> bool {
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
fn encoded(tags: &Tags) -> String {
    let mut text = String::new();
    for (code, value) in tags {
        writeln!(text, "{code:>3}\n{value}").unwrap();
    }
    text
}
fn guid() -> Result<String, String> {
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
    let donor: Vec<Tags> = serde_json::from_str(include_str!("data/profile-leader-donor.json"))
        .map_err(|e| format!("Ongeldige ingebouwde donor: {e}"))?;
    let (mut next, mut seed) = document.seed()?;
    let mut map = HashMap::new();
    map.insert(id("2E5")?, block_handle);
    for (r, (_, old, _)) in geometry.iter().zip(GEOMETRY) {
        map.insert(id(old)?, r.handle().ok_or("Donor-geometrie mist handle")?);
    }
    for tags in &donor {
        let old = id(tags
            .iter()
            .find(|(c, _)| *c == 5)
            .ok_or("Donor-metadata mist handle")?
            .1
            .as_str())?;
        if map.insert(old, next).is_some() {
            return Err("Dubbele donorhandle".into());
        }
        next = next.checked_add(1).ok_or("Geen vrije DXF-handles")?;
    }
    let mut objects = String::new();
    for tags in donor {
        crate::input::check(cancel)?;
        let mut remapped = Vec::with_capacity(tags.len());
        for (code, value) in tags {
            if code == 5 || reference(code) {
                let old = id(&value)?;
                if old == 0 {
                    remapped.push((code, value));
                } else {
                    remapped.push((
                        code,
                        format!(
                            "{:X}",
                            map.get(&old)
                                .ok_or("Donor bevat een externe handleverwijzing")?
                        ),
                    ));
                }
            } else {
                remapped.push((code, value));
            }
        }
        objects.push_str(&encoded(&remapped));
    }
    let mut tags = remove_apps(
        block,
        &[
            "AcDbBlockRepETag",
            "AcDbDynamicBlockTrueName",
            "AcDbDynamicBlockGUID",
        ],
    );
    let at = tags
        .iter()
        .position(|(c, _)| *c == 5)
        .ok_or("Blokhandle ontbreekt")?
        + 1;
    tags.splice(
        at..at,
        [
            (102, "{ACAD_XDICTIONARY".into()),
            (360, format!("{:X}", map[&id("2E6")?])),
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
    let mut changes = vec![document.record_change(block.index(), encoded(&tags))];
    for (r, (_, _, index)) in geometry.into_iter().zip(GEOMETRY) {
        let mut tags = remove_apps(r, &["AcDbBlockRepETag"]);
        tags.extend([
            (1001, "AcDbBlockRepETag".into()),
            (1070, "     1".into()),
            (1071, format!("{index:9}")),
            (
                1005,
                if index == 0 || index == 2 {
                    format!("{:X}", r.handle().unwrap())
                } else {
                    "0".into()
                },
            ),
        ]);
        changes.push(document.record_change(r.index(), encoded(&tags)));
    }
    let at = document.section_end("OBJECTS")?;
    changes.push(Change {
        start: at,
        end: at,
        text: objects,
    });
    seed.text = format!("{next:X}");
    changes.push(seed);
    crate::input::check(cancel)?;
    document.apply(changes)?;
    document.profile_leader_promoted = true;
    Ok(true)
}
