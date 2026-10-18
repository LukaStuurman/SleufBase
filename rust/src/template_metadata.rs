//! Per-export title values. Only attributes attached to selected paper layouts
//! are edited; unused sheets, block definitions and later revisions stay intact.
use crate::{
    dxf_document::{Change, Document, RecordView},
    python_values::{casefold, trim, uppercase},
};
use std::{collections::HashSet, fmt, sync::atomic::AtomicBool};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    year: u16,
    month: u16,
    day: u16,
}
impl Date {
    pub fn new(year: u16, month: u16, day: u16) -> Result<Self, String> {
        let leap =
            year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        let days = match month {
            4 | 6 | 9 | 11 => 30,
            2 => {
                if leap {
                    29
                } else {
                    28
                }
            }
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            _ => 0,
        };
        if year == 0 || year > 9999 || day == 0 || day > days {
            return Err("Ongeldige exportdatum".into());
        }
        Ok(Self { year, month, day })
    }
    /// Windows' local calendar, including its configured timezone and DST.
    /// Capture once per export rather than once per sheet.
    pub fn today() -> Result<Self, String> {
        #[cfg(windows)]
        {
            let mut time =
                std::mem::MaybeUninit::<windows_sys::Win32::Foundation::SYSTEMTIME>::uninit();
            // GetLocalTime initializes every SYSTEMTIME field at this valid pointer.
            let time = unsafe {
                windows_sys::Win32::System::SystemInformation::GetLocalTime(time.as_mut_ptr());
                time.assume_init()
            };
            Self::new(time.wYear, time.wMonth, time.wDay)
        }
        #[cfg(not(windows))]
        {
            Err("Automatische lokale exportdatum is beschikbaar in de Windows-preview".into())
        }
    }
}
impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}-{:02}-{:04}", self.day, self.month, self.year)
    }
}
pub struct Values {
    pub date: Date,
    /// None preserves an existing project name; Some("") deliberately clears it.
    pub project: Option<String>,
    pub drawn_by: String,
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
fn text(value: &str) -> Result<&str, String> {
    let value = trim(value);
    if value.len() > 4096 || value.chars().any(char::is_control) {
        return Err("Gebruik titelbloktekst op één regel van maximaal 4096 UTF-8-bytes".into());
    }
    Ok(value)
}
fn stage(changes: &mut Vec<Change>, bytes: &mut usize, change: Change) -> Result<(), String> {
    *bytes += change.text.len() + std::mem::size_of::<Change>();
    if *bytes > 32 * 1024 * 1024 {
        return Err("Te veel titelblokwijzigingen in één export".into());
    }
    changes.push(change);
    Ok(())
}
pub fn update(
    document: &mut Document,
    layouts: &[String],
    values: &Values,
    cancel: &AtomicBool,
) -> Result<usize, String> {
    crate::input::check(cancel)?;
    let project = values.project.as_deref().map(text).transpose()?;
    let drawn_by = text(&values.drawn_by)?;
    let date = values.date.to_string();
    let names: HashSet<_> = layouts
        .iter()
        .map(|v| casefold(v))
        .filter(|v| v != "model")
        .collect();
    let mut ids = HashSet::new();
    for r in document.records() {
        if r.index().is_multiple_of(1024) {
            crate::input::check(cancel)?;
        }
        if r.kind() == "LAYOUT"
            && r.tags()
                .filter(|(c, _)| *c == 1)
                .last()
                .is_some_and(|(_, v)| names.contains(&casefold(v)))
        {
            if let Some(id) = r.handle() {
                ids.insert(id);
            }
        }
    }
    let mut spaces = HashSet::new();
    for r in document.records() {
        if r.index().is_multiple_of(1024) {
            crate::input::check(cancel)?;
        }
        if r.kind() == "BLOCK_RECORD"
            && r.first(340)
                .and_then(|v| u64::from_str_radix(v.trim(), 16).ok())
                .is_some_and(|id| ids.contains(&id))
        {
            if let Some(id) = r.handle() {
                spaces.insert(id);
            }
        }
    }
    let mut changes = Vec::new();
    let mut bytes = 0;
    let mut records = document.records().peekable();
    while let Some(insert) = records.next() {
        if insert.index().is_multiple_of(1024) {
            crate::input::check(cancel)?;
        }
        if insert.kind() != "INSERT"
            || !owner(insert).is_some_and(|id| spaces.contains(&id))
            || !matches!(insert.section(), Some("BLOCKS" | "ENTITIES"))
        {
            continue;
        }
        let mut attributes = Vec::new();
        while records.peek().is_some_and(|r| r.kind() == "ATTRIB") {
            let r = records.next().unwrap();
            if r.index().is_multiple_of(1024) {
                crate::input::check(cancel)?;
            }
            let tag = trim(r.first(2).unwrap_or(""));
            // None of the recognized tags can exceed this length, even before
            // Unicode uppercase expansion. Avoid allocating for arbitrary tags.
            if tag.len() > 64 {
                continue;
            }
            if attributes.len() >= 10_000 {
                return Err("Te veel attributen in één titelblok".into());
            }
            attributes.push((r, uppercase(tag)));
        }
        for (numbered, plain, value) in [
            ("DATUM1", "DATUM", date.as_str()),
            ("OMSCHRIJVING1", "OMSCHRIJVING", "Proefsleuven"),
            ("TEKENAAR1", "TEKENAAR", drawn_by),
        ] {
            let numbered_exists = attributes.iter().any(|(_, tag)| tag == numbered);
            let wanted = if numbered_exists { numbered } else { plain };
            for (r, _) in attributes
                .iter()
                .filter(|(_, tag)| tag == wanted)
                .take(if numbered_exists { usize::MAX } else { 1 })
            {
                stage(
                    &mut changes,
                    &mut bytes,
                    document.tag_change(r.index(), 1, value)?,
                )?;
            }
        }
        if let Some(project) = project {
            for (r, _) in attributes.iter().filter(|(_, tag)| {
                matches!(
                    tag.as_str(),
                    "GEM-PROJECT" | "PROJECTOMSCHRIJVING" | "PROJECTNAAM"
                )
            }) {
                stage(
                    &mut changes,
                    &mut bytes,
                    document.tag_change(r.index(), 1, project)?,
                )?;
            }
        }
    }
    crate::input::check(cancel)?;
    drop(records);
    let count = changes.len();
    document.apply(changes)?;
    Ok(count)
}
