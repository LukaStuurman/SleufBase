//! Local DXF font selection matching the original filename/family/fallback order.
//! Scan only SFNT name/OS2 tables; glyph data is loaded once on actual use.
use crate::{dxf_text_metrics::Font, input};
use rustybuzz::ttf_parser::{name, PlatformId};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
const FALLBACKS: [&str; 7] = [
    "ArialUni.ttf",
    "Arial Unicode.ttf",
    "Arial.ttf",
    "DejaVuSansCondensed.ttf",
    "DejaVuSans.ttf",
    "LiberationSans-Regular.ttf",
    "OpenSans-Regular.ttf",
];
#[derive(Debug)]
struct Entry {
    path: PathBuf,
    family: String,
    style: String,
    weight: u16,
    width: u16,
}
pub struct Catalog {
    entries: Vec<Entry>,
    names: HashMap<String, usize>,
    loaded: HashMap<usize, Font>,
    resolved: HashMap<(usize, bool), Option<usize>>,
    loaded_bytes: usize,
    metadata_bytes: usize,
}
fn key(value: &str) -> String {
    value.to_lowercase()
}
fn part(
    f: &mut File,
    file_len: u64,
    offset: u64,
    len: usize,
    used: &mut usize,
) -> Result<Vec<u8>, String> {
    if len > 2 * 1024 * 1024 || offset.checked_add(len as u64).is_none_or(|n| n > file_len) {
        return Err("Ongeldige lettertypetabel".into());
    }
    *used = used
        .checked_add(len)
        .ok_or("Lettertype-indexbudget overschreden")?;
    if *used > 64 * 1024 * 1024 {
        return Err("Lettertype-indexbudget overschreden".into());
    }
    f.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    let mut bytes = vec![0; len];
    f.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}
