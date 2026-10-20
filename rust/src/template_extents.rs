//! Original fast-bbox semantics for generated template profiles and rasters.
//! Retains curve control bounds rather than using preview tessellation. Unknown
//! geometry is an error, never an invented/partial selector position.
use crate::{
    cad_geometry::{self as g, Affine, Placement, V3},
    dxf_document::{Document, RecordView},
    dxf_text_metrics::{Font, Text},
};
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::AtomicBool,
};

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct Bounds {
    pub min: V3,
    pub max: V3,
}
impl Bounds {
    fn add(&mut self, p: V3) {
        for (i, v) in p.into_iter().enumerate() {
            self.min[i] = self.min[i].min(v);
            self.max[i] = self.max[i].max(v);
        }
    }
    pub fn selector(self) -> Result<V3, String> {
        let span = (self.max[0] - self.min[0])
            .max(self.max[1] - self.min[1])
            .max(0.);
        let margin = (span * 0.08).clamp(0.15, 0.75);
        g::finite([self.max[0] + margin, self.max[1] + margin, self.max[2]])
    }
}
fn number(r: RecordView<'_>, c: i32, default: f64) -> Result<f64, String> {
    let n = r.first(c).map_or(Ok(default), |v| {
        v.trim()
            .parse::<f64>()
            .map_err(|_| format!("Ongeldig DXF-getal bij {c}"))
    })?;
    if n.is_finite() {
        Ok(n)
    } else {
        Err("Niet-eindige DXF-grenscoördinaat".into())
    }
}
fn integer(r: RecordView<'_>, c: i32, default: u32, max: u32) -> Result<u32, String> {
    let value = number(r, c, f64::from(default))?;
    if value < 0. || value > f64::from(max) || value.fract() != 0. {
        return Err(format!("Ongeldig geheel DXF-getal bij {c}"));
    }
    Ok(value as u32)
}
fn point(r: RecordView<'_>, c: i32, default: V3) -> Result<V3, String> {
    Ok([
        number(r, c, default[0])?,
        number(r, c + 10, default[1])?,
        number(r, c + 20, default[2])?,
    ])
}
fn vertices(r: RecordView<'_>, code: i32, cancel: &AtomicBool) -> Result<Vec<V3>, String> {
    let mut out = Vec::new();
    for (i, (c, v)) in r.tags().enumerate() {
        if i % 1024 == 0 {
            crate::input::check(cancel)?;
        }
        if c == code {
            if out.len() >= 2_000_000 {
                return Err("Te veel DXF-vertices".into());
            }
            out.push([v.trim().parse().map_err(|_| "Ongeldig DXF-vertex")?, 0., 0.]);
        } else if c == code + 10 || c == code + 20 {
            if let Some(p) = out.last_mut() {
                p[((c - code) / 10) as usize] =
                    v.trim().parse().map_err(|_| "Ongeldig DXF-vertex")?;
            }
        }
    }
    Ok(out)
}
struct Block<'a> {
    base: V3,
    records: Vec<RecordView<'a>>,
}
pub struct Index<'a> {
    doc: &'a Document,
    blocks: HashMap<String, Block<'a>>,
    children: HashMap<u64, Vec<RecordView<'a>>>,
    styles: HashMap<String, String>,
}
impl<'a> Index<'a> {
    /// Document must be freshly indexed, with staged geometry already applied.
    pub fn new(doc: &'a Document, cancel: &AtomicBool) -> Result<Self, String> {
        doc.require_current_index()?;
        let mut blocks = HashMap::new();
        let mut children = HashMap::<u64, Vec<_>>::new();
        let mut styles = HashMap::new();
        let mut block = None;
        let mut parent = None;
        for (i, r) in doc.records().enumerate() {
            if i % 1024 == 0 {
                crate::input::check(cancel)?;
            }
            if r.kind() == "STYLE" {
                styles.insert(
                    crate::dxf_symbols::key(r.first(2).unwrap_or("Standard")),
                    r.first(3).unwrap_or("txt").to_owned(),
                );
            }
            if !matches!(r.section(), Some("BLOCKS" | "ENTITIES")) {
                parent = None;
                continue;
            }
            if r.kind() == "BLOCK" {
                let name = crate::dxf_symbols::key(r.first(2).ok_or("Blok mist naam")?);
                blocks.insert(
                    name.clone(),
                    Block {
                        base: point(r, 10, [0.; 3])?,
                        records: Vec::new(),
                    },
                );
                block = Some(name);
                parent = None;
                continue;
            }
            if r.kind() == "ENDBLK" {
                block = None;
                parent = None;
                continue;
            }
            if matches!(r.kind(), "ATTRIB" | "VERTEX" | "SEQEND") {
                if let Some(h) = parent {
                    children.entry(h).or_default().push(r);
                }
            } else {
                parent = r.handle();
                if let Some(name) = &block {
                    blocks.get_mut(name).unwrap().records.push(r);
                }
            }
        }
        Ok(Self {
            doc,
            blocks,
            children,
            styles,
        })
    }
    pub fn bounds(
        &self,
        ids: &[u64],
        resolve: &mut impl FnMut(&str) -> Result<Font, String>,
        cancel: &AtomicBool,
    ) -> Result<Option<Bounds>, String> {
        let mut work = Work {
            index: self,
            resolve,
            fonts: HashMap::new(),
            font_bytes: 0,
            bounds: None,
            stack: HashSet::new(),
            count: 0,
            points: 0,
            cancel,
        };
        for h in ids {
            let r = self
                .doc
                .by_handle(&format!("{h:X}"))?
                .ok_or("Grensmeting mist een geselecteerd object")?;
            work.entity(r, Affine::IDENTITY, 0)?;
        }
        crate::input::check(cancel)?;
        Ok(work.bounds)
    }
    /// Same candidate rule as production: ignore IMAGE if any profile exists.
    pub fn selector(
        &self,
        ids: &[u64],
        resolve: &mut impl FnMut(&str) -> Result<Font, String>,
        cancel: &AtomicBool,
    ) -> Result<Option<V3>, String> {
        let mut profiles = Vec::new();
        for h in ids {
            crate::input::check(cancel)?;
            let r = self
                .doc
                .by_handle(&format!("{h:X}"))?
                .ok_or("Selector mist een geselecteerd object")?;
            if r.kind() != "IMAGE" {
                profiles.push(*h);
            }
        }
        self.bounds(
            if profiles.is_empty() { ids } else { &profiles },
            resolve,
            cancel,
        )?
        .map(Bounds::selector)
        .transpose()
    }
}
struct Work<'a, 'b, F> {
    index: &'b Index<'a>,
    resolve: &'b mut F,
    fonts: HashMap<String, Font>,
    font_bytes: usize,
    bounds: Option<Bounds>,
    stack: HashSet<String>,
    count: usize,
    points: usize,
    cancel: &'b AtomicBool,
}
impl<F: FnMut(&str) -> Result<Font, String>> Work<'_, '_, F> {
    fn add(&mut self, m: Affine, p: V3) -> Result<(), String> {
        self.points += 1;
        if self.points.is_multiple_of(1024) {
            crate::input::check(self.cancel)?;
        }
        if self.points > 2_000_000 {
            return Err("Te veel profielgrenspunten".into());
        }
        let p = m.checked_point(p)?;
        if let Some(b) = &mut self.bounds {
            b.add(p)
        } else {
            self.bounds = Some(Bounds { min: p, max: p });
        }
        Ok(())
    }
    fn entity(&mut self, r: RecordView<'_>, m: Affine, depth: usize) -> Result<(), String> {
        crate::input::check(self.cancel)?;
        self.count += 1;
        if depth > 32 || self.count > 200_000 {
            return Err("Te veel geneste profielgrensobjecten".into());
        }
        match r.kind() {
            "INSERT" => {
                let name = crate::dxf_symbols::key(r.first(2).ok_or("Profielblok mist naam")?);
                if !self.stack.insert(name.clone()) {
                    return Err("Cyclische profielblokverwijzing".into());
                }
                let index = self.index;
                let block = index
                    .blocks
                    .get(&name)
                    .ok_or("Profielblokdefinitie ontbreekt")?;
                let placement = Placement {
                    insert: point(r, 10, [0.; 3])?,
                    scale: [number(r, 41, 1.)?, number(r, 42, 1.)?, number(r, 43, 1.)?],
                    rotation: number(r, 50, 0.)?,
                    normal: point(r, 210, [0., 0., 1.])?,
                    columns: integer(r, 70, 1, u32::MAX)?,
                    rows: integer(r, 71, 1, u32::MAX)?,
                    column_spacing: number(r, 44, 0.)?,
                    row_spacing: number(r, 45, 0.)?,
                };
                placement.validate()?;
                // Generated profile inserts are uniform. Nonuniform text needs
                // virtual-entity TEXT regeneration, not affine corner guesses.
                if (placement.scale[0].abs() - placement.scale[1].abs()).abs() > 1e-12 {
                    return Err(
                        "Niet-uniform profielblok vraagt aanvullende teksttransformatie".into(),
                    );
                }
                if placement.grid_size() > 10_000 {
                    return Err("Te groot profielblokrooster".into());
                }
                if let Some(children) = r.handle().and_then(|h| self.index.children.get(&h)) {
                    for &child in children {
                        if child.kind() == "ATTRIB" {
                            self.entity(child, m, depth + 1)?;
                        }
                    }
                }
                let rows = if placement.row_spacing == 0. {
                    1
                } else {
                    placement.rows
                };
                let columns = if placement.column_spacing == 0. {
                    1
                } else {
                    placement.columns
                };
                for row in 0..rows {
                    for col in 0..columns {
                        let transform = m.compose(placement.matrix(block.base, row, col)?)?;
                        for &child in &block.records {
                            if child.kind() != "ATTDEF" {
                                self.entity(child, transform, depth + 1)?;
                            }
                        }
                    }
                }
                self.stack.remove(&name);
            }
            "TEXT" | "ATTRIB" | "ATTDEF" => {
                let style = crate::dxf_symbols::key(r.first(7).unwrap_or("Standard"));
                let name = self.index.styles.get(&style).map_or("txt", String::as_str);
                if !self.fonts.contains_key(name) {
                    if self.fonts.len() >= 32 {
                        return Err("Te veel profiellettertypen".into());
                    }
                    let font = (self.resolve)(name)?;
                    self.font_bytes = self
                        .font_bytes
                        .checked_add(font.source_bytes())
                        .ok_or("Lettertypebudget overschreden")?;
                    if self.font_bytes > 256 * 1024 * 1024 {
                        return Err("Lettertypebudget overschreden".into());
                    }
                    self.fonts.insert(name.to_owned(), font);
                }
                let text = Text {
                    content: r
                        .first(if r.kind() == "ATTDEF" { 2 } else { 1 })
                        .unwrap_or("")
                        .into(),
                    insert: point(r, 10, [0.; 3])?,
                    align_point: r.first(11).map(|_| point(r, 11, [0.; 3])).transpose()?,
                    height: number(r, 40, 1.)?,
                    width: number(r, 41, 1.)?,
                    rotation: number(r, 50, 0.)?,
                    oblique: number(r, 51, 0.)?,
                    halign: integer(r, 72, 0, 5)? as u8,
                    valign: integer(r, if r.kind() == "TEXT" { 73 } else { 74 }, 0, 3)? as u8,
                    generation: integer(r, 71, 0, 255)? as u8,
                    normal: point(r, 210, [0., 0., 1.])?,
                };
                if let Some(corners) = self
                    .fonts
                    .get_mut(name)
                    .unwrap()
                    .extent(&text, self.cancel)?
                    .corners
                {
                    for p in corners {
                        self.add(m, p)?;
                    }
                }
            }
            "LINE" => {
                self.add(m, point(r, 10, [0.; 3])?)?;
                self.add(m, point(r, 11, [0.; 3])?)?;
            }
            "POINT" => {
                self.add(m, point(r, 10, [0.; 3])?)?;
            }
            "SOLID" | "TRACE" | "3DFACE" => {
                let transform = if r.kind() == "3DFACE" {
                    m
                } else {
                    m.compose(Affine::ocs(point(r, 210, [0., 0., 1.])?)?)?
                };
                for c in 10..=13 {
                    self.add(transform, point(r, c, [0.; 3])?)?;
                }
            }
            "LWPOLYLINE" => {
                if r.tags()
                    .any(|(c, v)| matches!(c, 40..=43) && v.trim().parse::<f64>() != Ok(0.))
                {
                    return Err(
                        "Gebogen of brede profielpolyline vraagt aanvullende grensgeometrie".into(),
                    );
                }
                let transform = m.compose(Affine::ocs(point(r, 210, [0., 0., 1.])?)?)?;
                let elevation = number(r, 38, 0.)?;
                for mut p in vertices(r, 10, self.cancel)? {
                    p[2] = elevation;
                    self.add(transform, p)?;
                }
            }
            "POLYLINE" => {
                let flags = integer(r, 70, 0, u32::MAX)?;
                if flags & !(1 | 8) != 0 {
                    return Err("Niet-ondersteund profielpolyline-type".into());
                }
                let transform = if flags & 8 == 0 {
                    m.compose(Affine::ocs(point(r, 210, [0., 0., 1.])?)?)?
                } else {
                    m
                };
                let children = r
                    .handle()
                    .and_then(|h| self.index.children.get(&h))
                    .ok_or("Profielpolyline mist vertices")?;
                for &child in children {
                    if child.kind() == "VERTEX" {
                        if number(child, 42, 0.)? != 0.
                            || number(child, 40, 0.)? != 0.
                            || number(child, 41, 0.)? != 0.
                        {
                            return Err("Gebogen of brede profielvertex".into());
                        }
                        let mut p = point(child, 10, [0.; 3])?;
                        if flags & 8 == 0 {
                            p[2] = number(r, 30, 0.)?;
                        }
                        self.add(transform, p)?;
                    }
                }
            }
            "LEADER" => {
                if number(r, 71, 1.)? != 0.
                    || number(r, 72, 0.)? != 0.
                    || number(r, 73, 0.)? == 0. && number(r, 75, 0.)? != 0.
                {
                    return Err("Profielleader vraagt aanvullende pijl-/splinegeometrie".into());
                }
                for p in vertices(r, 10, self.cancel)? {
                    self.add(m, p)?;
                }
            }
            "CIRCLE" | "ARC" => {
                let center = point(r, 10, [0.; 3])?;
                let radius = number(r, 40, 0.)?;
                if radius <= 0. {
                    return Err("Ongeldige profielcirkel".into());
                }
                let transform = m.compose(Affine::ocs(point(r, 210, [0., 0., 1.])?)?)?;
                let start = if r.kind() == "ARC" {
                    number(r, 50, 0.)?.to_radians()
                } else {
                    0.
                };
                let sweep = if r.kind() == "ARC" {
                    let end = number(r, 51, 0.)?.to_radians();
                    if end == start {
                        return Ok(());
                    }
                    let span = (end - start).rem_euclid(std::f64::consts::TAU);
                    if span == 0. {
                        std::f64::consts::TAU
                    } else {
                        span
                    }
                } else {
                    std::f64::consts::TAU
                };
                let segments = (sweep / std::f64::consts::FRAC_PI_2).ceil().max(1.) as usize;
                let step = sweep / segments as f64;
                for i in 0..segments {
                    let a = start + step * i as f64;
                    let b = a + step;
                    let k = 4. / 3. * (step / 4.).tan();
                    let (sa, ca) = a.sin_cos();
                    let (sb, cb) = b.sin_cos();
                    for p in [
                        [ca, sa],
                        [ca - k * sa, sa + k * ca],
                        [cb + k * sb, sb - k * cb],
                        [cb, sb],
                    ] {
                        self.add(
                            transform,
                            g::add(center, [radius * p[0], radius * p[1], 0.]),
                        )?;
                    }
                }
            }
            "IMAGE" => {
                let origin = point(r, 10, [0.; 3])?;
                let u = point(r, 11, [0.; 3])?;
                let v = point(r, 12, [0.; 3])?;
                let size = point(r, 13, [0.; 3])?;
                let mut points = vertices(r, 14, self.cancel)?;
                if points.len() == 2 {
                    let (a, b) = (points[0], points[1]);
                    points = vec![a, [b[0], a[1], 0.], b, [a[0], b[1], 0.]];
                }
                if points.is_empty() {
                    return Err("Profielafbeelding mist rastergrens".into());
                }
                let origin = g::add(origin, g::sub(g::scale(u, 0.5), g::scale(v, 0.5)));
                for p in points {
                    self.add(
                        m,
                        g::add(
                            origin,
                            g::add(g::scale(u, p[0]), g::scale(v, size[1] - p[1])),
                        ),
                    )?;
                }
            }
            "SEQEND" => {}
            other => return Err(format!("Profielgrens ondersteunt {other} nog niet")),
        }
        Ok(())
    }
}
