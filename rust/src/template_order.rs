//! Persistent template order and labels. Paths identify sources, not mutable UI
//! indices. Excluded rasters consume no slot; explicit empty rows do consume one.
use crate::{geotiff::GeoTiffInfo, python_values};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Row {
    pub raster: Option<PathBuf>,
    pub included: bool,
    pub number: String,
    pub variant: String,
    pub scale: Option<i64>,
    pub dataset: Option<PathBuf>,
    pub without_profile: bool,
    pub comments: String,
    pub address: Option<String>,
    pub metadata: serde_json::Value,
}
impl Default for Row {
    fn default() -> Self {
        Self {
            raster: None,
            included: true,
            number: String::new(),
            variant: String::new(),
            scale: None,
            dataset: None,
            without_profile: false,
            comments: String::new(),
            address: None,
            metadata: serde_json::Value::Null,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct State {
    #[serde(default="crate::cadastral_overview::template_defaults")]
    pub overview_settings: crate::cadastral_overview::Settings,
    #[serde(skip_serializing_if="crate::template_branding::Logos::is_empty")]
    pub logos: crate::template_branding::Logos,
    pub rows: Vec<Row>,
    pub project: String,
    pub drawn_by: String,
    pub font_directories: Vec<PathBuf>,
    pub template: Option<PathBuf>,
    pub profiles: bool,
    pub paired: bool,
    pub reverse: bool,
    pub aerial: bool,
    pub auto_bgt: bool,
    pub show_reference: bool,
    pub avoid_collisions: bool,
    pub marker_diameter: f64,
    pub use_custom_maaiveld: bool,
    pub clip_markers: bool,
    pub diameter_prefix: String,
    pub label_gap: f64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            overview_settings: crate::cadastral_overview::template_defaults(),
            rows: Vec::new(),
            logos: Default::default(),
            project: String::new(),
            drawn_by: String::new(),
            font_directories: Vec::new(),
            template: None,
            profiles: true,
            paired: true,
            reverse: false,
            aerial: true,
            auto_bgt: true,
            show_reference: true,
            avoid_collisions: true,
            marker_diameter: 0.02,
            use_custom_maaiveld: true,
            clip_markers: false,
            diameter_prefix: "Ø".into(),
            label_gap: 18.,
        }
    }
}
pub fn number(value: &str) -> Option<String> {
    let text = python_values::uppercase(python_values::trim(value));
    let text = text
        .strip_prefix("PS")
        .map(python_values::trim)
        .unwrap_or(&text);
    if text.is_empty() {
        return None;
    }
    let digits = text
        .chars()
        .map(crate::object_layout::decimal)
        .collect::<Option<Vec<_>>>()?;
    let value = digits
        .into_iter()
        .map(|n| char::from(b'0' + n as u8))
        .collect::<String>();
    let trimmed = value.trim_start_matches('0');
    Some(if trimmed.is_empty() {
        "0".into()
    } else {
        trimmed.into()
    })
}
pub fn variant(value: &serde_json::Value) -> String {
    if let Some(s) = value.as_str() {
        let s = python_values::uppercase(python_values::trim(s));
        match s.to_lowercase().as_str() {
            "" | "0" | "false" | "nee" | "no" | "off" | "geen" => return String::new(),
            "1" | "true" | "ja" | "yes" | "on" | "auto" | "automatisch" => return "AUTO".into(),
            _ => {}
        }
        if s.bytes().all(|c| c.is_ascii_uppercase()) {
            return s;
        }
    }
    if python_values::truth(value) {
        "AUTO".into()
    } else {
        String::new()
    }
}
pub fn suffix(mut n: usize) -> String {
    n = n.max(1);
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push(b'A' + (n % 26) as u8);
        n /= 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}
fn first_digits(text: &str) -> Option<String> {
    let mut found = String::new();
    for c in text.chars() {
        if let Some(n) = crate::object_layout::decimal(c) {
            found.push(char::from(b'0' + n as u8));
        } else if !found.is_empty() {
            break;
        }
    }
    number(&found)
}
pub fn default_number(path: &std::path::Path, index: usize) -> String {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let chars = stem.chars().collect::<Vec<_>>();
    for i in 0..chars.len().saturating_sub(1) {
        if matches!(chars[i], 'p' | 'P')
            && matches!(chars[i + 1], 's' | 'S' | 'ſ')
            && (i == 0 || !ascii_case_word(chars[i - 1]))
        {
            let tail = chars[i + 2..].iter().collect::<String>();
            let tail = tail.trim_start_matches(|c: char| {
                c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}' | '.' | '_' | '-')
            });
            if tail
                .chars()
                .next()
                .and_then(crate::object_layout::decimal)
                .is_some()
            {
                if let Some(n) = first_digits(tail) {
                    return n;
                }
            }
        }
    }
    first_digits(&stem)
        .or_else(|| first_digits(&path.to_string_lossy()))
        .unwrap_or_else(|| index.to_string())
}
pub fn default_variant(path: &std::path::Path) -> String {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let chars = stem.chars().collect::<Vec<_>>();
    for i in 0..chars.len().saturating_sub(1) {
        if matches!(chars[i], 'p' | 'P')
            && matches!(chars[i + 1], 's' | 'S' | 'ſ')
            && (i == 0 || !ascii_case_word(chars[i - 1]))
        {
            let mut at = i + 2;
            while chars.get(at).is_some_and(|c| {
                c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}' | '.' | '_' | '-')
            }) {
                at += 1;
            }
            let digits = at;
            while chars
                .get(at)
                .and_then(|c| crate::object_layout::decimal(*c))
                .is_some()
            {
                at += 1;
            }
            if at == digits {
                continue;
            }
            let start = at;
            while at - start < 2
                && chars
                    .get(at)
                    .is_some_and(|c| c.is_ascii_alphabetic() || matches!(c, 'İ' | 'ı' | 'ſ' | 'K'))
            {
                at += 1;
            }
            if at > start
                && chars
                    .get(at)
                    .is_none_or(|c| !crate::object_layout::word(*c))
            {
                return variant(&serde_json::Value::String(
                    chars[start..at].iter().collect(),
                ));
            }
        }
    }
    String::new()
}
fn ascii_case_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, 'İ' | 'ı' | 'ſ' | 'K')
}
impl State {
    pub fn sync(&mut self, rasters: &[GeoTiffInfo]) -> Result<(), String> {
        let paths = rasters
            .iter()
            .map(|r| r.path.as_path())
            .collect::<HashSet<_>>();
        let mut existing = self
            .rows
            .iter()
            .filter_map(|r| r.raster.as_deref())
            .filter(|p| paths.contains(p))
            .collect::<HashSet<_>>();
        let kept = self
            .rows
            .iter()
            .filter(|r| r.raster.as_deref().is_none_or(|p| paths.contains(p)))
            .count();
        let added = rasters
            .iter()
            .filter(|r| existing.insert(r.path.as_path()))
            .count();
        if kept + added > 5000 {
            return Err("Maximaal 5000 bronnen en lege vakken per sjabloonexport".into());
        }
        self.rows
            .retain(|r| r.raster.as_deref().is_none_or(|p| paths.contains(p)));
        let mut seen = self
            .rows
            .iter()
            .filter_map(|r| r.raster.clone())
            .collect::<HashSet<_>>();
        for r in rasters {
            if seen.insert(r.path.clone()) {
                self.rows.push(Row {
                    raster: Some(r.path.clone()),
                    number: default_number(&r.path, self.rows.len() + 1),
                    variant: default_variant(&r.path),
                    ..Default::default()
                });
            }
        }
        Ok(())
    }
    pub fn preview_labels(&self) -> (Vec<Option<String>>, Vec<usize>) {
        let mut variants = HashMap::<String, usize>::new();
        let mut invalid = Vec::new();
        let labels = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                if r.raster.is_none() || !r.included {
                    return None;
                }
                let Some(n) = number(&r.number) else {
                    invalid.push(i);
                    return None;
                };
                let mut v = variant(&serde_json::Value::String(r.variant.clone()));
                if v == "AUTO" {
                    let counter = variants.entry(n.clone()).or_insert(1);
                    *counter += 1;
                    v = suffix(*counter);
                }
                Some(format!("PS{n}{v}"))
            })
            .collect();
        (labels, invalid)
    }
    pub fn labels(&self) -> Result<Vec<Option<String>>, String> {
        let (labels, invalid) = self.preview_labels();
        if let Some(i) = invalid.first() {
            return Err(format!("Vul een geldig PS-nummer in bij regel {}", i + 1));
        }
        Ok(labels)
    }
    pub fn confirmed(&self) -> Result<Self, String> {
        self.validate()?;
        self.labels()?;
        let mut state = self.clone();
        state.project = python_values::trim(&state.project).into();
        state.drawn_by = python_values::trim(&state.drawn_by).into();
        for row in &mut state.rows {
            row.number = if row.raster.is_some() {
                number(&row.number).unwrap_or_default()
            } else {
                String::new()
            };
            row.variant = if row.raster.is_some() {
                variant(&serde_json::Value::String(row.variant.clone()))
            } else {
                String::new()
            };
        }
        Ok(state)
    }
    pub fn validate(&self) -> Result<(), String> {
        crate::dxf_fonts::Catalog::validate_dirs(&self.font_directories)?;
        if self.diameter_prefix.len()>32768 {return Err("Diametertekst is te lang".into());}
        if self.rows.len() > 5000 {
            return Err("Maximaal 5000 sjabloonvakken".into());
        }
        for text in [&self.project, &self.drawn_by] {
            let text = python_values::trim(text);
            if text.len() > 4096 || text.chars().any(char::is_control) {
                return Err(
                    "Gebruik projectnaam en tekenaar op één regel van maximaal 4096 UTF-8-bytes"
                        .into(),
                );
            }
        }
        if !self.marker_diameter.is_finite()
            || !(0.005..=1.).contains(&self.marker_diameter)
            || !self.label_gap.is_finite()
            || !(0. ..=100.).contains(&self.label_gap)
        {
            return Err("Ongeldige markerdiameter of labelafstand".into());
        }
        for r in &self.rows {
            if r.number.len() > 4096
                || r.variant.len() > 4096
                || r.comments.len() > 65_536
                || r.address.as_ref().is_some_and(|s| s.len() > 4096)
                || r.scale.is_some_and(|s| !(1..=1_000_000).contains(&s))
            {
                return Err("Ongeldige vaktekst of profielschaal".into());
            }
        }
        Ok(())
    }
    pub fn move_selected(&mut self, selection: &mut [bool], down: bool) {
        if selection.len() != self.rows.len() {
            return;
        }
        let indices: Box<dyn Iterator<Item = usize>> = if down {
            Box::new((0..self.rows.len().saturating_sub(1)).rev())
        } else {
            Box::new(1..self.rows.len())
        };
        for i in indices {
            let j = if down { i + 1 } else { i - 1 };
            if selection[i] && !selection[j] {
                self.rows.swap(i, j);
                selection.swap(i, j);
            }
        }
    }
}
