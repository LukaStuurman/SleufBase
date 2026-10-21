//! LibreCAD LFF glyphs and horizontal fast-text measurement.
//! Source contract: ezdxf 1.4.4 fonts/lff.py, Copyright 2023 Manfred Moitzi,
//! MIT; retain third_party/ezdxf-LICENSE.txt. Inherited contours are shared.
use crate::{
    dxf_text_metrics::Measurements,
    stroke_curve::{self, Point},
    stroke_font::Glyph,
};
use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc},
};
type Vertex = [f64; 3];
#[derive(Debug)]
struct Node {
    parent: Option<Arc<Node>>,
    polylines: Vec<Vec<Vertex>>,
    depth: usize,
}
#[derive(Debug)]
pub struct Program {
    pub name: String,
    pub letter_spacing: f64,
    pub word_spacing: f64,
    glyphs: HashMap<u32, Arc<Node>>,
    bytes: usize,
}
#[derive(Clone)]
pub struct Font {
    program: Arc<Program>,
    measurements: Measurements,
    tofu_advance: f64,
    widths: HashMap<u32, f64>,
}
fn parsed(value: &str) -> Option<f64> {
    let value = value.trim();
    if value.chars().any(|c| !c.is_ascii() || c == '_') {
        return crate::python_values::number(Some(&serde_json::Value::String(value.into())));
    }
    value.parse().ok()
}
fn value(value: &str) -> Result<f64, crate::font_error::Error> {
    let n = parsed(value.strip_prefix('A').unwrap_or(value)).unwrap_or(0.);
    if !n.is_finite() {
        return Err("Niet-eindig LFF-coördinaat".into());
    }
    Ok(n)
}
fn hex(value: &str) -> Option<u32> {
    let value = crate::python_values::trim(value);
    let translated: String = value
        .chars()
        .map(|c| crate::object_layout::decimal(c).map_or(c, |n| char::from(b'0' + n as u8)))
        .collect();
    let digits = translated.strip_prefix('+').unwrap_or(&translated);
    let prefixed = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"));
    let digits = if let Some(s) = prefixed {
        s.strip_prefix('_').unwrap_or(s)
    } else {
        digits
    };
    let bytes = digits.as_bytes();
    if bytes.iter().enumerate().any(|(i, b)| {
        *b == b'_'
            && (i == 0
                || i + 1 == bytes.len()
                || !bytes[i - 1].is_ascii_hexdigit()
                || !bytes[i + 1].is_ascii_hexdigit())
    }) {
        return None;
    }
    u32::from_str_radix(&digits.replace('_', ""), 16).ok()
}
fn code(header: &str) -> Option<u32> {
    let field = header.strip_prefix('[')?;
    let field = field.split(']').next().unwrap_or(field);
    let parsed = hex(field).or_else(|| {
        let chars: String = field.chars().filter(char::is_ascii_hexdigit).collect();
        u32::from_str_radix(&chars, 16).ok()
    })?;
    (parsed > 0 && parsed <= 0x10ffff).then_some(parsed)
}
impl Program {
    pub fn codes(&self) -> impl Iterator<Item = u32> + '_ {
        self.glyphs.keys().copied()
    }
    pub fn parse(data: &[u8], cancel: &AtomicBool) -> Result<Self, crate::font_error::Error> {
        crate::font_error::Error::check(cancel)?;
        if data.len() > 64 * 1024 * 1024 {
            return Err(crate::font_error::Error::limit("LFF-font is te groot"));
        }
        let text = std::str::from_utf8(data).map_err(|_| "LFF-font is geen UTF-8")?;
        let mut result = Self {
            name: String::new(),
            letter_spacing: 0.,
            word_spacing: 0.,
            glyphs: HashMap::new(),
            bytes: 0,
        };
        let mut current = None;
        let mut lines = Vec::new();
        let mut definitions = 0usize;
        let mut vertices = 0usize;
        for (i, line) in text.split('\n').enumerate() {
            if i >= 1_000_000 {
                return Err(crate::font_error::Error::limit("Te veel LFF-regels"));
            }
            if i % 1024 == 0 {
                crate::font_error::Error::check(cancel)?;
            }
            let line = crate::python_values::trim(line);
            if let Some(comment) = line.strip_prefix('#') {
                if let Some((key, val)) = comment.split_once(':').filter(|(_, v)| !v.contains(':'))
                {
                    match crate::python_values::trim(key) {
                        "Name" => {
                            if val.len() > 1_048_576 {
                                return Err(crate::font_error::Error::limit(
                                    "LFF-fontnaam is te lang",
                                ));
                            }
                            result.name = crate::python_values::trim(val).into();
                        }
                        "LetterSpacing" | "WordSpacing" => {
                            if let Some(n) = parsed(val) {
                                if !n.is_finite() {
                                    return Err("Niet-eindige LFF-letterafstand".into());
                                }
                                if crate::python_values::trim(key) == "LetterSpacing" {
                                    result.letter_spacing = n;
                                } else {
                                    result.word_spacing = n;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                continue;
            }
            if line.starts_with('[') {
                if let Some(id) = current.take() {
                    result.record(id, &lines, &mut vertices, cancel)?;
                }
                lines.clear();
                definitions += 1;
                if definitions > 65536 {
                    return Err(crate::font_error::Error::limit(
                        "Te veel LFF-tekenprogramma's",
                    ));
                }
                current = code(line);
            } else if !line.is_empty() && current.is_some() {
                lines.push(line);
            }
        }
        if let Some(id) = current {
            result.record(id, &lines, &mut vertices, cancel)?;
        }
        result.bytes += result.name.capacity()
            + result.glyphs.capacity() * std::mem::size_of::<(u32, Arc<Node>)>();
        if result.glyphs.is_empty() {
            return Err("Leeg LFF-lettertype".into());
        }
        crate::font_error::Error::check(cancel)?;
        Ok(result)
    }
    fn record(
        &mut self,
        id: u32,
        lines: &[&str],
        vertices: &mut usize,
        cancel: &AtomicBool,
    ) -> Result<(), crate::font_error::Error> {
        let Some(first) = lines.first() else {
            return Err("LFF-teken mist geometrie".into());
        };
        let mut parent = None;
        let mut data = lines;
        if let Some(hex) = first.strip_prefix('C') {
            let Some(n) = self::hex(hex) else {
                return Ok(());
            };
            if n != 0 {
                let Some(p) = self.glyphs.get(&n) else {
                    return Ok(());
                };
                parent = Some(p.clone());
            }
            data = &lines[1..];
        }
        let depth = parent.as_ref().map_or(1, |p| p.depth + 1);
        if depth > 64 {
            return Err(crate::font_error::Error::limit("LFF-overerving is te diep"));
        }
        let mut polylines = Vec::new();
        for line in data {
            crate::font_error::Error::check(cancel)?;
            let mut polyline = Vec::new();
            for token in line.split(';') {
                *vertices += 1;
                if vertices.is_multiple_of(1024) {
                    crate::font_error::Error::check(cancel)?;
                }
                if *vertices > 1_000_000 {
                    return Err(crate::font_error::Error::limit(
                        "LFF-vertexbudget overschreden",
                    ));
                }
                let mut parts = token.split(',');
                let x = parts.next().unwrap();
                if let Some(y) = parts.next() {
                    polyline.push([
                        value(x)?,
                        value(y)?,
                        parts.next().map(value).transpose()?.unwrap_or(0.),
                    ]);
                }
            }
            self.bytes += polyline.capacity() * std::mem::size_of::<Vertex>();
            polylines.push(polyline);
        }
        self.bytes +=
            std::mem::size_of::<Node>() + polylines.capacity() * std::mem::size_of::<Vec<Vertex>>();
        if self.bytes > 64 * 1024 * 1024 {
            return Err(crate::font_error::Error::limit(
                "LFF-programmabudget overschreden",
            ));
        }
        self.glyphs.insert(
            id,
            Arc::new(Node {
                parent,
                polylines,
                depth,
            }),
        );
        Ok(())
    }
    pub fn source_bytes(&self) -> usize {
        self.bytes
    }
    pub fn render(&self, id: u32, cancel: &AtomicBool) -> Result<Glyph, crate::font_error::Error> {
        self.render_with_work(id, cancel).map(|r| r.0)
    }
    fn render_with_work(
        &self,
        id: u32,
        cancel: &AtomicBool,
    ) -> Result<(Glyph, usize), crate::font_error::Error> {
        crate::font_error::Error::check(cancel)?;
        let mut node = self.glyphs.get(&id).ok_or("LFF-teken ontbreekt")?.as_ref();
        let mut chain = vec![node];
        while let Some(parent) = &node.parent {
            node = parent;
            chain.push(node);
        }
        let mut path = Path::new();
        let mut work = 0usize;
        for node in chain.into_iter().rev() {
            for polyline in &node.polylines {
                work += 1;
                if work.is_multiple_of(1024) {
                    crate::font_error::Error::check(cancel)?;
                }
                if work > 1_000_000 {
                    return Err(crate::font_error::Error::limit(
                        "LFF-teken overschrijdt werkbudget",
                    ));
                }
                // Empty/single-point polylines never add commands in the source.
                // Count them as work but avoid allocating a temporary path.
                if polyline.len() < 2 {
                    continue;
                }
                let mut part = Path::new();
                if let Some(first) = polyline.first() {
                    part.point([first[0], first[1]], true)?;
                }
                for pair in polyline.windows(2) {
                    work += 1;
                    if work.is_multiple_of(1024) {
                        crate::font_error::Error::check(cancel)?;
                    }
                    if work > 1_000_000 {
                        return Err(crate::font_error::Error::limit(
                            "LFF-teken overschrijdt werkbudget",
                        ));
                    }
                    let start = [pair[0][0], pair[0][1]];
                    let end = [pair[1][0], pair[1][1]];
                    let bulge = pair[1][2];
                    if bulge.abs() < 1e-6 {
                        part.point(end, false)?;
                    } else {
                        part.bulge(start, end, bulge)?;
                    }
                }
                if part.commands > 0 {
                    path.point(part.controls[0], true)?;
                    if path.controls.len() + part.controls.len() - 1 > 200_000 {
                        return Err(crate::font_error::Error::limit(
                            "LFF-teken overschrijdt puntbudget",
                        ));
                    }
                    path.controls.extend_from_slice(&part.controls[1..]);
                    path.commands += part.commands;
                    path.at = part.at;
                    path.last_move = false;
                }
            }
        }
        crate::font_error::Error::check(cancel)?;
        Ok((
            Glyph {
                controls: path.controls,
                end: path.at,
                commands: path.commands,
            },
            work,
        ))
    }
}
struct Path {
    controls: Vec<Point>,
    at: Point,
    commands: usize,
    last_move: bool,
}
impl Path {
    fn new() -> Self {
        Self {
            controls: vec![[0.; 2]],
            at: [0.; 2],
            commands: 0,
            last_move: false,
        }
    }
    fn point(&mut self, p: Point, moving: bool) -> Result<(), crate::font_error::Error> {
        if self.controls.len() >= 200_000 {
            return Err(crate::font_error::Error::limit("Te grote LFF-geometrie"));
        }
        if p.iter().any(|n| !n.is_finite()) {
            return Err("Ongeldige of te grote LFF-geometrie".into());
        }
        if moving && self.commands == 0 {
            self.controls[0] = p;
        } else if moving && self.last_move {
            *self.controls.last_mut().unwrap() = p;
        } else {
            self.controls.push(p);
            self.commands += 1;
        }
        self.at = p;
        self.last_move = moving;
        Ok(())
    }
    fn near(a: Point, b: Point, rel: f64, abs: f64) -> bool {
        (0..2).all(|i| (a[i] - b[i]).abs() <= abs.max(rel * a[i].abs().max(b[i].abs())))
    }
    fn bulge(&mut self, start: Point, end: Point, b: f64) -> Result<(), crate::font_error::Error> {
        if Self::near(start, end, 1e-10, 0.) {
            return Ok(());
        }
        let dx = end[0] - start[0];
        let dy = end[1] - start[1];
        let r = dx.hypot(dy) * (1. + b * b) / 4. / b;
        let a = dy.atan2(dx) + (std::f64::consts::FRAC_PI_2 - b.atan() * 2.);
        let center = [start[0] + r * a.cos(), start[1] + r * a.sin()];
        let from = (start[1] - center[1]).atan2(start[0] - center[0]);
        let to = (end[1] - center[1]).atan2(end[0] - center[0]);
        let (from, to) = if b < 0. { (to, from) } else { (from, to) };
        let from = from.rem_euclid(std::f64::consts::TAU);
        let mut to = to.rem_euclid(std::f64::consts::TAU);
        if from > to {
            to += std::f64::consts::TAU;
        }
        let mut curves = stroke_curve::ellipse(
            center,
            r.abs(),
            from.to_degrees().to_radians(),
            to.to_degrees().to_radians(),
        )?;
        if curves
            .first()
            .is_some_and(|c| Self::near(c[0], end, 1e-10, 0.))
        {
            curves.reverse();
            for c in &mut curves {
                c.reverse();
            }
        }
        if curves
            .last()
            .is_some_and(|c| Self::near(c[3], self.at, 1e-9, 1e-12))
        {
            curves.reverse();
            for c in &mut curves {
                c.reverse();
            }
        }
        for c in curves {
            if !Self::near(c[0], self.at, 1e-9, 1e-12) {
                self.point(c[0], false)?;
            }
            if Self::near(c[0], c[1], 1e-15, 0.) && Self::near(c[3], c[2], 1e-15, 0.) {
                self.point(c[3], false)?;
                continue;
            }
            if self.controls.len() + 3 > 200_000 {
                return Err(crate::font_error::Error::limit(
                    "LFF-boog overschrijdt puntbudget",
                ));
            }
            if c.iter().flatten().any(|v| !v.is_finite()) {
                return Err("Niet-eindige LFF-boog".into());
            }
            self.controls.extend_from_slice(&c[1..]);
            self.at = c[3];
            self.commands += 1;
            self.last_move = false;
        }
        Ok(())
    }
}
fn range(points: &[Point]) -> Result<(Point, Point), crate::font_error::Error> {
    let first = *points
        .first()
        .ok_or("LFF-font mist meetbare A/x/p-contouren")?;
    let (mut min, mut max) = (first, first);
    for p in points {
        for i in 0..2 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    Ok((min, max))
}
impl Font {
    pub fn new(program: Program, cancel: &AtomicBool) -> Result<Self, crate::font_error::Error> {
        let a = program.render(65, cancel)?;
        let (min, max) = range(&a.controls)?;
        let start = a.controls[0];
        let w = max[0] - min[0];
        let h = max[1] - min[1];
        let tofu = Glyph {
            controls: vec![
                start,
                [start[0] + w, start[1]],
                [start[0] + w, start[1] + h],
                [start[0], start[1] + h],
                start,
                a.end,
            ],
            end: a.end,
            commands: 5,
        };
        let x = if program.glyphs.contains_key(&120) {
            program.render(120, cancel)?
        } else {
            tofu.clone()
        };
        let p = if program.glyphs.contains_key(&112) {
            program.render(112, cancel)?
        } else {
            tofu.clone()
        };
        let (xmin, xmax) = range(&x.controls)?;
        let pmin = range(&p.controls)?.0;
        let measurements = Measurements {
            baseline: xmin[1],
            cap_height: max[1] - xmin[1],
            x_height: xmax[1] - xmin[1],
            descender_height: xmin[1] - pmin[1],
        };
        if measurements.cap_height <= 0. || !measurements.cap_height.is_finite() {
            return Err("LFF-font heeft geen geldige kapitaalhoogte".into());
        }
        let tofu_advance = range(&tofu.controls)?.1[0] + program.letter_spacing;
        let mut widths = HashMap::new();
        for (id, glyph) in [(65, a), (120, x), (112, p)] {
            widths.insert(
                id,
                if glyph.commands == 0 {
                    0.
                } else {
                    range(&glyph.controls)?.1[0] + program.letter_spacing
                },
            );
        }
        Ok(Self {
            program: Arc::new(program),
            measurements,
            tofu_advance,
            widths,
        })
    }
    pub fn source_bytes(&self) -> usize {
        self.program.source_bytes()
    }
    pub(crate) fn source_id(&self) -> usize {
        Arc::as_ptr(&self.program) as usize
    }
    pub fn measure(
        &mut self,
        text: &str,
        height: f64,
        width: f64,
        cancel: &AtomicBool,
    ) -> Result<(f64, Measurements), crate::font_error::Error> {
        let factor = height / self.measurements.cap_height;
        let mut total = 0.;
        let mut work = 0usize;
        if !crate::python_values::trim(text).is_empty() {
            for (i, ch) in text.chars().enumerate() {
                if i % 1024 == 0 {
                    crate::font_error::Error::check(cancel)?;
                }
                let id = ch as u32;
                let advance = if id < 32 {
                    0.
                } else if id == 32 {
                    self.program.word_spacing
                } else if let Some(n) = self.widths.get(&id) {
                    *n
                } else if self.program.glyphs.contains_key(&id) {
                    let (glyph, spent) = self.program.render_with_work(id, cancel)?;
                    work += spent;
                    if work > 20_000_000 {
                        return Err(crate::font_error::Error::limit(
                            "LFF-tekst overschrijdt werkbudget",
                        ));
                    }
                    let n = if glyph.commands == 0 {
                        0.
                    } else {
                        range(&glyph.controls)?.1[0] + self.program.letter_spacing
                    };
                    if self.widths.len() < 8192 {
                        self.widths.insert(id, n);
                    }
                    n
                } else {
                    self.tofu_advance
                };
                total += advance;
            }
        }
        Ok((total * (factor * width), self.measurements.scale(factor)))
    }
}
