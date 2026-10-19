//! Source-preserving modern ASCII DXF editing for template work.
//! Unknown records, sections, layouts, dictionaries and proxy data remain bytes.
//! This is deliberately separate from the lossy overlay/scene importer.
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Write,
    ops::Range,
    path::Path,
    sync::atomic::AtomicBool,
};
const MAX_BYTES: usize = 256 * 1024 * 1024;
const MAX_RECORDS: usize = 2_000_000;
const MAX_TEXT: usize = 1024 * 1024;
const NO_SECTION: usize = usize::MAX;

struct Span {
    code: i32,
    whole: Range<usize>,
    value: Range<usize>,
}

fn spans(text: &str, base: usize) -> impl Iterator<Item = Span> + '_ {
    let text_without_bom = text.trim_start_matches('\u{feff}');
    let mut offset = base + text.len() - text_without_bom.len();
    let mut lines = text_without_bom.split_inclusive('\n');
    std::iter::from_fn(move || {
        let code = lines.next()?;
        let value = lines.next()?;
        let start = offset;
        offset += code.len() + value.len();
        let value_start = start + code.len();
        let content = value.strip_suffix('\n').unwrap_or(value);
        let content = content.strip_suffix('\r').unwrap_or(content);
        Some(Span {
            code: code.trim().parse().expect("validated DXF code"),
            whole: start..offset,
            value: value_start..value_start + content.len(),
        })
    })
}
struct Record {
    range: Range<usize>,
    kind: Range<usize>,
    section: usize,
    handle: Option<u64>,
}
struct Edit {
    end: usize,
    replacement: String,
}
pub(crate) struct Change {
    pub start: usize,
    pub end: usize,
    pub text: String,
}
pub(crate) struct AddedBlock {
    pub base: [f64; 3],
    pub attributes: Vec<Vec<(i32, String)>>,
}
#[derive(Clone, Copy)]
pub(crate) struct AddedSymbol {
    pub handle: u64,
    pub at: usize,
}
pub(crate) type InsertedValues = BTreeMap<Option<u64>, Vec<(i32, String)>>;

