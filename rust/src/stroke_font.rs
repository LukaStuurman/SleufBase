//! Bounded AutoCAD SHP/SHX glyph programs and control geometry.
//! Protocol/metric semantics follow ezdxf 1.4.4 fonts/shapefile.py (MIT,
//! Copyright Manfred Moitzi; see third_party/ezdxf-LICENSE.txt). Subshape pen
//! state and consecutive vertical conditions follow Autodesk's specification.
use crate::{dxf_text_metrics::Measurements, input};
use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc},
};
type Point = [f64; 2];
#[derive(Debug, Clone, serde::Serialize)]
pub struct Glyph {
    pub controls: Vec<Point>,
    pub end: Point,
    pub commands: usize,
}
#[derive(Debug)]
pub struct Program {
    pub above: f64,
    pub below: f64,
    shapes: HashMap<u32, Vec<i32>>,
    bytes: usize,
    code_count: usize,
}
#[derive(Clone)]
pub struct Font {
    program: Arc<Program>,
    measurements: Measurements,
    space: f64,
    tofu: Glyph,
    widths: HashMap<u32, f64>,
}
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).ok_or("SHX-lengte loopt over")?;
        let b = self
            .data
            .get(self.at..end)
            .ok_or("Onvolledig SHX-bestand")?;
        self.at = end;
        Ok(b)
    }
    fn byte(&mut self) -> Result<i32, String> {
        Ok(i32::from(self.take(1)?[0]))
    }
    fn word(&mut self) -> Result<u32, String> {
        Ok(u32::from(u16::from_le_bytes(
            self.take(2)?.try_into().unwrap(),
        )))
    }
    fn string(&mut self) -> Result<&'a [u8], String> {
        let len = self
            .data
            .get(self.at..)
            .and_then(|v| v.iter().position(|b| *b == 0))
            .ok_or("SHX-naam mist afsluiting")?;
        let name = self.take(len)?;
        self.take(1)?;
        Ok(name)
    }
    fn signed(&mut self) -> Result<i32, String> {
        Ok(i32::from(self.take(1)?[0] as i8))
    }
    fn octant(&mut self) -> Result<i32, String> {
        let b = self.byte()?;
        Ok(if b & 128 != 0 { -(b & 127) } else { b })
    }
}
fn binary_codes(bytes: &[u8], unicode: bool, cancel: &AtomicBool) -> Result<Vec<i32>, String> {
    let mut r = Reader { data: bytes, at: 0 };
    let mut out = Vec::new();
    loop {
        if out.len().is_multiple_of(1024) {
            input::check(cancel)?;
        }
        let code = r.byte()?;
        out.push(code);
        match code {
            0 => break,
            1 | 2 | 5 | 6 | 14 | 16..=255 => {}
            3 | 4 => out.push(r.byte()?),
            7 => out.push(if unicode { r.word()? as i32 } else { r.byte()? }),
            8 => out.extend([r.signed()?, r.signed()?]),
            9 | 13 => loop {
                if out.len().is_multiple_of(1024) {
                    input::check(cancel)?;
                }
                if out.len() > 1_000_000 - 3 {
                    return Err("Te groot SHX-tekenprogramma".into());
                }
                let (x, y) = (r.signed()?, r.signed()?);
                out.extend([x, y]);
                if x == 0 && y == 0 {
                    break;
                }
                if code == 13 {
                    out.push(r.signed()?);
                }
            },
            10 => out.extend([r.byte()?, r.octant()?]),
            11 => out.extend([r.byte()?, r.byte()?, r.byte()?, r.byte()?, r.octant()?]),
            12 => out.extend([r.signed()?, r.signed()?, r.signed()?]),
            _ => return Err("Onbekende SHX-opdracht".into()),
        }
        if out.len() > 1_000_000 {
            return Err("Te groot SHX-tekenprogramma".into());
        }
    }
    Ok(out)
}
fn number(token: &str) -> Result<i32, String> {
    let t = token.trim().trim_matches(['(', ')']).trim();
    let (negative, digits) = t.strip_prefix('-').map_or((false, t), |s| (true, s));
    let digits = digits.strip_prefix('+').unwrap_or(digits);
    let value = if digits.starts_with('0') {
        i32::from_str_radix(digits.trim_start_matches("0x").trim_start_matches("0X"), 16)
    } else {
        digits.parse()
    }
    .map_err(|_| "Ongeldig SHP-getal")?;
    if negative {
        value
            .checked_neg()
            .ok_or_else(|| "SHP-getal loopt over".into())
    } else {
        Ok(value)
    }
}
fn numbers(data: &[u8], cancel: &AtomicBool) -> Result<Vec<i32>, String> {
    let text = std::str::from_utf8(data).map_err(|_| "SHP-codes zijn geen ASCII")?;
    let mut values = Vec::new();
    for token in text
        .split(',')
        .filter(|s| !s.trim().trim_matches(['(', ')']).is_empty())
    {
        if values.len().is_multiple_of(1024) {
            input::check(cancel)?;
        }
        if values.len() >= 1_000_000 {
            return Err("Te groot SHP-tekenprogramma".into());
        }
        values.push(number(token)?);
    }
    Ok(values)
}
impl Program {
    pub fn parse(data: &[u8], ext: &str, cancel: &AtomicBool) -> Result<Self, String> {
        input::check(cancel)?;
        if data.len() > 64 * 1024 * 1024 {
            return Err("SHP/SHX-bestand is te groot".into());
        }
        let mut result = Self {
            above: 0.,
            below: 0.,
            shapes: HashMap::new(),
            bytes: data.len(),
            code_count: 0,
        };
        if ext.eq_ignore_ascii_case("shp") {
            let mut lines = Vec::<Vec<u8>>::new();
            for (i, line) in data.split(|b| *b == b'\n').enumerate() {
                if i >= 1_000_000 {
                    return Err("Te veel SHP-regels".into());
                }
                if i % 1024 == 0 {
                    input::check(cancel)?;
                }
                let line = line.split(|b| *b == b';').next().unwrap().trim_ascii();
                if line.is_empty() {
                    continue;
                }
                if let Some(previous) = lines.last_mut() {
                    if (previous.starts_with(b"*") && line.starts_with(b","))
                        || (!previous.starts_with(b"*")
                            && (previous.ends_with(b",") || line.starts_with(b",")))
                    {
                        previous.extend_from_slice(line);
                        continue;
                    }
                }
                lines.push(line.to_vec());
            }
            let mut header = None::<Vec<u8>>;
            let mut body = Vec::new();
            let mut records = Vec::new();
            for line in lines {
                if line.starts_with(b"*") {
                    if records.len() >= 65536 {
                        return Err("Te veel SHP-definities".into());
                    }
                    if let Some(h) = header.replace(line) {
                        records.push((h, std::mem::take(&mut body)));
                    }
                } else {
                    body.extend_from_slice(&line);
                }
            }
            if let Some(h) = header {
                records.push((h, body));
            }
            for (h, b) in records {
                input::check(cancel)?;
                if h.starts_with(b"*BIGFONT") {
                    return Err("BIGFONT is niet ondersteund".into());
                }
                let spec = h.split(|c| *c == b',').next().unwrap();
                if spec == b"*0" || spec == b"*UNIFONT" {
                    // Font definition fields are decimal (glyph codes allow leading-0 hex).
                    let p: Vec<i32> = std::str::from_utf8(&b)
                        .map_err(|_| "Ongeldige SHP-fontdefinitie")?
                        .split(',')
                        .take(65)
                        .map(|s| {
                            s.trim()
                                .parse::<i32>()
                                .map_err(|_| "Ongeldige SHP-fontdefinitie")
                        })
                        .collect::<Result<_, _>>()?;
                    if p.len() < 4 || *p.last().unwrap() != 0 || !matches!(p[2], 0 | 2) {
                        return Err("Ongeldige SHP-fontdefinitie".into());
                    }
                    if spec == b"*UNIFONT"
                        && (p.len() != 6 || !(0..=2).contains(&p[3]) || !(0..=2).contains(&p[4]))
                    {
                        return Err("Ongeldige UNIFONT-definitie".into());
                    }
                    result.above = f64::from(p[0]);
                    result.below = f64::from(p[1]);
                    continue;
                }
                let text =
                    std::str::from_utf8(spec.strip_prefix(b"*").ok_or("SHP mist tekennummer")?)
                        .map_err(|_| "Ongeldig SHP-tekennummer")?;
                let id = number(text)?;
                if !(0..=65535).contains(&id) {
                    return Err("SHP-tekennummer buiten bereik".into());
                }
                result.insert(id as u32, numbers(&b, cancel)?)?;
            }
        } else if ext.eq_ignore_ascii_case("shx") {
            if data.starts_with(b"AutoCAD-86 shapes 1.0")
                || data.starts_with(b"AutoCAD-86 shapes 1.1")
            {
                let mut r = Reader { data, at: 23 };
                if r.byte()? != 26 {
                    return Err("SHX-handtekening ontbreekt".into());
                }
                let (first, last, count) = (r.word()?, r.word()?, r.word()?);
                let mut index = Vec::new();
                for _ in 0..count {
                    index.push((r.word()?, r.word()? as usize));
                }
                if index.first().map(|p| p.0) != Some(first)
                    || index.last().map(|p| p.0) != Some(last)
                {
                    return Err("Ongeldige SHX-index".into());
                }
                for (id, len) in index {
                    input::check(cancel)?;
                    let mut record = Reader {
                        data: r.take(len)?,
                        at: 0,
                    };
                    record.string()?;
                    if id == 0 {
                        result.above = f64::from(record.byte()?);
                        result.below = f64::from(record.byte()?);
                        if !matches!(record.byte()?, 0 | 2) {
                            return Err("Ongeldige SHX-fontmodus".into());
                        }
                        if record.byte()? != 0 {
                            return Err("SHX-fontdefinitie mist afsluiting".into());
                        }
                    } else {
                        result
                            .insert(id, binary_codes(&record.data[record.at..], false, cancel)?)?;
                    }
                }
                if r.take(3)? != b"EOF" {
                    return Err("SHX mist EOF-markering".into());
                }
            } else if data.starts_with(b"AutoCAD-86 unifont 1.0") {
                let mut r = Reader { data, at: 24 };
                if r.byte()? != 26 {
                    return Err("UNIFONT-handtekening ontbreekt".into());
                }
                r.take(6)?;
                r.string()?;
                result.above = f64::from(r.byte()?);
                result.below = f64::from(r.byte()?);
                if !matches!(r.byte()?, 0 | 2)
                    || !(0..=2).contains(&r.byte()?)
                    || !(0..=2).contains(&r.byte()?)
                    || r.byte()? != 0
                {
                    return Err("Ongeldige SHX UNIFONT-definitie".into());
                }
                while r.at < data.len() {
                    input::check(cancel)?;
                    let id = r.word()?;
                    let size = r.word()? as usize;
                    let bytes = r.take(size)?;
                    let mut record = Reader { data: bytes, at: 0 };
                    record.string()?;
                    result.insert(id, binary_codes(&bytes[record.at..], true, cancel)?)?;
                }
            } else {
                return Err("Onbekend of niet-ondersteund SHX-formaat (BIGFONT)".into());
            }
        } else {
            return Err("Onbekend stroke-fontformaat".into());
        }
        if result.above < 0. || result.below < 0. || result.shapes.is_empty() {
            return Err("Leeg of ongeldig SHP/SHX-lettertype".into());
        }
        result.bytes = result
            .shapes
            .values()
            .map(|c| c.capacity() * std::mem::size_of::<i32>())
            .sum::<usize>()
            + result.shapes.capacity() * std::mem::size_of::<(u32, Vec<i32>)>();
        input::check(cancel)?;
        Ok(result)
    }
    fn insert(&mut self, id: u32, codes: Vec<i32>) -> Result<(), String> {
        if codes.is_empty()
            || codes.last() != Some(&0)
            || codes.len() > 1_000_000
            || self.shapes.len() >= 65536
        {
            return Err("Ongeldig of te groot tekenprogramma".into());
        }
        let previous = self.shapes.get(&id).map_or(0, Vec::len);
        let count = self.code_count - previous + codes.len();
        if count > 4_000_000 {
            return Err("SHX-lettertype overschrijdt opdrachtbudget".into());
        }
        self.code_count = count;
        self.shapes.insert(id, codes);
        Ok(())
    }
    pub fn render(&self, id: u32, stacked: bool, cancel: &AtomicBool) -> Result<Glyph, String> {
        let mut r = Renderer {
            font: self,
            controls: vec![[0.; 2]],
            last_move: false,
            commands: 0,
            at: [0.; 2],
            scale: 1.,
            down: true,
            stack: Vec::new(),
            active: Vec::new(),
            steps: 0,
            stacked,
            cancel,
        };
        r.run(id, false)?;
        input::check(cancel)?;
        Ok(Glyph {
            controls: if r.commands == 0 {
                Vec::new()
            } else {
                r.controls
            },
            end: r.at,
            commands: r.commands,
        })
    }
}
struct Renderer<'a> {
    font: &'a Program,
    controls: Vec<Point>,
    last_move: bool,
    commands: usize,
    at: Point,
    scale: f64,
    down: bool,
    stack: Vec<Point>,
    active: Vec<u32>,
    steps: usize,
    stacked: bool,
    cancel: &'a AtomicBool,
}
fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1]]
}
fn mul(a: Point, s: f64) -> Point {
    [a[0] * s, a[1] * s]
}
impl Renderer<'_> {
    fn tick(&mut self) -> Result<(), String> {
        self.steps += 1;
        if self.steps.is_multiple_of(1024) {
            input::check(self.cancel)?;
        }
        if self.steps > 1_000_000 {
            return Err("SHX-opdrachtbudget overschreden".into());
        }
        Ok(())
    }
    fn point(&mut self, p: Point, moving: bool) -> Result<(), String> {
        if p.iter().any(|n| !n.is_finite()) || self.controls.len() >= 200_000 {
            return Err("Te grote of niet-eindige SHX-geometrie".into());
        }
        if moving && self.commands == 0 {
            self.controls[0] = p;
        } else if moving && self.last_move {
            *self.controls.last_mut().unwrap() = p;
        } else {
            self.controls.push(p);
            self.commands += 1;
        }
        self.last_move = moving;
        self.at = p;
        Ok(())
    }
    fn displacement(&mut self, x: f64, y: f64) -> Result<(), String> {
        self.point(add(self.at, [x * self.scale, y * self.scale]), !self.down)
    }
    fn close(a: f64, b: f64, abs: f64) -> bool {
        (a - b).abs() <= abs.max(1e-9 * a.abs().max(b.abs()))
    }
    fn ellipse(&mut self, center: Point, radius: f64, start: f64, end: f64) -> Result<(), String> {
        let mut curves = crate::stroke_curve::ellipse(center, radius, start, end)?;
        let close =
            |a: Point, b: Point| Self::close(a[0], b[0], 1e-12) && Self::close(a[1], b[1], 1e-12);
        if curves.last().is_some_and(|c| close(self.at, c[3])) {
            curves.reverse();
            for c in &mut curves {
                c.reverse();
            }
        }
        for c in curves {
            if !close(self.at, c[0]) {
                self.point(c[0], false)?;
            }
            let linear = |a: Point, b: Point| {
                (0..2).all(|i| (a[i] - b[i]).abs() <= 1e-15 * a[i].abs().max(b[i].abs()))
            };
            if linear(c[0], c[1]) && linear(c[3], c[2]) {
                self.point(c[3], false)?;
                continue;
            }
            for p in &c[1..] {
                if p.iter().any(|n| !n.is_finite()) || self.controls.len() >= 200_000 {
                    return Err("SHX-boog overschrijdt budget".into());
                }
                self.controls.push(*p);
            }
            self.commands += 1;
            self.last_move = false;
            self.at = c[3];
        }
        Ok(())
    }
    fn radial(&mut self, r: f64, start: f64, end: f64, ccw: bool) -> Result<(), String> {
        if r <= 0. || !r.is_finite() {
            return Err("Ongeldige SHX-boogstraal".into());
        }
        let offset = [r * start.cos(), r * start.sin()];
        let center = add(self.at, mul(offset, -1.));
        if !self.down {
            return self.point(add(center, [r * end.cos(), r * end.sin()]), true);
        }
        if ccw {
            self.ellipse(center, r, start, end)
        } else {
            self.ellipse(center, r, end, start)
        }
    }
    fn bulge(&mut self, x: i32, y: i32, b: i32) -> Result<(), String> {
        if !(-127..=127).contains(&b) || !(-127..=127).contains(&x) || !(-127..=127).contains(&y) {
            return Err("Ongeldige SHX-bulge".into());
        }
        if b == 0 || !self.down {
            return self.displacement(f64::from(x), f64::from(y));
        }
        let target = add(
            self.at,
            [f64::from(x) * self.scale, f64::from(y) * self.scale],
        );
        let (start, end) = if b > 0 {
            (self.at, target)
        } else {
            (target, self.at)
        };
        let b = f64::from(b.abs()) / 127.;
        let dx = end[0] - start[0];
        let dy = end[1] - start[1];
        let radius = dx.hypot(dy) * (1. + b * b) / 4. / b;
        let angle = dy.atan2(dx) + (std::f64::consts::FRAC_PI_2 - b.atan() * 2.);
        let center = add(start, [radius * angle.cos(), radius * angle.sin()]);
        let from = (start[1] - center[1]).atan2(start[0] - center[0]);
        let to = (end[1] - center[1]).atan2(end[0] - center[0]);
        self.ellipse(center, radius, from, to)
    }
    fn run(&mut self, id: u32, nested: bool) -> Result<(), String> {
        if self.active.len() >= 32 || self.active.contains(&id) {
            return Err("Cyclische of te diepe SHX-subtekens".into());
        }
        let codes = self
            .font
            .shapes
            .get(&id)
            .ok_or_else(|| format!("SHX-subteken {id} ontbreekt"))?;
        self.active.push(id);
        if !nested {
            self.down = true;
        }
        let mut at = 0;
        let mut skip = false;
        let mut ended = false;
        while at < codes.len() {
            self.tick()?;
            let code = codes[at];
            at += 1;
            let arg = |at: &mut usize| -> Result<i32, String> {
                let v = *codes.get(*at).ok_or("Onvolledige SHX-opdracht")?;
                *at += 1;
                Ok(v)
            };
            if code == 0 {
                ended = true;
                break;
            }
            if code == 14 {
                if skip {
                    skip = false;
                } else {
                    skip = !self.stacked;
                }
                continue;
            }
            match code {
                1 => {
                    if !skip {
                        self.down = true;
                    }
                }
                2 => {
                    if !skip {
                        self.down = false;
                    }
                }
                3 | 4 => {
                    let n = arg(&mut at)?;
                    if !(1..=255).contains(&n) {
                        return Err("Ongeldige SHX-schaalfactor".into());
                    }
                    if !skip {
                        self.scale = if code == 3 {
                            self.scale / f64::from(n)
                        } else {
                            self.scale * f64::from(n)
                        };
                        if !self.scale.is_finite() {
                            return Err("SHX-schaal loopt over".into());
                        }
                    }
                }
                5 => {
                    if !skip {
                        if self.stack.len() >= 4 {
                            return Err("SHX-positiestack is vol".into());
                        }
                        self.stack.push(self.at);
                    }
                }
                6 => {
                    if !skip {
                        let p = self.stack.pop().ok_or("SHX-positiestack is leeg")?;
                        self.point(p, true)?;
                    }
                }
                7 => {
                    let n = arg(&mut at)?;
                    if !(1..=65535).contains(&n) {
                        return Err("Ongeldig SHX-subteken".into());
                    }
                    if !skip {
                        self.run(n as u32, true)?;
                    }
                }
                8 | 9 => loop {
                    self.tick()?;
                    let (x, y) = (arg(&mut at)?, arg(&mut at)?);
                    if !(-128..=127).contains(&x) || !(-128..=127).contains(&y) {
                        return Err("SHX-verplaatsing buiten bereik".into());
                    }
                    if code == 9 && x == 0 && y == 0 {
                        break;
                    }
                    if !skip {
                        self.displacement(f64::from(x), f64::from(y))?;
                    }
                    if code == 8 {
                        break;
                    }
                },
                10 | 11 => {
                    let (startoff, endoff, radius, spec) = if code == 10 {
                        (0, 256, arg(&mut at)?, arg(&mut at)?)
                    } else {
                        let (s, e, hi, lo, spec) = (
                            arg(&mut at)?,
                            arg(&mut at)?,
                            arg(&mut at)?,
                            arg(&mut at)?,
                            arg(&mut at)?,
                        );
                        if [s, e, hi, lo].iter().any(|n| !(0..=255).contains(n)) {
                            return Err("Ongeldige fractionele SHX-boog".into());
                        }
                        (s, if e == 0 { 256 } else { e }, hi * 256 + lo, spec)
                    };
                    if !(-127..=127).contains(&spec)
                        || radius <= 0
                        || radius > if code == 10 { 255 } else { 65535 }
                    {
                        return Err("Ongeldige SHX-octantboog".into());
                    }
                    let sign = if spec < 0 { -1. } else { 1. };
                    let oct = (spec.abs() >> 4) & 15;
                    let span = spec.abs() & 15;
                    if oct > 7 || span > 7 {
                        return Err("SHX-octant buiten bereik".into());
                    }
                    let start = (f64::from(oct) * 45. + sign * f64::from(startoff) * 45. / 256.)
                        .to_radians();
                    let end = if code == 10 {
                        start
                            + sign
                                * f64::from(if span == 0 { 8 } else { span })
                                * std::f64::consts::FRAC_PI_4
                    } else {
                        (f64::from(oct) * 45.
                            + sign * (f64::from(span - 1) * 45. + f64::from(endoff) * 45. / 256.))
                            .to_radians()
                    };
                    if !skip {
                        self.radial(f64::from(radius) * self.scale, start, end, spec >= 0)?;
                    }
                }
                12 | 13 => loop {
                    self.tick()?;
                    let (x, y) = (arg(&mut at)?, arg(&mut at)?);
                    if code == 13 && x == 0 && y == 0 {
                        break;
                    }
                    let b = arg(&mut at)?;
                    if !skip {
                        self.bulge(x, y, b)?;
                    }
                    if code == 12 {
                        break;
                    }
                },
                16..=255 => {
                    if !skip {
                        const X: [f64; 16] = [
                            1., 1., 1., 0.5, 0., -0.5, -1., -1., -1., -1., -1., -0.5, 0., 0.5, 1.,
                            1.,
                        ];
                        const Y: [f64; 16] = [
                            0., 0.5, 1., 1., 1., 1., 1., 0.5, 0., -0.5, -1., -1., -1., -1., -1.,
                            -0.5,
                        ];
                        let dir = (code & 15) as usize;
                        let len = f64::from((code >> 4) & 15);
                        self.displacement(X[dir] * len, Y[dir] * len)?;
                    }
                }
                _ => return Err("Ongeldige SHX-opcode".into()),
            }
            skip = false;
        }
        self.active.pop();
        if !ended {
            return Err("SHX-tekenprogramma mist einde".into());
        }
        Ok(())
    }
}
fn range(points: &[Point]) -> Result<(Point, Point), String> {
    let first = *points
        .first()
        .ok_or("SHX-font mist meetbare glyphgeometrie")?;
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
    pub fn new(program: Program, cancel: &AtomicBool) -> Result<Self, String> {
        let space = if program.shapes.contains_key(&32) {
            program.render(32, false, cancel)?.end[0]
        } else if program.shapes.contains_key(&65) {
            program.render(65, false, cancel)?.end[0]
        } else {
            program.above
        };
        let (start, end, w, h) = if program.shapes.contains_key(&65) {
            let a = program.render(65, false, cancel)?;
            let (min, max) = range(&a.controls)?;
            (a.controls[0], a.end, max[0] - min[0], max[1] - min[1])
        } else {
            ([0.; 2], [space, 0.], space, program.above)
        };
        let tofu = Glyph {
            controls: vec![
                start,
                add(start, [w, 0.]),
                add(start, [w, h]),
                add(start, [0., h]),
                start,
                end,
            ],
            end,
            commands: 5,
        };
        let get = |id| {
            if program.shapes.contains_key(&id) {
                program.render(id, false, cancel)
            } else {
                Ok(tofu.clone())
            }
        };
        let x = get(120)?;
        let (xmin, xmax) = range(&x.controls[..x.controls.len().saturating_sub(1)])?;
        let cap = if program.above != 0. {
            program.above
        } else {
            let a = get(65)?;
            range(&a.controls[..a.controls.len().saturating_sub(1)])?.1[1] - xmin[1]
        };
        let below = if program.below != 0. {
            program.below
        } else {
            let p = get(112)?;
            xmin[1] - range(&p.controls[..p.controls.len().saturating_sub(1)])?.0[1]
        };
        if cap <= 0. || !cap.is_finite() {
            return Err("SHX-font heeft geen geldige kapitaalhoogte".into());
        }
        let measurements = Measurements {
            baseline: xmin[1],
            cap_height: cap,
            x_height: xmax[1] - xmin[1],
            descender_height: below,
        };
        input::check(cancel)?;
        Ok(Self {
            program: Arc::new(program),
            measurements,
            space,
            tofu,
            widths: HashMap::new(),
        })
    }
    pub fn source_bytes(&self) -> usize {
        self.program.bytes
    }
    pub fn measure(
        &mut self,
        text: &str,
        height: f64,
        width: f64,
        cancel: &AtomicBool,
    ) -> Result<(f64, Measurements), String> {
        let factor = height / self.measurements.cap_height;
        let mut advance = 0.;
        if !crate::python_values::trim(text).is_empty() {
            for (i, ch) in text.chars().enumerate() {
                if i % 1024 == 0 {
                    input::check(cancel)?;
                }
                let id = ch as u32;
                let value = if id == 32 {
                    self.space
                } else if let Some(n) = self.widths.get(&id) {
                    *n
                } else {
                    let n = if self.program.shapes.contains_key(&id) {
                        self.program.render(id, false, cancel)?.end[0]
                    } else if id < 32 {
                        self.space
                    } else {
                        self.tofu.end[0]
                    };
                    if self.widths.len() < 8192 {
                        self.widths.insert(id, n);
                    }
                    n
                };
                advance += value;
            }
        }
        Ok((advance * (factor * width), self.measurements.scale(factor)))
    }
}
