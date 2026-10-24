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
    lff: Option<crate::lff_font::Font>,
}
/// Streaming, unscaled advances preserve each font's original addition order.
/// The wrapping caller can checkpoint a line without rescanning its prefix.
pub(crate) enum AdvanceReader<'a> {
    TrueType {
        face: Box<Face<'a>>,
        advances: &'a mut HashMap<char, u16>,
        factor: f64,
    },
    Stroke {
        font: &'a mut crate::stroke_font::Font,
        factor: f64,
    },
    Lff {
        font: &'a mut crate::lff_font::Font,
        factor: f64,
    },
    Mono {
        factor: f64,
    },
}
impl AdvanceReader<'_> {
    pub fn append(
        &mut self,
        text: &str,
        mut total: f64,
        work: &mut usize,
        cancel: &AtomicBool,
    ) -> Result<f64, String> {
        for ch in text.chars() {
            *work = work.saturating_add(1);
            if work.is_multiple_of(256) {
                crate::input::check(cancel)?;
            }
            let advance = match self {
                Self::TrueType { face, advances, .. } => {
                    let value = if let Some(value) = advances.get(&ch) {
                        *value
                    } else {
                        let value = face
                            .glyph_hor_advance(face.glyph_index(ch).unwrap_or(GlyphId(0)))
                            .unwrap_or(0);
                        if advances.len() < 8192 {
                            advances.insert(ch, value);
                        }
                        value
                    };
                    f64::from(value)
                }
                Self::Stroke { font, .. } => {
                    font.advance(ch, work, cancel).map_err(String::from)?
                }
                Self::Lff { font, .. } => font.advance(ch, work, cancel).map_err(String::from)?,
                Self::Mono { .. } => 1.,
            };
            if *work > 20_000_000 {
                return Err("MTEXT overschrijdt meetwerkbudget".into());
            }
            total += advance;
            if !total.is_finite() {
                return Err("MTEXT-tekstlengte valt buiten getalbereik".into());
            }
        }
        Ok(total)
    }
    pub fn width(&self, total: f64) -> Result<f64, String> {
        let factor = match self {
            Self::TrueType { factor, .. }
            | Self::Stroke { factor, .. }
            | Self::Lff { factor, .. }
            | Self::Mono { factor } => *factor,
        };
        let width = total * factor;
        if width.is_finite() {
            Ok(width)
        } else {
            Err("MTEXT-tekstbreedte valt buiten getalbereik".into())
        }
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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
    pub(crate) fn advance_reader(&mut self, height: f64) -> Result<AdvanceReader<'_>, String> {
        if let Some(f) = &mut self.ttf {
            return Ok(AdvanceReader::TrueType {
                face: Box::new(Face::parse(&f.data, 0).map_err(|e| e.to_string())?),
                advances: &mut f.advances,
                factor: 1. / f.measurements.cap_height * height,
            });
        }
        if let Some(font) = &mut self.stroke {
            let factor = height / font.native_measurements().cap_height;
            return Ok(AdvanceReader::Stroke { font, factor });
        }
        if let Some(font) = &mut self.lff {
            let factor = height / font.native_measurements().cap_height;
            return Ok(AdvanceReader::Lff { font, factor });
        }
        Ok(AdvanceReader::Mono { factor: height })
    }
    pub fn kind(&self) -> &'static str {
        if self.ttf.is_some() {
            "TrueTypeFont"
        } else if self.stroke.is_some() {
            "ShapeFileFont"
        } else if self.lff.is_some() {
            "LibreCadFont"
        } else {
            "MonospaceFont"
        }
    }
    pub(crate) fn source_id(&self) -> Option<(u8, usize)> {
        if let Some(ttf) = &self.ttf {
            Some((0, Arc::as_ptr(&ttf.data) as usize))
        } else if let Some(stroke) = &self.stroke {
            Some((1, stroke.source_id()))
        } else {
            self.lff.as_ref().map(|lff| (2, lff.source_id()))
        }
    }
    pub fn source_bytes(&self) -> usize {
        self.ttf.as_ref().map_or(0, |f| f.data.len())
            + self.stroke.as_ref().map_or(0, |f| f.source_bytes())
            + self.lff.as_ref().map_or(0, |f| f.source_bytes())
    }
    pub fn monospace() -> Self {
        Self {
            ttf: None,
            stroke: None,
            lff: None,
        }
    }
    pub fn load(path: &Path, cancel: &AtomicBool) -> Result<Self, String> {
        Self::load_catalog(path, cancel).map_err(String::from)
    }
    pub(crate) fn load_catalog(
        path: &Path,
        cancel: &AtomicBool,
    ) -> Result<Self, crate::font_error::Error> {
        let data = crate::input::read_typed(path, 64 * 1024 * 1024, "DXF-lettertype", cancel)?;
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ext.eq_ignore_ascii_case("lff") {
            let program = crate::lff_font::Program::parse(&data, cancel)?;
            return Ok(Self {
                ttf: None,
                stroke: None,
                lff: Some(crate::lff_font::Font::new(program, cancel)?),
            });
        }
        if ext.eq_ignore_ascii_case("shp") || ext.eq_ignore_ascii_case("shx") {
            let program = crate::stroke_font::Program::parse(&data, ext, cancel)?;
            return Ok(Self {
                ttf: None,
                lff: None,
                stroke: Some(
                    crate::stroke_font::Font::new(program, cancel)
                        .map_err(crate::font_error::Error::shape_metrics)?,
                ),
            });
        }
        let face = Face::parse(&data, 0).map_err(|e| {
            crate::font_error::Error::unavailable(format!("Ongeldig DXF-lettertype: {e:?}"))
        })?;
        let bounds = |ch| {
            let mut pen = ControlY {
                min: f64::INFINITY,
                max: f64::NEG_INFINITY,
            };
            face.outline_glyph(face.glyph_index(ch).unwrap_or(GlyphId(0)), &mut pen)
                .ok_or_else(|| {
                    crate::font_error::Error::unsupported(
                        "DXF-lettertype mist meetbare A/x/p-glyphs",
                    )
                })?;
            Ok::<_, crate::font_error::Error>((pen.min, pen.max))
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
            return Err(crate::font_error::Error::unsupported(
                "DXF-lettertype heeft geen geldige kapitaalhoogte",
            ));
        }
        crate::font_error::Error::check(cancel)?;
        Ok(Self {
            stroke: None,
            lff: None,
            ttf: Some(TrueType {
                data: Arc::new(data),
                measurements,
                advances: HashMap::new(),
            }),
        })
    }
    pub(crate) fn measure(
        &mut self,
        text: &str,
        height: f64,
        width: f64,
        cancel: &AtomicBool,
    ) -> Result<(f64, Measurements), String> {
        if let Some(lff) = &mut self.lff {
            return lff
                .measure(text, height, width, cancel)
                .map_err(String::from);
        }
        if let Some(stroke) = &mut self.stroke {
            return stroke
                .measure(text, height, width, cancel)
                .map_err(String::from);
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
    /// One unkerned glyph in drawing units at cap-height one. The TrueType
    /// renderer removes its measured baseline; CAD stroke fonts retain theirs.
    pub(crate) fn glyph_path(
        &self,
        ch: char,
        cancel: &AtomicBool,
    ) -> Result<(crate::cad_text_path::Path, f64, usize), String> {
        use crate::cad_text_path::Path;
        crate::input::check(cancel)?;
        if let Some(ttf) = &self.ttf {
            struct Pen {
                path: Path,
                start: V3,
                current: V3,
                error: Option<String>,
            }
            impl Pen {
                fn push(&mut self, kind: u8, points: &[V3]) {
                    if self.error.is_none() {
                        if let Err(err) = self.path.push(kind, points, 200_000) {
                            self.error = Some(err);
                        }
                    }
                    self.current = *points.last().unwrap();
                }
            }
            impl OutlineBuilder for Pen {
                fn move_to(&mut self, x: f32, y: f32) {
                    self.start = [f64::from(x), f64::from(y), 0.];
                    self.push(0, &[self.start]);
                }
                fn line_to(&mut self, x: f32, y: f32) {
                    self.push(1, &[[f64::from(x), f64::from(y), 0.]]);
                }
                fn quad_to(&mut self, x: f32, y: f32, x1: f32, y1: f32) {
                    self.push(
                        2,
                        &[
                            [f64::from(x), f64::from(y), 0.],
                            [f64::from(x1), f64::from(y1), 0.],
                        ],
                    );
                }
                fn curve_to(&mut self, x: f32, y: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
                    self.push(
                        3,
                        &[
                            [f64::from(x), f64::from(y), 0.],
                            [f64::from(x1), f64::from(y1), 0.],
                            [f64::from(x2), f64::from(y2), 0.],
                        ],
                    );
                }
                fn close(&mut self) {
                    if self.current != self.start {
                        self.push(1, &[self.start]);
                    }
                }
            }
            let face =
                Face::parse(&ttf.data, 0).map_err(|e| format!("Ongeldig DXF-lettertype: {e:?}"))?;
            let id = face.glyph_index(ch).unwrap_or(GlyphId(0));
            let mut pen = Pen {
                path: Path::default(),
                start: [0.; 3],
                current: [0.; 3],
                error: None,
            };
            face.outline_glyph(id, &mut pen);
            if let Some(err) = pen.error {
                return Err(err);
            }
            let factor = 1. / ttf.measurements.cap_height;
            // FontTools' glyf drawing reconciles hmtx LSB with glyf.xMin.
            // CFF outlines are already expressed in their drawing coordinates.
            let offset = if face.tables().glyf.is_some() {
                f64::from(face.glyph_hor_side_bearing(id).unwrap_or(0))
                    - face
                        .glyph_bounding_box(id)
                        .map_or(0., |b| f64::from(b.x_min))
            } else {
                0.
            };
            pen.path.transform(Affine([
                [factor, 0., 0., offset * factor],
                [0., factor, 0., -ttf.measurements.baseline * factor],
                [0., 0., 1., 0.],
            ]))?;
            crate::input::check(cancel)?;
            let work = pen.path.kinds.len();
            return Ok((
                pen.path,
                f64::from(face.glyph_hor_advance(id).unwrap_or(0)) * factor,
                work,
            ));
        }
        let (glyph, advance, cap, work) = if let Some(stroke) = &self.stroke {
            stroke.glyph(ch, cancel).map_err(String::from)?
        } else if let Some(lff) = &self.lff {
            lff.glyph(ch, cancel).map_err(String::from)?
        } else {
            return Err("Monospace heeft geen afzonderlijke lettercontouren".into());
        };
        let mut path = Path::default();
        if ch as u32 > 32 {
            if let Some(glyph) = glyph.filter(|g| g.commands > 0) {
                path.push(
                    0,
                    &[[glyph.controls[0][0] / cap, glyph.controls[0][1] / cap, 0.]],
                    200_000,
                )?;
                let mut at = 1;
                for kind in glyph.kinds {
                    let n = usize::from(kind.max(1));
                    let points: Vec<_> = glyph.controls[at..at + n]
                        .iter()
                        .map(|p| [p[0] / cap, p[1] / cap, 0.])
                        .collect();
                    path.push(kind, &points, 200_000)?;
                    at += n;
                }
            }
        }
        Ok((path, advance / cap, work))
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