fn u16be(data: &[u8], i: usize) -> u16 {
    u16::from_be_bytes([data[i], data[i + 1]])
}
fn u32be(data: &[u8], i: usize) -> u32 {
    u32::from_be_bytes(data[i..i + 4].try_into().unwrap())
}
fn metadata(path: &Path, used: &mut usize) -> Result<Entry, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    // Index only small name/OS2 tables. The actual glyph-byte limit is checked
    // on use, so an indexed oversized font cannot silently become a fallback.
    let header = part(&mut file, len, 0, 16, used)?;
    let offset = if &header[..4] == b"ttcf" {
        u64::from(u32be(&header, 12))
    } else {
        0
    };
    let header = part(&mut file, len, offset, 12, used)?;
    if !matches!(&header[..4], b"\0\x01\0\0" | b"OTTO" | b"true") {
        return Err("Geen ondersteund SFNT-lettertype".into());
    }
    let count = usize::from(u16be(&header, 4));
    if count > 4096 {
        return Err("Te veel lettertypetabellen".into());
    }
    let directory = part(&mut file, len, offset + 12, count * 16, used)?;
    let mut names = None;
    let mut os2 = None;
    for entry in directory.as_chunks::<16>().0 {
        let range = (u64::from(u32be(entry, 8)), u32be(entry, 12) as usize);
        match &entry[..4] {
            b"name" => names = Some(range),
            b"OS/2" => os2 = Some(range),
            _ => {}
        }
    }
    let (offset, size) = names.ok_or("Lettertype mist namen")?;
    let bytes = part(&mut file, len, offset, size, used)?;
    let table = name::Table::parse(&bytes).ok_or("Ongeldige lettertypenamen")?;
    let (mut family, mut style) = (None, None);
    for n in table.names {
        if !matches!(n.name_id, 1 | 2) {
            continue;
        }
        let text = n.to_string().or_else(|| {
            (n.platform_id == PlatformId::Macintosh && n.encoding_id == 0)
                .then(|| encoding_rs::MACINTOSH.decode(n.name).0.into_owned())
        });
        if n.name_id == 1 {
            family = text;
        } else {
            style = text;
        }
        if family.is_some() && style.is_some() {
            break;
        }
    }
    let (mut weight, mut width) = (400, 5);
    if let Some((offset, size)) = os2 {
        if size >= 8 {
            let bytes = part(&mut file, len, offset, 8, used)?;
            weight = u16be(&bytes, 4);
            width = u16be(&bytes, 6);
        }
    }
    let style = style.unwrap_or_default();
    Ok(Entry {
        path: path.to_owned(),
        family: family.unwrap_or_default(),
        style: if style == "Book" {
            "Regular".into()
        } else {
            style
        },
        weight,
        width,
    })
}
impl Catalog {
    pub fn system(cancel: &AtomicBool) -> Result<Self, String> {
        Self::system_with_dirs(&[], cancel)
    }
    /// Explicit CAD support folders override matching system filenames.
    pub fn system_with_dirs(extra: &[PathBuf], cancel: &AtomicBool) -> Result<Self, String> {
        Self::validate_dirs(extra)?;
        #[cfg(windows)]
        let dirs = {
            let mut dirs = Vec::new();
            if let Some(home) = std::env::var_os("USERPROFILE") {
                dirs.push(PathBuf::from(home).join("AppData/Local/Microsoft/Windows/Fonts"));
            }
            dirs.push(
                PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into()))
                    .join("Fonts"),
            );
            dirs
        };
        #[cfg(not(windows))]
        let dirs = {
            let mut dirs = vec![
                PathBuf::from("/usr/share/fonts"),
                PathBuf::from("/usr/local/share/fonts"),
            ];
            if let Some(home) = std::env::var_os("HOME") {
                for folder in [".fonts", ".local/share/fonts", ".local/share/texmf/fonts"] {
                    dirs.push(PathBuf::from(&home).join(folder));
                }
            }
            dirs
        };
        let mut dirs = dirs;
        dirs.extend_from_slice(extra);
        Self::scan_dirs(&dirs, cancel)
    }
    pub fn validate_dirs(dirs: &[PathBuf]) -> Result<(), String> {
        if dirs.len() > 64
            || dirs
                .iter()
                .any(|p| p.as_os_str().is_empty() || p.as_os_str().len() > 32768)
        {
            return Err("Ongeldige of te veel lettertypemappen".into());
        }
        Ok(())
    }
    pub fn from_dirs(dirs: &[PathBuf], cancel: &AtomicBool) -> Result<Self, String> {
        Self::validate_dirs(dirs)?;
        Self::scan_dirs(dirs, cancel)
    }
    fn scan_dirs(dirs: &[PathBuf], cancel: &AtomicBool) -> Result<Self, String> {
        let mut result = Self {
            entries: Vec::new(),
            names: HashMap::new(),
            loaded: HashMap::new(),
            resolved: HashMap::new(),
            loaded_bytes: 0,
            metadata_bytes: 0,
        };
        let mut seen = HashSet::new();
        let mut visits = 0;
        for path in dirs {
            result.scan(path, 0, &mut seen, &mut visits, cancel)?;
        }
        input::check(cancel)?;
        Ok(result)
    }
    fn scan(
        &mut self,
        path: &Path,
        depth: usize,
        seen: &mut HashSet<PathBuf>,
        visits: &mut usize,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        input::check(cancel)?;
        if !path.is_dir() {
            return Ok(());
        }
        if depth > 32 {
            return Err("Te diep geneste lettertypemappen".into());
        }
        let canonical = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
        if !seen.insert(canonical) {
            return Ok(());
        }
        if seen.len() > 512 {
            return Err("Te veel lettertypemappen".into());
        }
        let entries = std::fs::read_dir(path).map_err(|e| e.to_string())?;
        for entry in entries {
            input::check(cancel)?;
            *visits += 1;
            if *visits > 50_000 {
                return Err("Te veel bestanden in lettertypemappen".into());
            }
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                self.scan(&path, depth + 1, seen, visits, cancel)?;
                continue;
            }
            let ext = key(path.extension().and_then(|s| s.to_str()).unwrap_or(""));
            let entry = match ext.as_str() {
                "ttf" | "ttc" | "otf" => match metadata(&path, &mut self.metadata_bytes) {
                    Ok(e) => e,
                    Err(e) => {
                        if self.metadata_bytes > 64 * 1024 * 1024 {
                            return Err(e);
                        }
                        continue;
                    }
                },
                "shx" | "shp" | "lff" => Entry {
                    family: key(path.file_stem().and_then(|s| s.to_str()).unwrap_or("")),
                    style: format!(".{ext}"),
                    weight: 400,
                    width: match ext.as_str() {
                        "shp" => 6,
                        "lff" => 7,
                        _ => 5,
                    },
                    path: path.clone(),
                },
                _ => continue,
            };
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let name = key(name);
            if let Some(&i) = self.names.get(&name) {
                self.entries[i] = entry;
            } else {
                if self.entries.len() >= 20_000 {
                    return Err("Te veel geïndexeerde lettertypen".into());
                }
                self.names.insert(name, self.entries.len());
                self.entries.push(entry);
            }
        }
        Ok(())
    }
    fn select(&self, request: &str) -> Option<usize> {
        let query = key(request);
        let ext = Path::new(request)
            .extension()
            .and_then(|s| s.to_str())
            .map(key)
            .unwrap_or_default();
        if ext.is_empty() {
            let family = match request {
                "serif" => "dejavu serif",
                "sans-serif" => "dejavu sans",
                "monospace" => "dejavu sans mono",
                _ => &query,
            };
            let candidates = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| key(&e.family).starts_with(family))
                .collect::<Vec<_>>();
            let has_shape = candidates
                .iter()
                .any(|(_, e)| key(&e.style).contains(".shx"));
            if let Some((i, _)) = candidates
                .into_iter()
                .filter(|(_, e)| !has_shape || key(&e.style).contains(".shx"))
                .min_by_key(|(_, e)| (e.weight.abs_diff(400), e.width.abs_diff(5)))
            {
                return Some(i);
            }
        }
        if matches!(ext.as_str(), "ttf" | "ttc" | "otf" | "shx" | "shp" | "lff") {
            if let Some(&i) = self.names.get(&query) {
                return Some(i);
            }
        }
        if matches!(ext.as_str(), "shx" | "shp") {
            if let Some(i) = ["txt.shx", "txt.shp", "iso.shx", "iso.shp"]
                .iter()
                .find_map(|n| self.names.get(*n).copied())
            {
                return Some(i);
            }
        }
        if ext == "lff" {
            // Correct the old manager's accidental SHX list in fallback_lff().
            if let Some(i) = ["standard.lff", "iso.lff", "simplex.lff"]
                .iter()
                .find_map(|n| self.names.get(*n).copied())
            {
                return Some(i);
            }
        }
        self.ttf_fallback()
    }
    fn ttf_fallback(&self) -> Option<usize> {
        FALLBACKS
            .iter()
            .find_map(|name| self.names.get(&key(name)).copied())
    }
    pub fn selected_path(&self, request: &str) -> Option<&Path> {
        if request == "*monospace" {
            None
        } else {
            let i = self.select(request)?;
            let exact = self.exact_lookup(request);
            self.resolved
                .get(&(i, exact))
                .copied()
                .unwrap_or(Some(i))
                .map(|i| self.entries[i].path.as_path())
        }
    }
    fn exact_lookup(&self, request: &str) -> bool {
        // A matched family recurses through its selected filename in make_font().
        Path::new(request).extension().is_none() || self.names.contains_key(&key(request))
    }
    pub fn source_paths(&self) -> impl Iterator<Item = &Path> {
        self.entries.iter().map(|e| e.path.as_path())
    }
    pub fn resolve(&mut self, request: &str, cancel: &AtomicBool) -> Result<Font, String> {
        input::check(cancel)?;
        if request == "*monospace" {
            return Ok(Font::monospace());
        }
        let Some(i) = self.select(request) else {
            return Ok(Font::monospace());
        };
        let exact = self.exact_lookup(request);
        self.resolved.remove(&(i, exact));
        let result = self.resolve_file(i, exact, cancel).map_err(String::from)?;
        input::check(cancel)?;
        self.resolved.insert((i, exact), result.1);
        Ok(result.0)
    }
    fn resolve_file(
        &mut self,
        i: usize,
        exact: bool,
        cancel: &AtomicBool,
    ) -> Result<(Font, Option<usize>), crate::font_error::Error> {
        use crate::font_error::{Error, Kind};
        Error::check(cancel)?;
        match self.load_file(i, cancel) {
            Ok(f) => Ok((f, Some(i))),
            Err(e) => {
                let shape = self.entries[i]
                    .path
                    .extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| {
                        s.eq_ignore_ascii_case("shx") || s.eq_ignore_ascii_case("shp")
                    });
                let stroke = shape
                    || self.entries[i]
                        .path
                        .extension()
                        .and_then(|s| s.to_str())
                        .is_some_and(|s| s.eq_ignore_ascii_case("lff"));
                let fallback = ((shape || !stroke) && e.kind() == Kind::Unsupported)
                    || (stroke && !exact && e.kind() == Kind::Unavailable);
                if fallback {
                    Error::check(cancel)?;
                    if let Some(target) = self.ttf_fallback() {
                        if target == i {
                            return Err(e);
                        }
                        return match self.load_file(target, cancel) {
                            Ok(f) => Ok((f, Some(target))),
                            Err(e) if e.kind() == Kind::Unavailable => {
                                Ok((Font::monospace(), None))
                            }
                            Err(e) => Err(e),
                        };
                    }
                    return Ok((Font::monospace(), None));
                }
                if e.kind() == Kind::Unavailable {
                    Ok((Font::monospace(), None))
                } else {
                    Err(e)
                }
            }
        }
    }
    fn load_file(
        &mut self,
        i: usize,
        cancel: &AtomicBool,
    ) -> Result<Font, crate::font_error::Error> {
        let entry = &self.entries[i];
        if let Some(font) = self.loaded.get(&i) {
            return Ok(font.clone());
        }
        let font = Font::load_catalog(&entry.path, cancel)?;
        let loaded_bytes = self
            .loaded_bytes
            .checked_add(font.source_bytes())
            .ok_or_else(|| crate::font_error::Error::limit("Lettertypebudget overschreden"))?;
        if loaded_bytes > 256 * 1024 * 1024 {
            return Err(crate::font_error::Error::limit(
                "Lettertypebudget overschreden",
            ));
        }
        self.loaded_bytes = loaded_bytes;
        self.loaded.insert(i, font.clone());
        Ok(font)
    }
}