pub struct Document {
    source: String,
    records: Vec<Record>,
    sections: Vec<Range<usize>>,
    handles: HashMap<u64, usize>,
    edits: BTreeMap<usize, Edit>,
    newline: &'static str,
    output_len: usize,
    pub(crate) image_batch_added: bool,
    pub(crate) added_layers: HashMap<String, AddedSymbol>,
    pub(crate) added_styles: HashMap<String, AddedSymbol>,
    // Keep bases/ATTDEFs for new blocks so later batches can reference them
    // without reparsing the full source document or cloning attribute strings.
    pub(crate) added_blocks: HashMap<String, AddedBlock>,
    pub(crate) profile_leader_promoted: bool,
}
#[derive(Clone, Copy)]
pub struct RecordView<'a> {
    document: &'a Document,
    index: usize,
}
impl<'a> RecordView<'a> {
    /// Current scalar value, including staged source-tag and missing-tag edits.
    pub(crate) fn current_value(self, code: i32) -> Result<Option<&'a str>, String> {
        let r = &self.document.records[self.index];
        if let Some(edit) = self
            .document
            .edits
            .get(&r.range.start)
            .filter(|e| e.end == r.range.end)
        {
            return Ok(spans(&edit.replacement, 0)
                .take_while(|s| s.code < 1000)
                .find(|s| s.code == code)
                .map(|s| &edit.replacement[s.value]));
        }
        if let Some(tag) = spans(&self.document.source[r.range.clone()], r.range.start)
            .take_while(|s| s.code < 1000)
            .find(|s| s.code == code)
        {
            if let Some(edit) = self.document.edits.get(&tag.value.start) {
                return Ok(Some(&edit.replacement));
            }
            if let Some(edit) = self
                .document
                .edits
                .get(&tag.whole.start)
                .filter(|e| e.end > tag.whole.start)
            {
                let value = spans(&edit.replacement, 0)
                    .find(|s| s.code == code)
                    .ok_or("Bewerkte DXF-tag ontbreekt")?;
                return Ok(Some(&edit.replacement[value.value]));
            }
            return Ok(Some(&self.document.source[tag.value]));
        }
        let at = self.document.before_xdata(self.index);
        if let Some(edit) = self.document.edits.get(&at).filter(|e| e.end == at) {
            return Ok(spans(&edit.replacement, 0)
                .take_while(|s| s.code != 0)
                .find(|s| s.code == code)
                .map(|s| &edit.replacement[s.value]));
        }
        Ok(None)
    }
    /// Current single-line text, including staged text/tag replacements. Other
    /// tag iteration intentionally continues to describe the original source.
    pub(crate) fn current_text(self) -> Result<&'a str, String> {
        if !matches!(self.kind(), "TEXT" | "ATTRIB" | "ATTDEF") {
            return Err("DXF-record is geen enkelregelige tekst".into());
        }
        let r = &self.document.records[self.index];
        let tag = spans(&self.document.source[r.range.clone()], r.range.start)
            .take_while(|s| s.code != 101 && s.code < 1000)
            .find(|s| s.code == 1)
            .ok_or("DXF-tekstinhoud ontbreekt")?;
        if let Some(edit) = self.document.edits.get(&tag.value.start) {
            return Ok(&edit.replacement);
        }
        if let Some(edit) = self.document.edits.get(&tag.whole.start) {
            let text = spans(&edit.replacement, 0)
                .find(|s| s.code == 1)
                .ok_or("Bewerkte DXF-tekstinhoud ontbreekt")?;
            return Ok(&edit.replacement[text.value]);
        }
        Ok(&self.document.source[tag.value])
    }
    pub(crate) fn index(self) -> usize {
        self.index
    }
    pub fn kind(self) -> &'a str {
        self.document.source[self.document.records[self.index].kind.clone()].trim()
    }
    pub fn section(self) -> Option<&'a str> {
        self.document
            .sections
            .get(self.document.records[self.index].section)
            .map(|r| self.document.source[r.clone()].trim())
    }
    pub fn handle(self) -> Option<u64> {
        self.document.records[self.index].handle
    }
    /// Original source tags; staged edits are applied only by write_to.
    pub fn tags(self) -> impl Iterator<Item = (i32, &'a str)> {
        let record = &self.document.records[self.index];
        let source = &self.document.source;
        spans(&source[record.range.clone()], record.range.start)
            .map(move |s| (s.code, &source[s.value]))
    }
    pub fn first(self, code: i32) -> Option<&'a str> {
        self.tags().find_map(|(c, v)| (c == code).then_some(v))
    }
}

