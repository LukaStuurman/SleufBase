//! Same-document entity copies: independent owned dictionaries/linked entities,
//! shared named resources, no copied reactors, fresh image reactors.
use crate::{
    dxf_document::{Change, Document, RecordView},
    input,
};
use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
    sync::atomic::AtomicBool,
};
pub(crate) type Tags = Vec<(i32, String)>;
pub(crate) fn tags(r: RecordView<'_>) -> Tags {
    r.tags().map(|(c, v)| (c, v.into())).collect()
}
pub(crate) fn encode(tags: &Tags) -> String {
    let mut text = String::new();
    for (c, v) in tags {
        writeln!(text, "{c}\n{v}").unwrap();
    }
    text
}
pub(crate) fn first(tags: &Tags, code: i32) -> Option<&str> {
    let mut depth = 0;
    for (c, v) in tags {
        if *c == 101 || *c >= 1000 {
            break;
        }
        if *c == 102 {
            if v.starts_with('{') {
                depth += 1;
            } else if v == "}" {
                depth -= 1;
            }
            continue;
        }
        if depth == 0 && *c == code {
            return Some(v);
        }
    }
    None
}
pub(crate) fn id(value: &str) -> Result<u64, String> {
    u64::from_str_radix(value.trim(), 16).map_err(|_| "Ongeldige kopieerhandle".into())
}
pub(crate) fn owner(r: RecordView<'_>) -> Option<u64> {
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
            return id(v).ok();
        }
    }
    None
}
pub(crate) fn replace(tags: &mut Tags, code: i32, value: String) {
    if let Some((_, v)) = tags.iter_mut().find(|(c, _)| *c == code) {
        *v = value;
    } else {
        tags.push((code, value));
    }
}
fn clean(source: Tags) -> Tags {
    let mut result = Vec::new();
    let mut skip = 0;
    for (c, v) in source {
        if skip > 0 {
            if c == 102 {
                if v.starts_with('{') {
                    skip += 1;
                } else if v == "}" {
                    skip -= 1;
                }
            }
            continue;
        }
        if c == 102 && v == "{ACAD_REACTORS" {
            skip = 1;
            continue;
        }
        result.push((c, v));
    }
    result
}
fn set_owner(tags: &mut Tags, value: u64) {
    let mut depth = 0;
    let mut at = 1;
    for (i, (c, v)) in tags.iter_mut().enumerate() {
        if *c == 100 {
            at = i;
            break;
        }
        if *c == 102 {
            if v.starts_with('{') {
                depth += 1;
            } else if v == "}" {
                depth -= 1;
            }
        }
        if *c == 330 && depth == 0 {
            *v = format!("{value:X}");
            return;
        }
    }
    tags.insert(at, (330, format!("{value:X}")));
}
pub(crate) fn scope_set(
    tags: &mut Tags,
    subclass: &str,
    code: i32,
    value: String,
) -> Result<(), String> {
    let start = tags
        .iter()
        .position(|(c, v)| *c == 100 && v == subclass)
        .ok_or_else(|| {
            format!(
                "Ontbrekende DXF-kopieersubklasse {subclass} in {}",
                first(tags, 0).unwrap_or("record")
            )
        })?
        + 1;
    let end = tags[start..]
        .iter()
        .position(|(c, _)| matches!(*c, 100 | 101) || *c >= 1000)
        .map_or(tags.len(), |i| i + start);
    if let Some((_, v)) = tags[start..end].iter_mut().find(|(c, _)| *c == code) {
        *v = value;
    } else {
        tags.insert(end, (code, value));
    }
    Ok(())
}
pub(crate) struct Copies<'a> {
    doc: &'a Document,
    pub next: u64,
    pub objects: String,
    pub image_reactors: HashMap<u64, Vec<u64>>,
    count: usize,
    cancel: &'a AtomicBool,
}
impl<'a> Copies<'a> {
    pub fn cancel(&self) -> &'a AtomicBool {
        self.cancel
    }
    pub fn new(doc: &'a Document, next: u64, cancel: &'a AtomicBool) -> Self {
        Self {
            doc,
            next,
            objects: String::new(),
            image_reactors: HashMap::new(),
            count: 0,
            cancel,
        }
    }
    pub fn allocate(&mut self) -> Result<u64, String> {
        input::check(self.cancel)?;
        self.count += 1;
        if self.count > 100_000 {
            return Err("Te veel gekopieerde DXF-records".into());
        }
        let value = self.next;
        self.next = self.next.checked_add(1).ok_or("Geen vrije DXF-handles")?;
        Ok(value)
    }
    fn xdict(
        &mut self,
        old: u64,
        new_owner: u64,
        visiting: &mut HashSet<u64>,
        depth: usize,
    ) -> Result<u64, String> {
        if depth > 16 || !visiting.insert(old) {
            return Err("Cyclische of te diepe uitbreidingsdictionary".into());
        }
        let r = self
            .doc
            .by_handle(&format!("{old:X}"))?
            .ok_or("Ontbrekend eigendom in uitbreidingsdictionary")?;
        if r.section() != Some("OBJECTS") {
            return Err("Uitbreidingsdictionary verwijst buiten OBJECTS".into());
        }
        let kind = r.kind();
        if !matches!(
            kind,
            "DICTIONARY" | "XRECORD" | "DICTIONARYVAR" | "ACDBDICTIONARYWDFLT"
        ) {
            return Err(format!(
                "Kopiëren van dictionary-object {kind} is nog niet ondersteund"
            ));
        }
        let mut copied = clean(tags(r));
        let handle = self.allocate()?;
        replace(&mut copied, 5, format!("{handle:X}"));
        set_owner(&mut copied, new_owner);
        // A soft-owned dictionary's entries still have one DXF owner. Sharing
        // their old objects makes a copied dictionary invalid: audit removes
        // its keys. Copy by actual ownership, retaining the original flag.
        if matches!(kind, "DICTIONARY" | "ACDBDICTIONARYWDFLT") {
            for (c, v) in &mut copied {
                if matches!(*c, 350 | 360) {
                    let child = id(v)?;
                    if child != 0 {
                        let record = self
                            .doc
                            .by_handle(&format!("{child:X}"))?
                            .ok_or("Dictionarywaarde ontbreekt")?;
                        if owner(record) != Some(old) {
                            return Err("Dictionarywaarde heeft ongeldig eigendom".into());
                        }
                        *v = format!("{:X}", self.xdict(child, handle, visiting, depth + 1)?);
                    }
                }
            }
        }
        self.objects.push_str(&encode(&copied));
        visiting.remove(&old);
        Ok(handle)
    }
    pub fn entity(&mut self, r: RecordView<'a>, owner: u64) -> Result<(u64, Tags), String> {
        if matches!(
            r.kind(),
            "DIMENSION"
                | "ACAD_PROXY_ENTITY"
                | "ACAD_TABLE"
                | "PDFUNDERLAY"
                | "DGNUNDERLAY"
                | "DWFUNDERLAY"
        ) {
            return Err(format!(
                "Zelfstandig kopiëren van {} wordt nog niet ondersteund",
                r.kind()
            ));
        }
        let handle = self.allocate()?;
        let mut copied = clean(tags(r));
        replace(&mut copied, 5, format!("{handle:X}"));
        set_owner(&mut copied, owner);
        let mut extension = false;
        for (c, v) in &mut copied {
            if *c == 102 {
                extension = v == "{ACAD_XDICTIONARY";
            } else if extension && *c == 360 {
                *v = format!("{:X}", self.xdict(id(v)?, handle, &mut HashSet::new(), 0)?);
            }
        }
        if r.kind() == "IMAGE" {
            let definition = id(first(&copied, 340).ok_or("Afbeeldingsdefinitie ontbreekt")?)?;
            let def = self
                .doc
                .by_handle(&format!("{definition:X}"))?
                .ok_or("Afbeeldingsdefinitie ontbreekt")?;
            if def.kind() != "IMAGEDEF" {
                return Err("Ongeldige gedeelde afbeeldingsdefinitie".into());
            }
            let reactor = self.allocate()?;
            scope_set(&mut copied, "AcDbRasterImage", 360, format!("{reactor:X}"))?;
            write!(self.objects,"0\nIMAGEDEF_REACTOR\n5\n{reactor:X}\n330\n{handle:X}\n100\nAcDbRasterImageDefReactor\n90\n2\n330\n{handle:X}\n").unwrap();
            self.image_reactors
                .entry(definition)
                .or_default()
                .push(reactor);
        }
        Ok((handle, copied))
    }
    pub fn changes(&self) -> Result<Vec<Change>, String> {
        let mut changes = Vec::new();
        if !self.objects.is_empty() {
            let at = self.doc.section_end("OBJECTS")?;
            changes.push(Change {
                start: at,
                end: at,
                text: self.objects.clone(),
            });
        }
        for (&handle, reactors) in &self.image_reactors {
            let r = self
                .doc
                .by_handle(&format!("{handle:X}"))?
                .ok_or("Afbeeldingsdefinitie ontbreekt")?;
            let mut t = tags(r);
            let mut group = false;
            let mut at = None;
            for (i, (c, v)) in t.iter().enumerate() {
                if *c == 102 && v == "{ACAD_REACTORS" {
                    group = true;
                } else if group && *c == 102 && v == "}" {
                    at = Some(i);
                    break;
                }
            }
            let values = reactors
                .iter()
                .map(|h| (330, format!("{h:X}")))
                .collect::<Vec<_>>();
            if let Some(at) = at {
                t.splice(at..at, values);
            } else {
                let at = t.iter().position(|(c, _)| *c == 100).unwrap_or(t.len());
                let mut group = vec![(102, "{ACAD_REACTORS".into())];
                group.extend(values);
                group.push((102, "}".into()));
                t.splice(at..at, group);
            }
            changes.push(self.doc.record_change(r.index(), encode(&t)));
        }
        Ok(changes)
    }
}
