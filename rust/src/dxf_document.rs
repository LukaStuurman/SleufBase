//! Source-preserving modern ASCII DXF editing for template work.
//! Unknown records, sections, layouts, dictionaries and proxy data remain bytes.
//! This is deliberately separate from the lossy overlay/scene importer.
use std::{
    collections::{BTreeMap, HashMap},
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

pub struct Document {
    source: String,
    records: Vec<Record>,
    sections: Vec<Range<usize>>,
    handles: HashMap<u64, usize>,
    edits: BTreeMap<usize, Edit>,
    newline: &'static str,
    output_len: usize,
}
#[derive(Clone, Copy)]
pub struct RecordView<'a> {
    document: &'a Document,
    index: usize,
}
impl<'a> RecordView<'a> {
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