fn handle(value: &str) -> Result<u64, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 16 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Ongeldige DXF-handle".into());
    }
    u64::from_str_radix(value, 16).map_err(|_| "Ongeldige DXF-handle".into())
}
impl Document {
    pub(crate) fn header_scalar(&self,name:&str,code:i32,value:&str)->Result<Change,String>{
        if !name.starts_with('$') || name.len()>64 || name.chars().any(char::is_control) || value.len()>MAX_TEXT || value.chars().any(char::is_control){return Err("Ongeldige DXF-headervariabele".into());}
        for r in &self.records{
            if r.section==NO_SECTION || self.source[self.sections[r.section].clone()].trim()!="HEADER"{continue;}
            let mut active=false;let mut at=None;
            for s in spans(&self.source[r.range.clone()],r.range.start){
                if s.code==9{
                    if active{at=Some(s.whole.start);break;}
                    active=self.source[s.value.clone()].trim()==name;
                }else if active && s.code==code{return Ok(Change{start:s.value.start,end:s.value.end,text:value.into()});}
            }
            if active{
                let at=at.unwrap_or(r.range.end);return Ok(Change{start:at,end:at,text:format!("{code}{}{value}{}",self.newline,self.newline)});
            }
            let at=r.range.end;return Ok(Change{start:at,end:at,text:format!("9{}{name}{}{code}{}{value}{}",self.newline,self.newline,self.newline,self.newline)});
        }Err("DXF mist HEADER-sectie".into())
    }
    /// Materialize staged edits only when a structural operation needs a fresh
    /// record index. The caller keeps the original until the whole operation
    /// succeeds. Added symbols become ordinary source records in the copy.
    pub(crate) fn indexed_copy(&self,cancel:&AtomicBool)->Result<Self,String>{
        let mut bytes=Vec::with_capacity(self.output_len);
        self.write_to(&mut bytes,cancel)?;
        let mut result=Self::from_bytes(bytes,cancel)?;
        result.image_batch_added=self.image_batch_added;
        result.profile_leader_promoted=self.profile_leader_promoted;
        Ok(result)
    }
    pub(crate) fn has_layer(&self, name: &str) -> bool {
        let key = crate::dxf_symbols::key(name);
        self.added_layers.contains_key(&key)
            || self.records().any(|r| {
                r.kind() == "LAYER"
                    && r.first(2)
                        .is_some_and(|v| crate::dxf_symbols::key(v) == key)
            })
    }
    pub(crate) fn has_style(&self, name: &str) -> bool {
        let key = crate::dxf_symbols::key(name);
        self.added_styles.contains_key(&key)
            || self.records().any(|r| {
                r.kind() == "STYLE"
                    && r.first(2)
                        .is_some_and(|v| crate::dxf_symbols::key(v) == key)
            })
    }
    pub(crate) fn layer_count(&self) -> usize {
        self.records().filter(|r| r.kind() == "LAYER").count() + self.added_layers.len()
    }
    pub(crate) fn record_start(&self, index: usize) -> usize {
        self.records[index].range.start
    }
    pub(crate) fn record_change(&self, index: usize, text: String) -> Change {
        let range = &self.records[index].range;
        Change {
            start: range.start,
            end: range.end,
            text,
        }
    }
    pub(crate) fn before_xdata(&self, index: usize) -> usize {
        let r = &self.records[index];
        spans(&self.source[r.range.clone()], r.range.start)
            .find(|s| s.code >= 1000)
            .map_or(r.range.end, |s| s.whole.start)
    }
    /// Edit unique scalar tags of a record, preserving every other source byte.
    /// Repeated calls start from the previous whole-record edit. Refuse mixed
    /// granular edits rather than accidentally discarding another operation.
    pub(crate) fn record_scalars(
        &self,
        index: usize,
        values: &[(i32, String)],
    ) -> Result<Change, String> {
        let range = &self.records[index].range;
        let mut text = &self.source[range.clone()];
        for (&at, edit) in self.edits.range(range.start..range.end) {
            if at != range.start || edit.end != range.end {
                return Err("DXF-record bevat al afzonderlijke tagbewerkingen".into());
            }
            text = &edit.replacement;
        }
        let mut patches = Vec::new();
        let mut missing = String::new();
        let mut codes = HashSet::new();
        for (code, value) in values {
            if !codes.insert(*code)
                || !matches!(*code, 12 | 22 | 16 | 26 | 36 | 17 | 27 | 37 | 45 | 51)
                || value.len() > MAX_TEXT
                || value.chars().any(char::is_control)
            {
                return Err("Ongeldige viewporttag".into());
            }
            let mut found = spans(text, 0)
                .take_while(|s| s.code < 1000)
                .filter(|s| s.code == *code);
            if let Some(tag) = found.next() {
                if found.next().is_some() {
                    return Err("Dubbele viewporttag".into());
                }
                patches.push((tag.value, value.as_str()));
            } else {
                missing.push_str(&format!("{code}{}{value}{}", self.newline, self.newline));
            }
        }
        let at = spans(text, 0)
            .find(|s| s.code >= 1000)
            .map_or(text.len(), |s| s.whole.start);
        if !missing.is_empty() {
            patches.push((at..at, missing.as_str()));
        }
        patches.sort_by_key(|(r, _)| r.start);
        let mut result = String::with_capacity(text.len() + missing.len());
        let mut cursor = 0;
        for (r, value) in patches {
            result.push_str(&text[cursor..r.start]);
            result.push_str(value);
            cursor = r.end;
        }
        result.push_str(&text[cursor..]);
        Ok(self.record_change(index, result))
    }
    pub(crate) fn tag_change(
        &self,
        index: usize,
        code: i32,
        value: &str,
    ) -> Result<Change, String> {
        if value.len() > MAX_TEXT || value.chars().any(char::is_control) {
            return Err("Ongeldige DXF-tagwaarde".into());
        }
        let record = &self.records[index];
        let s = spans(&self.source[record.range.clone()], record.range.start)
            .find(|s| s.code == code)
            .ok_or("DXF-tag ontbreekt")?;
        Ok(Change {
            start: s.value.start,
            end: s.value.end,
            text: value.into(),
        })
    }
    pub(crate) fn seed(&self) -> Result<(u64, Change), String> {
        for record in &self.records {
            if record.section == NO_SECTION
                || self.source[self.sections[record.section].clone()].trim() != "HEADER"
            {
                continue;
            }
            let mut found = false;
            for s in spans(&self.source[record.range.clone()], record.range.start) {
                if s.code == 9 {
                    found = self.source[s.value.clone()].trim() == "$HANDSEED";
                } else if found && s.code == 5 {
                    let id = self.edits.get(&s.value.start).map_or_else(
                        || handle(&self.source[s.value.clone()]),
                        |e| handle(&e.replacement),
                    )?;
                    let next = self
                        .handles
                        .keys()
                        .copied()
                        .max()
                        .unwrap_or(0)
                        .checked_add(1)
                        .ok_or("Geen vrije DXF-handles")?
                        .max(id)
                        .max(1);
                    return Ok((
                        next,
                        Change {
                            start: s.value.start,
                            end: s.value.end,
                            text: String::new(),
                        },
                    ));
                }
            }
        }
        Err("DXF-sjabloon mist $HANDSEED".into())
    }
    pub(crate) fn section_end(&self, name: &str) -> Result<usize, String> {
        self.records()
            .find(|r| r.kind() == "ENDSEC" && r.section() == Some(name))
            .map(|r| self.records[r.index].range.start)
            .ok_or_else(|| format!("DXF mist sectie {name}"))
    }
    /// Apply a prepared edit batch without copying the original drawing. Equal
    /// insertion offsets concatenate; replacement ranges must be disjoint.
    pub(crate) fn apply(&mut self, changes: Vec<Change>) -> Result<(), String> {
        self.apply_with_insertions(changes, BTreeMap::new())
    }
    /// Replace prepared insertion payloads and append new edits as one atomic
    /// batch. Replacing a payload must not turn into another concatenation.
    pub(crate) fn apply_with_insertions(
        &mut self,
        changes: Vec<Change>,
        replacements: BTreeMap<usize, String>,
    ) -> Result<(), String> {
        let overwritten: HashSet<_> = replacements.keys().copied().collect();
        let mut planned: BTreeMap<usize, Edit> = BTreeMap::new();
        for c in changes {
            if c.start > c.end
                || c.end > self.source.len()
                || !self.source.is_char_boundary(c.start)
                || !self.source.is_char_boundary(c.end)
            {
                return Err("Ongeldig DXF-bewerkingsbereik".into());
            }
            if let Some(old) = planned.get_mut(&c.start) {
                if c.start != c.end || old.end != c.start {
                    return Err("Overlappende DXF-bewerkingen".into());
                }
                old.replacement.push_str(&c.text);
            } else {
                planned.insert(
                    c.start,
                    Edit {
                        end: c.end,
                        replacement: c.text,
                    },
                );
            }
        }
        for (at, text) in replacements {
            if self.edits.get(&at).is_none_or(|e| e.end != at) {
                return Err("Te vervangen DXF-invoeging ontbreekt".into());
            }
            if let Some(append) = planned.get_mut(&at) {
                if append.end != at {
                    return Err("Overlappende DXF-invoegingsvervanging".into());
                }
                let suffix = std::mem::replace(&mut append.replacement, text);
                append.replacement.push_str(&suffix);
            } else {
                planned.insert(
                    at,
                    Edit {
                        end: at,
                        replacement: text,
                    },
                );
            }
        }
        let mut previous_end = 0;
        let mut total = self.output_len as i64;
        for (&start, e) in &planned {
            if start < previous_end {
                return Err("Overlappende DXF-bewerkingen".into());
            }
            previous_end = e.end;
            if self
                .edits
                .range(..start)
                .next_back()
                .is_some_and(|(_, p)| p.end > start)
                || self
                    .edits
                    .range((std::ops::Bound::Excluded(start), std::ops::Bound::Unbounded))
                    .next()
                    .is_some_and(|(&n, _)| n < e.end)
            {
                return Err("Overlappende DXF-bewerkingen".into());
            }
            if let Some(old) = self.edits.get(&start) {
                if old.end != e.end {
                    return Err("Overlappende DXF-bewerkingen".into());
                }
                total += e.replacement.len() as i64
                    - if start == e.end && !overwritten.contains(&start) {
                        0
                    } else {
                        old.replacement.len() as i64
                    };
            } else {
                total += e.replacement.len() as i64 - (e.end - start) as i64;
            }
        }
        if !(0..=MAX_BYTES as i64).contains(&total) {
            return Err("Bewerkt DXF-sjabloon is te groot".into());
        }
        for (start, e) in planned {
            if start == e.end && !overwritten.contains(&start) {
                if let Some(old) = self.edits.get_mut(&start) {
                    old.replacement.push_str(&e.replacement);
                    continue;
                }
            }
            self.edits.insert(start, e);
        }
        self.output_len = total as usize;
        Ok(())
    }
    /// Patch only selected scalars in pending records (or the tag prefix before
    /// the first record). Unknown bytes and all unrelated records stay intact.
    pub(crate) fn patch_inserted_values(
        &self,
        at: usize,
        updates: &InsertedValues,
        cancel: &AtomicBool,
    ) -> Result<String, String> {
        let payload = match self.edits.get(&at) {
            Some(e) if e.end == at => e.replacement.as_str(),
            None => "",
            _ => return Err("DXF-invoeging overlapt een bronbewerking".into()),
        };
        let mut boundaries = vec![0];
        boundaries.extend(
            spans(payload, 0)
                .filter(|s| s.code == 0 && s.whole.start != 0)
                .map(|s| s.whole.start),
        );
        if spans(payload, 0).next().is_some_and(|s| s.code == 0) {
            boundaries.insert(0, 0);
        }
        boundaries.push(payload.len());
        let mut patches = Vec::new();
        let mut found = HashSet::new();
        for (i, range) in boundaries.windows(2).enumerate() {
            if i % 1024 == 0 {
                crate::input::check(cancel)?;
            }
            let start = range[0];
            let end = range[1];
            let is_record = start < end
                && spans(&payload[start..end], start)
                    .next()
                    .is_some_and(|s| s.code == 0);
            let key = if is_record {
                spans(&payload[start..end], start)
                    .take_while(|s| s.code != 100)
                    .find(|s| s.code == 5)
                    .map(|s| handle(&payload[s.value]))
                    .transpose()?
            } else {
                None
            };
            let Some(values) = updates.get(&key) else {
                continue;
            };
            found.insert(key);
            let record_tags = spans(&payload[start..end], start)
                .take_while(|s| s.code < 1000)
                .collect::<Vec<_>>();
            let insertion = spans(&payload[start..end], start)
                .find(|s| s.code >= 1000)
                .map_or(end, |s| s.whole.start);
            let mut missing = String::new();
            for (code, value) in values {
                if !matches!(*code, 62 | 370)
                    || value.len() > MAX_TEXT
                    || value.chars().any(char::is_control)
                {
                    return Err("Ongeldige ingevoegde laagwaarde".into());
                }
                let mut matches = record_tags.iter().filter(|s| s.code == *code);
                if let Some(tag) = matches.next() {
                    if matches.next().is_some() {
                        return Err("Dubbele ingevoegde laagwaarde".into());
                    }
                    if payload[tag.value.clone()].trim() != value {
                        patches.push((tag.value.clone(), value.clone()));
                    }
                } else {
                    missing.push_str(&format!("{code}{}{value}{}", self.newline, self.newline));
                }
            }
            if !missing.is_empty() {
                patches.push((insertion..insertion, missing));
            }
        }
        if updates.keys().any(|key| !found.contains(key)) {
            return Err("Ingevoegd laagrecord ontbreekt".into());
        }
        patches.sort_by_key(|(r, _)| r.start);
        let mut output = String::with_capacity(payload.len());
        let mut cursor = 0;
        for (r, text) in patches {
            if r.start < cursor {
                return Err("Overlappende ingevoegde laagwaarden".into());
            }
            output.push_str(&payload[cursor..r.start]);
            output.push_str(&text);
            cursor = r.end;
        }
        output.push_str(&payload[cursor..]);
        Ok(output)
    }
    pub(crate) fn has_insertion(&self, at: usize) -> bool {
        self.edits.get(&at).is_some_and(|e| e.end == at)
    }
    pub fn load(path: &Path, cancel: &AtomicBool) -> Result<Self, String> {
        let bytes = crate::input::read(path, MAX_BYTES as u64, "DXF-sjabloon", cancel)?;
        Self::from_bytes(bytes, cancel)
    }
    pub fn from_bytes(bytes: Vec<u8>, cancel: &AtomicBool) -> Result<Self, String> {
        crate::input::check(cancel)?;
        if bytes.len() > MAX_BYTES {
            return Err("DXF-sjabloon is te groot".into());
        }
        if bytes.starts_with(crate::dxf_tags::SIGNATURE) {
            return Err("Behoudende sjabloonbewerking ondersteunt nog geen binaire DXF".into());
        }
        let source = String::from_utf8(bytes)
            .map_err(|_| "DXF-sjabloon is geen geldige UTF-8".to_owned())?;
        // Reuse the same code, scalar and per-value limits as the overlay parser.
        for (i, tag) in crate::dxf_tags::ascii(&source).enumerate() {
            if i % 1024 == 0 {
                crate::input::check(cancel)?;
            }
            tag?;
        }
        let newline = if source.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let mut records: Vec<Record> = Vec::new();
        for (i, span) in spans(&source, 0).enumerate() {
            if i % 1024 == 0 {
                crate::input::check(cancel)?;
            }
            if span.code != 0 {
                if records.is_empty() && span.code != 999 {
                    return Err("DXF-sjabloon begint niet met een record".into());
                }
                continue;
            }
            if records.len() >= MAX_RECORDS {
                return Err("Te veel records in DXF-sjabloon".into());
            }
            if let Some(previous) = records.last_mut() {
                previous.range.end = span.whole.start;
            }
            records.push(Record {
                range: span.whole.start..source.len(),
                kind: span.value,
                section: NO_SECTION,
                handle: None,
            });
        }
        let mut sections = Vec::new();
        let mut current = None;
        let mut handles = HashMap::new();
        let mut version = None;
        let mut eof = false;
        for (index, record) in records.iter_mut().enumerate() {
            if index % 256 == 0 {
                crate::input::check(cancel)?;
            }
            if eof {
                return Err("DXF-sjabloon bevat records na EOF".into());
            }
            let kind = source[record.kind.clone()].trim();
            if kind == "SECTION" {
                if current.is_some() {
                    return Err("Geneste DXF-secties zijn ongeldig".into());
                }
                let name = spans(&source[record.range.clone()], record.range.start)
                    .find(|s| s.code == 2)
                    .ok_or("DXF-sectie mist een naam")?;
                current = Some(sections.len());
                sections.push(name.value);
            }
            record.section = current.unwrap_or(NO_SECTION);
            if kind == "ENDSEC" {
                if current.take().is_none() {
                    return Err("DXF ENDSEC zonder sectie".into());
                }
            } else if kind == "EOF" {
                if current.is_some() {
                    return Err("DXF mist ENDSEC voor EOF".into());
                }
                if spans(&source[record.range.clone()], record.range.start)
                    .any(|s| s.code != 0 && s.code != 999)
                {
                    return Err("DXF bevat gegevens na EOF".into());
                }
                eof = true;
            } else if current.is_none() {
                return Err("DXF-record staat buiten een sectie".into());
            }
            let header = current.is_some_and(|i| source[sections[i].clone()].trim() == "HEADER");
            let mut acadver = false;
            for (n, span) in spans(&source[record.range.clone()], record.range.start).enumerate() {
                if n % 1024 == 0 {
                    crate::input::check(cancel)?;
                }
                let value = &source[span.value.clone()];
                if header {
                    if span.code == 9 {
                        acadver = value.trim() == "$ACADVER";
                    } else if acadver && span.code == 1 {
                        version = Some(value.trim().to_owned());
                        acadver = false;
                    }
                } else if kind != "SECTION"
                    && matches!(span.code, 5 | 105)
                    && record.handle.is_none()
                {
                    let id = handle(value)?;
                    if id == 0 || handles.insert(id, index).is_some() {
                        return Err("Dubbele of lege DXF-recordhandle".into());
                    }
                    record.handle = Some(id);
                }
                // Following subclass data can contain arbitrary handle-valued fields.
                if !header && span.code == 100 {
                    break;
                }
            }
        }
        if current.is_some() || !eof {
            return Err("DXF-sjabloon is niet volledig afgesloten".into());
        }
        let version = version.ok_or("DXF-sjabloon mist $ACADVER")?;
        if !matches!(version.as_str(), "AC1021" | "AC1024" | "AC1027" | "AC1032") {
            return Err("Behoudende sjabloonbewerking vereist DXF R2007 of nieuwer".into());
        }
        crate::input::check(cancel)?;
        let output_len = source.len();
        Ok(Self {
            source,
            records,
            sections,
            handles,
            edits: BTreeMap::new(),
            newline,
            output_len,
            image_batch_added: false,
            added_layers: HashMap::new(),
            added_styles: HashMap::new(),
            added_blocks: HashMap::new(),
            profile_leader_promoted: false,
        })
    }
    pub fn records(&self) -> impl Iterator<Item = RecordView<'_>> {
        (0..self.records.len()).map(|index| RecordView {
            document: self,
            index,
        })
    }
    pub fn by_handle(&self, id: &str) -> Result<Option<RecordView<'_>>, String> {
        Ok(self.handles.get(&handle(id)?).map(|&index| RecordView {
            document: self,
            index,
        }))
    }
    /// Replace DXF-formatted TEXT/ATTRIB/ATTDEF/MTEXT, keeping all other tags.
    /// Use MTEXT escapes (e.g. \P); raw newline/control characters are rejected.
    pub fn replace_text(&mut self, id: &str, text: &str) -> Result<(), String> {
        if text.len() > MAX_TEXT || text.chars().any(char::is_control) {
            return Err("Ongeldige of te lange DXF-tekst".into());
        }
        let index = *self
            .handles
            .get(&handle(id)?)
            .ok_or("DXF-teksthandle ontbreekt")?;
        let record = &self.records[index];
        let kind = self.source[record.kind.clone()].trim();
        let mtext = kind == "MTEXT";
        if !mtext && !matches!(kind, "TEXT" | "ATTRIB" | "ATTDEF") {
            return Err("DXF-record is geen bewerkbare tekst".into());
        }
        let mut tags = Vec::new();
        for s in spans(&self.source[record.range.clone()], record.range.start) {
            if s.code == 101 || s.code >= 1000 {
                break;
            }
            if s.code == 1 || (mtext && s.code == 3) {
                tags.push(s);
            }
        }
        if tags.iter().filter(|s| s.code == 1).count() != 1 {
            return Err("DXF-tekst bevat geen eenduidige inhoud".into());
        }
        let mut replacement = String::new();
        if mtext {
            let mut remaining = text;
            while let Some((boundary, _)) = remaining.char_indices().nth(250) {
                replacement.push_str("  3");
                replacement.push_str(self.newline);
                replacement.push_str(&remaining[..boundary]);
                replacement.push_str(self.newline);
                remaining = &remaining[boundary..];
            }
            replacement.push_str("  1");
            replacement.push_str(self.newline);
            replacement.push_str(remaining);
            replacement.push_str(self.newline);
        } else {
            replacement.push_str("  1");
            replacement.push_str(self.newline);
            replacement.push_str(text);
            replacement.push_str(self.newline);
        }
        let first = tags[0].whole.start;
        let mut output_len = self.output_len as i64;
        for tag in &tags {
            let old = self
                .edits
                .get(&tag.whole.start)
                .map_or(tag.whole.len(), |e| e.replacement.len());
            let new = if tag.whole.start == first {
                replacement.len()
            } else {
                0
            };
            output_len += new as i64 - old as i64;
        }
        if !(0..=MAX_BYTES as i64).contains(&output_len) {
            return Err("Bewerkt DXF-sjabloon is te groot".into());
        }
        // Stage only after validation; repeated calls replace the same edits.
        for tag in tags {
            self.edits.insert(
                tag.whole.start,
                Edit {
                    end: tag.whole.end,
                    replacement: if tag.whole.start == first {
                        std::mem::take(&mut replacement)
                    } else {
                        String::new()
                    },
                },
            );
        }
        self.output_len = output_len as usize;
        Ok(())
    }
    pub fn write_to(&self, out: &mut impl Write, cancel: &AtomicBool) -> Result<(), String> {
        crate::input::check(cancel)?;
        let write = |out: &mut dyn Write, bytes: &[u8]| -> Result<(), String> {
            for chunk in bytes.chunks(64 * 1024) {
                crate::input::check(cancel)?;
                out.write_all(chunk).map_err(|e| e.to_string())?;
            }
            Ok(())
        };
        let mut position = 0;
        for (&start, edit) in &self.edits {
            write(out, &self.source.as_bytes()[position..start])?;
            write(out, edit.replacement.as_bytes())?;
            position = edit.end;
        }
        write(out, &self.source.as_bytes()[position..])?;
        crate::input::check(cancel)
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    fn encoded(doc: &Document) -> Vec<u8> {
        let mut bytes = Vec::new();
        doc.write_to(&mut bytes, &AtomicBool::new(false)).unwrap();
        bytes
    }
    #[test]
    fn overlapping_or_invalid_batches_are_rejected_before_any_mutation() {
        let source = include_bytes!("../tests/fixtures/templates/standard.dxf").to_vec();
        let mut doc = Document::from_bytes(source.clone(), &AtomicBool::new(false)).unwrap();
        let (_, mut seed) = doc.seed().unwrap();
        let start = seed.start;
        seed.text = "FFFFFF".into();
        assert!(doc
            .apply(vec![
                seed,
                Change {
                    start: start + 1,
                    end: start + 2,
                    text: "bad".into()
                }
            ])
            .is_err());
        assert_eq!(encoded(&doc), source);
        assert!(doc
            .apply(vec![Change {
                start: source.len() + 1,
                end: source.len() + 1,
                text: "bad".into()
            }])
            .is_err());
        assert_eq!(encoded(&doc), source);
    }
}
