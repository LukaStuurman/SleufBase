//! Font-dependent DXF TEXT/ATTRIB extents for the original fast-bbox contract.
//! Uses unkerned advances (not UI shaping), A/x/p measurements, DXF alignment,
//! shear, mirror flags and OCS. Glyph data stays local; no font redistribution.
use crate::cad_geometry::{self as geo, Affine, V3};
use rustybuzz::ttf_parser::{Face, GlyphId, OutlineBuilder};
use std::{
    collections::HashMap,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct Measurements {
    pub baseline: f64,
    pub cap_height: f64,
    pub x_height: f64,
    pub descender_height: f64,
}
impl Measurements {
    pub(crate) fn scale(self, factor: f64) -> Self {
        Self {
            baseline: self.baseline * factor,
            cap_height: self.cap_height * factor,
            x_height: self.x_height * factor,
            descender_height: self.descender_height * factor,
        }
    }
}
#[derive(Clone)]
struct TrueType {
    data: Arc<Vec<u8>>,
    measurements: Measurements,
    advances: HashMap<char, u16>,
}
struct ControlY {
    min: f64,
    max: f64,
}
impl ControlY {
    fn add(&mut self, y: f64) {
        self.min = self.min.min(y);
        self.max = self.max.max(y);
    }
}
impl OutlineBuilder for ControlY {
    fn move_to(&mut self, _x: f32, y: f32) {
        self.add(f64::from(y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.move_to(x, y);
    }
    fn quad_to(&mut self, _x: f32, y: f32, x1: f32, y1: f32) {
        // GlyphPath retains quadratic controls for font measurements. This
        // differs from geometric paths used by the drawing fast-bbox code.
        self.add(f64::from(y));
        self.move_to(x1, y1);
    }
    fn curve_to(&mut self, _x1: f32, y1: f32, _x2: f32, y2: f32, x: f32, y: f32) {
        self.add(f64::from(y1));
        self.add(f64::from(y2));
        self.move_to(x, y);
    }
    fn close(&mut self) {}
}
#[derive(Clone)]
pub struct Font {
    ttf: Option<TrueType>,
    stroke: Option<crate::stroke_font::Font>,
}
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Text {
    pub content: String,
    pub insert: V3,
    pub align_point: Option<V3>,
    pub height: f64,
    pub width: f64,
    pub rotation: f64,
    pub oblique: f64,
    pub halign: u8,
    pub valign: u8,
    pub generation: u8,
    pub normal: V3,
}
impl Default for Text {
    fn default() -> Self {
        Self {
            content: String::new(),
            insert: [0.; 3],
            align_point: None,
            height: 1.,
            width: 1.,
            rotation: 0.,
            oblique: 0.,
            halign: 0,
            valign: 0,
            generation: 0,
            normal: [0., 0., 1.],
        }
    }
}
#[derive(Debug, serde::Serialize)]
pub struct Extent {
    pub width: f64,
    pub measurements: Measurements,
    pub corners: Option<[V3; 4]>,
}
impl Font {
    pub fn source_bytes(&self) -> usize {
        self.ttf.as_ref().map_or(0, |f| f.data.len())
            + self.stroke.as_ref().map_or(0, |f| f.source_bytes())
    }
    pub fn monospace() -> Self {
        Self {
            ttf: None,
            stroke: None,
        }
    }
    pub fn load(path: &Path, cancel: &AtomicBool) -> Result<Self, String> {
        let data = crate::input::read(path, 64 * 1024 * 1024, "DXF-lettertype", cancel)?;
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ext.eq_ignore_ascii_case("shp") || ext.eq_ignore_ascii_case("shx") {
            let program = crate::stroke_font::Program::parse(&data, ext, cancel)?;
            return Ok(Self {
                ttf: None,
                stroke: Some(crate::stroke_font::Font::new(program, cancel)?),
            });
        }
        let face = Face::parse(&data, 0).map_err(|e| format!("Ongeldig DXF-lettertype: {e:?}"))?;
        let bounds = |ch| {
            let mut pen = ControlY {
                min: f64::INFINITY,
                max: f64::NEG_INFINITY,
            };
            face.outline_glyph(face.glyph_index(ch).unwrap_or(GlyphId(0)), &mut pen)
                .ok_or("DXF-lettertype mist meetbare A/x/p-glyphs")?;
            Ok::<_, String>((pen.min, pen.max))
        };
        let x = bounds('x')?;
        let a = bounds('A')?;
        let p = bounds('p')?;
        let baseline = x.0;
        let measurements = Measurements {
            baseline,
            cap_height: a.1 - baseline,
            x_height: x.1 - baseline,
            descender_height: baseline - p.0,
        };
        if measurements.cap_height <= 0. {
            return Err("DXF-lettertype heeft geen geldige kapitaalhoogte".into());
        }
        crate::input::check(cancel)?;
        Ok(Self {
            stroke: None,
            ttf: Some(TrueType {
                data: Arc::new(data),
                measurements,
                advances: HashMap::new(),
            }),
        })
    }
    fn measure(
        &mut self,
        text: &str,
        height: f64,
        width: f64,
        cancel: &AtomicBool,
    ) -> Result<(f64, Measurements), String> {
        if let Some(stroke) = &mut self.stroke {
            return stroke.measure(text, height, width, cancel);
        }
        let Some(ttf) = &mut self.ttf else {
            return Ok((
                text.chars().count() as f64 * height * width.abs(),
                Measurements {
                    baseline: 0.,
                    cap_height: height,
                    x_height: height * 0.666,
                    descender_height: height * 0.333,
                },
            ));
        };
        let face =
            Face::parse(&ttf.data, 0).map_err(|e| format!("Ongeldig DXF-lettertype: {e:?}"))?;
        let factor = 1. / ttf.measurements.cap_height * height;
        let mut advance = 0f64;
        if !crate::python_values::trim(text).is_empty() {
            for (i, ch) in text.chars().enumerate() {
                if i % 1024 == 0 {
                    crate::input::check(cancel)?;
                }
                let w = if let Some(w) = ttf.advances.get(&ch) {
                    *w
                } else {
                    let w = face
                        .glyph_hor_advance(face.glyph_index(ch).unwrap_or(GlyphId(0)))
                        .unwrap_or(0);
                    if ttf.advances.len() < 8192 {
                        ttf.advances.insert(ch, w);
                    }
                    w
                };
                advance += f64::from(w);
            }
        }
        Ok((advance * factor * width, ttf.measurements.scale(factor)))
    }
    pub fn extent(&mut self, text: &Text, cancel: &AtomicBool) -> Result<Extent, String> {
        crate::input::check(cancel)?;
        if text.content.len() > 1_048_576
            || text.halign > 5
            || text.valign > 3
            || text.height <= 0.
            || !text.height.is_finite()
            || !text.width.is_finite()
            || !text.rotation.is_finite()
            || !text.oblique.is_finite()
        {
            return Err("Ongeldige DXF-tekstmaten of uitlijning".into());
        }
        geo::finite(text.insert)?;
        let align = geo::finite(text.align_point.unwrap_or(text.insert))?;
        let ocs = Affine::ocs(text.normal)?;
        let content = plain_text(&text.content);
        let (width, fm) = self.measure(&content, text.height, text.width, cancel)?;
        if [
            width,
            fm.baseline,
            fm.cap_height,
            fm.x_height,
            fm.descender_height,
        ]
        .iter()
        .any(|v| !v.is_finite())
        {
            return Err("DXF-tekstmaten vallen buiten getalbereik".into());
        }
        let mut result = Extent {
            width,
            measurements: fm,
            corners: None,
        };
        if content.is_empty() {
            return Ok(result);
        }
        let fit = text.halign == 3 || text.halign == 5;
        let mut sx = 1.;
        let mut sy = 1.;
        let delta = geo::sub(align, text.insert);
        if fit && width > 1e-9 {
            sx = geo::length(delta) / width;
            if text.halign == 3 {
                sy = sx;
            }
        }
        let width = width * sx;
        let fm = fm.scale(sy);
        let cap_top = fm.baseline + fm.cap_height;
        let bottom = fm.baseline - fm.descender_height;
        let (h, v) = if fit {
            (1, 0)
        } else if text.halign == 4 {
            (1, 4)
        } else {
            (text.halign, text.valign)
        };
        let dx = match h {
            1 => -width / 2.,
            2 => -width,
            _ => 0.,
        };
        let dy = match v {
            0 => fm.baseline,
            2 => -cap_top + fm.cap_height / 2.,
            3 => -cap_top,
            4 => -cap_top + (fm.cap_height + fm.descender_height) / 2.,
            _ => -bottom,
        };
        let insert = if fit {
            geo::add(text.insert, geo::scale(delta, 0.5))
        } else if h == 0 && v == 0 {
            text.insert
        } else {
            align
        };
        // FIT/ALIGNED use their endpoint angle except when endpoints are close.
        let close = delta
            .iter()
            .zip(text.insert.iter().zip(align))
            .all(|(d, (a, b))| d.abs() <= 1e-12f64.max(1e-9 * a.abs().max(b.abs())));
        let angle = if fit && !close {
            delta[1].atan2(delta[0])
        } else {
            text.rotation.to_radians()
        };
        let (s, c) = angle.sin_cos();
        let shear = text.oblique.to_radians().tan();
        let mx = if text.generation & 2 != 0 { -1. } else { 1. };
        let my = if text.generation & 4 != 0 { -1. } else { 1. };
        let mut corners = [[0.; 3]; 4];
        for (i, [x, y]) in [
            [0., bottom],
            [width, bottom],
            [width, cap_top],
            [0., cap_top],
        ]
        .into_iter()
        .enumerate()
        {
            let x = (x + y * shear + dx) * mx;
            let y = (y + dy) * my;
            corners[i] = ocs.checked_point(geo::add(insert, [x * c - y * s, x * s + y * c, 0.]))?;
        }
        crate::input::check(cancel)?;
        result.corners = Some(corners);
        Ok(result)
    }
}
/// Decode one-line TEXT codes before measurement, including Python's caret
/// modulo rule. MTEXT has a different layout/parser and must not use this path.
pub fn plain_text(value: &str) -> String {
    let mut decoded = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        let ch = if ch == '^' && chars.peek().is_some_and(|c| *c != '\n') {
            char::from_u32((chars.next().unwrap() as i64 - 64).rem_euclid(126) as u32).unwrap()
        } else {
            ch
        };
        if ch != '\n' && ch != '\r' {
            decoded.push(ch);
        }
    }
    let mut result = String::new();
    let mut chars = decoded.trim_end_matches('^').chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '%' && chars.peek() == Some(&'%') {
            let mut probe = chars.clone();
            probe.next();
            if let Some(code) = probe.next() {
                let code = code.to_lowercase().next().unwrap();
                let special = match code {
                    'c' => Some('Ø'),
                    'd' => Some('°'),
                    'p' => Some('±'),
                    _ => None,
                };
                if let Some(s) = special {
                    result.push(s);
                    chars = probe;
                    continue;
                }
                if matches!(code, 'k' | 'o' | 'u') {
                    chars = probe;
                    continue;
                }
            } else {
                // Original scanner returns an empty code at EOF; Python's
                // empty-string membership test treats it as a format code.
                chars = probe;
                continue;
            }
        }
        result.push(ch);
    }
    result
}
