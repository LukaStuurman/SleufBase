//! CAD appearance and source metadata shared by import, project storage and UI.
use crate::dxf_tags::Group;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod palette {
    include!("cad_palette.rs");
}

pub fn aci_rgb(index: i32) -> [u8; 3] {
    let index = index.unsigned_abs().clamp(1, 255) as usize;
    let rgb = palette::ACI[index];
    [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]
}

fn true_rgb(value: u32) -> [u8; 3] {
    [(value >> 16) as u8, (value >> 8) as u8, value as u8]
}
fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub color_aci: i32,
    #[serde(default)]
    pub true_color: Option<u32>,
    #[serde(default = "yes")]
    pub visible: bool,
}
impl Default for Layer {
    fn default() -> Self {
        Self {
            color_aci: 1,
            true_color: None,
            visible: true,
        }
    }
}
impl Layer {
    pub fn validate(&self) -> Result<(), String> {
        if self.true_color.is_some_and(|v| v > 0xffffff) {
            return Err("Ongeldige CAD-laagkleur".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Tag {
    pub code: i32,
    pub value: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Properties {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<Box<crate::cad_geometry::Placement>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve: Option<Box<crate::cad_geometry::CurvePath>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spline: Option<Box<crate::spline::Spline>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh: Option<std::sync::Arc<crate::cad_mesh::Mesh>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_breaks: Vec<usize>,
    #[serde(default)]
    pub true_color: Option<u32>,
    #[serde(default)]
    pub line_type: Option<String>,
    #[serde(default)]
    pub line_weight: Option<i32>,
    #[serde(default)]
    pub invisible: bool,
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
    #[serde(default)]
    pub xdata: BTreeMap<String, Vec<Tag>>,
}
impl Properties {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(placement) = &self.placement {
            placement.validate()?;
        }
        if let Some(curve) = &self.curve {
            curve.validate()?;
        }
        if let Some(spline) = &self.spline {
            spline.validate()?;
        }
        if let Some(mesh) = &self.mesh {
            mesh.validate()?;
        }
        if usize::from(self.spline.is_some())
            + usize::from(self.curve.is_some())
            + usize::from(self.mesh.is_some())
            > 1
        {
            return Err("CAD-object bevat meerdere geometriebeschrijvingen".into());
        }
        if self.path_breaks.len() > 1_000_000
            || self.path_breaks.first() == Some(&0)
            || self.path_breaks.windows(2).any(|p| p[0] >= p[1])
        {
            return Err("Ongeldige CAD-padonderbrekingen".into());
        }
        if self.true_color.is_some_and(|v| v > 0xffffff) {
            return Err("Ongeldige CAD-objectkleur".into());
        }
        for tags in self.xdata.values() {
            for tag in tags {
                if !(1000..=1071).contains(&tag.code) || tag.code == 1001 {
                    return Err("Ongeldige opgeslagen CAD-XDATA-groepcode".into());
                }
                if tag.code == 1004
                    && (tag.value.len() % 2 != 0
                        || !tag.value.bytes().all(|b| b.is_ascii_hexdigit()))
                {
                    return Err("Ongeldige opgeslagen CAD-XDATA-bytes".into());
                }
                if matches!(tag.code, 1010..=1059)
                    && !tag.value.trim().parse::<f64>().is_ok_and(|v| v.is_finite())
                {
                    return Err("Ongeldig opgeslagen CAD-XDATA-getal".into());
                }
                if matches!(tag.code, 1060..=1071) && tag.value.trim().parse::<i64>().is_err() {
                    return Err("Ongeldige opgeslagen CAD-XDATA-integer".into());
                }
            }
        }
        Ok(())
    }
    pub fn is_empty(&self) -> bool {
        self.placement.is_none()
            && self.curve.is_none()
            && self.spline.is_none()
            && self.mesh.is_none()
            && self.path_breaks.is_empty()
            && self.true_color.is_none()
            && self.line_type.is_none()
            && self.line_weight.is_none()
            && !self.invisible
            && self.attributes.is_empty()
            && self.xdata.is_empty()
    }
    pub fn validate_geometry(&self, points: usize) -> Result<(), String> {
        self.validate()?;
        if self
            .path_breaks
            .last()
            .is_some_and(|&index| index >= points)
        {
            return Err("CAD-padonderbreking buiten de geometrie".into());
        }
        Ok(())
    }
}

pub(crate) fn properties(groups: &[Group]) -> Result<Option<Box<Properties>>, String> {
    let field = |code| {
        groups
            .iter()
            .find(|g| g.code == code)
            .map(|g| g.value.as_ref())
    };
    let mut props = Properties {
        true_color: field(420)
            .map(|v| {
                v.trim()
                    .parse()
                    .map_err(|_| "Ongeldige CAD true-color".to_owned())
            })
            .transpose()?,
        line_type: field(6).map(str::to_owned),
        line_weight: field(370).and_then(|v| v.trim().parse().ok()),
        invisible: field(60).is_some_and(|v| v.trim() == "1"),
        ..Default::default()
    };
    let mut app = None;
    for group in groups {
        if group.code == 1001 {
            app = Some(group.value.to_string());
        } else if group.code >= 1000 {
            if let Some(name) = &app {
                let value = match group.code {
                    1004 => group.value.trim().to_ascii_uppercase(),
                    1010..=1059 => group
                        .value
                        .trim()
                        .parse::<f64>()
                        .map_err(|_| "Ongeldige CAD XDATA-float")?
                        .to_string(),
                    1060..=1071 => group
                        .value
                        .trim()
                        .parse::<i64>()
                        .map_err(|_| "Ongeldige CAD XDATA-integer")?
                        .to_string(),
                    _ => group.value.to_string(),
                };
                props.xdata.entry(name.clone()).or_default().push(Tag {
                    code: group.code,
                    value,
                });
            }
        }
    }
    props.validate()?;
    Ok((!props.is_empty()).then(|| Box::new(props)))
}

/// Match SleufBase's ACI contrast rule; explicit true-color retains white/black.
/// A true-color on the layer is honored as a correction to the old Python reader.
pub fn display_rgb(aci: i32, props: Option<&Properties>, layer: Option<&Layer>) -> [u8; 3] {
    if let Some(rgb) = props.and_then(|p| p.true_color) {
        return true_rgb(rgb);
    }
    let aci = if aci == 0 || aci == 256 {
        if let Some(rgb) = layer.and_then(|l| l.true_color) {
            return true_rgb(rgb);
        }
        layer.map_or(1, |l| l.color_aci)
    } else {
        aci
    };
    let rgb = aci_rgb(aci.clamp(1, 255));
    if rgb == [255, 255, 255] {
        [220, 20, 60]
    } else {
        rgb
    }
}

fn number(value: &str) -> String {
    let Ok(value) = value.trim().parse::<f64>() else {
        return value.to_owned();
    };
    let raw = format!("{value:?}");
    if let Some((base, exponent)) = raw.split_once('e') {
        if let Ok(exponent) = exponent.parse::<i32>() {
            return format!(
                "{base}e{}{:02}",
                if exponent < 0 { '-' } else { '+' },
                exponent.unsigned_abs()
            );
        }
    }
    raw
}
fn binary(value: &str) -> String {
    let bytes = value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .filter_map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|s| u8::from_str_radix(s, 16).ok())
        })
        .collect::<Vec<_>>();
    let quote = if bytes.contains(&b'\'') && !bytes.contains(&b'"') {
        b'"'
    } else {
        b'\''
    };
    let mut out = String::from("b");
    out.push(quote as char);
    for byte in bytes {
        match byte {
            b'\t' => out.push_str("\\t"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\\' => out.push_str("\\\\"),
            b if b == quote => {
                out.push('\\');
                out.push(b as char);
            }
            32..=126 => out.push(byte as char),
            _ => {
                use std::fmt::Write;
                let _ = write!(out, "\\x{byte:02x}");
            }
        }
    }
    out.push(quote as char);
    out
}

pub fn xdata_text(tags: &[Tag]) -> String {
    let mut values = Vec::new();
    let mut index = 0;
    while let Some(tag) = tags.get(index) {
        if (1010..=1013).contains(&tag.code)
            && tags.get(index + 1).is_some_and(|t| t.code == tag.code + 10)
            && tags.get(index + 2).is_some_and(|t| t.code == tag.code + 20)
        {
            values.push(format!(
                "({}, {}, {})",
                number(&tag.value),
                number(&tags[index + 1].value),
                number(&tags[index + 2].value)
            ));
            index += 3;
            continue;
        }
        values.push(match tag.code {
            1004 => binary(&tag.value),
            1010..=1059 => number(&tag.value),
            1060..=1071 => tag
                .value
                .trim()
                .parse::<i64>()
                .map_or_else(|_| tag.value.clone(), |v| v.to_string()),
            _ => tag.value.clone(),
        });
        index += 1;
    }
    values.join(", ")
}
