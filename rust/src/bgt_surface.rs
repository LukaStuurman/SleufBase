//! Length of the profile probes inside prepared, valid BGT geometries.
//! Intervals are unioned within a feature and added between features, matching
//! GEOS intersection length without building temporary clipped geometries.
use crate::{
    profile::{Profile, XY},
    python_values::{casefold, trim},
    Bounds,
};
use std::{collections::BTreeMap, sync::atomic::AtomicBool};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Geometry {
    Point(XY),
    MultiPoint(Vec<XY>),
    LineString(Vec<XY>),
    MultiLineString(Vec<Vec<XY>>),
    Polygon(Vec<Vec<XY>>),
    MultiPolygon(Vec<Vec<Vec<XY>>>),
    Collection(Vec<Geometry>),
}
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Feature {
    pub physical_appearance: String,
    pub geometry: Geometry,
}
struct Prepared<'a> {
    name: &'a str,
    key: String,
    bounds: Option<Bounds>,
    geometry: &'a Geometry,
}
pub struct Index<'a> {
    features: Vec<Prepared<'a>>,
}
fn bounds(
    g: &Geometry,
    count: &mut usize,
    depth: usize,
    cancel: &AtomicBool,
) -> Result<Option<Bounds>, String> {
    if depth > 16 {
        return Err("BGT-geometrie is te diep genest".into());
    }
    let mut b: Option<Bounds> = None;
    let mut point = |p: XY| -> Result<(), String> {
        *count += 1;
        if (*count).is_multiple_of(1024) {
            crate::input::check(cancel)?;
        }
        if *count > 2_000_000 || p.iter().any(|v| !v.is_finite() || v.abs() > 1e100) {
            return Err("Ongeldige of te grote BGT-geometrie".into());
        }
        if let Some(b) = &mut b {
            b.min_x = b.min_x.min(p[0]);
            b.min_y = b.min_y.min(p[1]);
            b.max_x = b.max_x.max(p[0]);
            b.max_y = b.max_y.max(p[1]);
        } else {
            b = Some(Bounds {
                min_x: p[0],
                min_y: p[1],
                max_x: p[0],
                max_y: p[1],
            });
        }
        Ok(())
    };
    match g {
        Geometry::Point(p) => point(*p)?,
        Geometry::MultiPoint(p) | Geometry::LineString(p) => {
            for &p in p {
                point(p)?;
            }
        }
        Geometry::MultiLineString(p) | Geometry::Polygon(p) => {
            for p in p {
                for &p in p {
                    point(p)?;
                }
            }
        }
        Geometry::MultiPolygon(p) => {
            for p in p {
                for p in p {
                    for &p in p {
                        point(p)?;
                    }
                }
            }
        }
        Geometry::Collection(items) => {
            for item in items {
                if let Some(next) = bounds(item, count, depth + 1, cancel)? {
                    if let Some(b) = &mut b {
                        b.min_x = b.min_x.min(next.min_x);
                        b.min_y = b.min_y.min(next.min_y);
                        b.max_x = b.max_x.max(next.max_x);
                        b.max_y = b.max_y.max(next.max_y);
                    } else {
                        b = Some(next);
                    }
                }
            }
        }
    }
    Ok(b)
}
fn orient(a: XY, b: XY, c: XY) -> f64 {
    robust::orient2d(
        robust::Coord { x: a[0], y: a[1] },
        robust::Coord { x: b[0], y: b[1] },
        robust::Coord { x: c[0], y: c[1] },
    )
}
fn side(a: f64, b: f64) -> bool {
    a == 0. || b == 0. || a.is_sign_positive() != b.is_sign_positive()
}
struct Probe {
    start: XY,
    end: XY,
    d: XY,
    length: f64,
    component: usize,
    bounds: Bounds,
}
impl Probe {
    fn new(start: XY, end: XY) -> Result<Self, String> {
        if start
            .iter()
            .chain(end.iter())
            .any(|v| !v.is_finite() || v.abs() > 1e100)
        {
            return Err("Ongeldige BGT-proeflijn".into());
        }
        let d = [end[0] - start[0], end[1] - start[1]];
        let length = d[0].hypot(d[1]);
        Ok(Self {
            start,
            end,
            d,
            length,
            component: usize::from(d[1].abs() > d[0].abs()),
            bounds: Bounds {
                min_x: start[0].min(end[0]),
                max_x: start[0].max(end[0]),
                min_y: start[1].min(end[1]),
                max_y: start[1].max(end[1]),
            },
        })
    }
    fn parameter(&self, p: XY) -> f64 {
        (p[self.component] - self.start[self.component]) / self.d[self.component]
    }
    fn point(&self, t: f64) -> XY {
        if t == 0. {
            self.start
        } else if t == 1. {
            self.end
        } else {
            [self.start[0] + self.d[0] * t, self.start[1] + self.d[1] * t]
        }
    }
    fn length_between(&self, piece: [f64; 2]) -> f64 {
        let a = self.point(piece[0]);
        let b = self.point(piece[1]);
        (b[0] - a[0]).hypot(b[1] - a[1])
    }
    fn intersection(&self, a: XY, b: XY) -> Option<[f64; 2]> {
        let oa = orient(self.start, self.end, a);
        let ob = orient(self.start, self.end, b);
        if oa == 0. && ob == 0. {
            let x = self.parameter(a);
            let y = self.parameter(b);
            let low = x.min(y).max(0.);
            let high = x.max(y).min(1.);
            return (low <= high).then_some([low, high]);
        }
        if !side(oa, ob) || !side(orient(a, b, self.start), orient(a, b, self.end)) {
            return None;
        }
        let edge = [b[0] - a[0], b[1] - a[1]];
        let denominator = orient([0., 0.], self.d, edge);
        if denominator == 0. {
            return None;
        }
        let t = (orient(self.start, a, b) / denominator).clamp(0., 1.);
        Some([t, t])
    }
}
fn intervals(
    g: &Geometry,
    p: &Probe,
    out: &mut Vec<[f64; 2]>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    crate::input::check(cancel)?;
    match g {
        Geometry::Polygon(rings) => polygon_intervals(rings, p, out, cancel)?,
        Geometry::MultiPolygon(polygons) => {
            for rings in polygons {
                polygon_intervals(rings, p, out, cancel)?;
            }
        }
        Geometry::LineString(line) => line_intervals(line, p, out, cancel)?,
        Geometry::MultiLineString(lines) => {
            for line in lines {
                line_intervals(line, p, out, cancel)?;
            }
        }
        Geometry::Collection(items) => {
            for item in items {
                intervals(item, p, out, cancel)?;
            }
        }
        Geometry::Point(_) | Geometry::MultiPoint(_) => {}
    }
    Ok(())
}
fn line_intervals(
    line: &[XY],
    p: &Probe,
    out: &mut Vec<[f64; 2]>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    for (i, pair) in line.windows(2).enumerate() {
        if i % 1024 == 0 {
            crate::input::check(cancel)?;
        }
        if let Some([a, b]) = p.intersection(pair[0], pair[1]) {
            if a < b {
                out.push([a, b]);
            }
        }
    }
    Ok(())
}
fn union(pieces: &mut Vec<[f64; 2]>) {
    pieces.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    let mut kept = 0;
    for i in 0..pieces.len() {
        let next = pieces[i];
        if next[0] >= next[1] {
            continue;
        }
        if kept > 0 && next[0] <= pieces[kept - 1][1] {
            pieces[kept - 1][1] = pieces[kept - 1][1].max(next[1]);
        } else {
            pieces[kept] = next;
            kept += 1;
        }
    }
    pieces.truncate(kept);
}
fn subtract(source: &[[f64; 2]], holes: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    let mut j = 0;
    for &[start, end] in source {
        let mut cursor = start;
        while j < holes.len() && holes[j][1] <= cursor {
            j += 1;
        }
        let mut k = j;
        while k < holes.len() && holes[k][0] < end {
            if holes[k][0] > cursor {
                out.push([cursor, holes[k][0].min(end)]);
            }
            cursor = cursor.max(holes[k][1]);
            if cursor >= end {
                break;
            }
            k += 1;
        }
        if cursor < end {
            out.push([cursor, end]);
        }
    }
    out
}
fn ring_intervals(
    ring: &[XY],
    p: &Probe,
    strict: bool,
    cancel: &AtomicBool,
) -> Result<Vec<[f64; 2]>, String> {
    let mut crossings = Vec::new();
    let mut boundary = Vec::new();
    for i in 0..ring.len() {
        if i % 1024 == 0 {
            crate::input::check(cancel)?;
        }
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        let sa = orient(p.start, p.end, a);
        let sb = orient(p.start, p.end, b);
        if sa == 0. && sb == 0. {
            if let Some(piece) = p.intersection(a, b) {
                boundary.push(piece);
            }
        } else if (sa > 0.) != (sb > 0.) {
            let denominator = orient([0., 0.], p.d, [b[0] - a[0], b[1] - a[1]]);
            if denominator != 0. {
                crossings.push(orient(p.start, a, b) / denominator);
            }
        }
    }
    crossings.sort_by(f64::total_cmp);
    let mut inside = Vec::new();
    for pair in crossings.as_chunks::<2>().0 {
        let a = pair[0].max(0.);
        let b = pair[1].min(1.);
        if a < b {
            inside.push([a, b]);
        }
    }
    union(&mut boundary);
    union(&mut inside);
    if strict {
        Ok(subtract(&inside, &boundary))
    } else {
        inside.extend(boundary);
        union(&mut inside);
        Ok(inside)
    }
}
fn polygon_intervals(
    rings: &[Vec<XY>],
    p: &Probe,
    out: &mut Vec<[f64; 2]>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let Some(exterior) = rings.first() else {
        return Ok(());
    };
    let exterior = ring_intervals(exterior, p, false, cancel)?;
    let mut holes = Vec::new();
    for ring in &rings[1..] {
        holes.extend(ring_intervals(ring, p, true, cancel)?);
    }
    union(&mut holes);
    out.extend(subtract(&exterior, &holes));
    Ok(())
}
impl<'a> Index<'a> {
    pub fn new(features: &'a [Feature], cancel: &AtomicBool) -> Result<Self, String> {
        if features.len() > 100_000 {
            return Err("Te veel BGT-oppervlakken".into());
        }
        let mut prepared = Vec::with_capacity(features.len());
        let mut count = 0;
        for feature in features {
            crate::input::check(cancel)?;
            let name = trim(&feature.physical_appearance);
            if name.len() > 4096 || name.chars().any(char::is_control) {
                return Err("Ongeldige BGT-oppervlaktekst".into());
            }
            if name.is_empty() {
                continue;
            }
            prepared.push(Prepared {
                name,
                key: casefold(name),
                bounds: bounds(&feature.geometry, &mut count, 0, cancel)?,
                geometry: &feature.geometry,
            });
        }
        Ok(Self { features: prepared })
    }
    pub fn dominant(
        &self,
        start: XY,
        end: XY,
        cancel: &AtomicBool,
    ) -> Result<Option<&'a str>, String> {
        crate::input::check(cancel)?;
        let p = Probe::new(start, end)?;
        if p.length == 0. {
            return Ok(None);
        }
        let mut lengths: BTreeMap<&str, (f64, &str)> = BTreeMap::new();
        let mut pieces = Vec::new();
        for (i, f) in self.features.iter().enumerate() {
            if i % 256 == 0 {
                crate::input::check(cancel)?;
            }
            let Some(b) = f.bounds else {
                continue;
            };
            if b.max_x < p.bounds.min_x
                || b.min_x > p.bounds.max_x
                || b.max_y < p.bounds.min_y
                || b.min_y > p.bounds.max_y
            {
                continue;
            }
            pieces.clear();
            intervals(f.geometry, &p, &mut pieces, cancel)?;
            pieces.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
            let mut length = 0.;
            let mut current: Option<[f64; 2]> = None;
            for &piece in &pieces {
                if let Some(c) = &mut current {
                    if piece[0] <= c[1] {
                        c[1] = c[1].max(piece[1]);
                    } else {
                        length += p.length_between(*c);
                        *c = piece;
                    }
                } else {
                    current = Some(piece);
                }
            }
            if let Some(c) = current {
                length += p.length_between(c);
            }
            if length <= 1e-8 {
                continue;
            }
            let vote = lengths.entry(&f.key).or_insert((0., f.name));
            vote.0 += length;
        }
        Ok(lengths
            .into_iter()
            .min_by(|a, b| b.1 .0.total_cmp(&a.1 .0).then(a.0.cmp(b.0)))
            .map(|(_, (_, name))| name))
    }
    pub fn texts(
        &self,
        profile: &Profile<'_>,
        cancel: &AtomicBool,
    ) -> Result<serde_json::Value, String> {
        crate::input::check(cancel)?;
        let mut values = serde_json::Map::new();
        if profile.axis_length <= 1e-9 || self.features.is_empty() {
            return Ok(values.into());
        }
        if !profile.axis_length.is_finite() {
            return Err("Ongeldige profielas".into());
        }
        let start = [profile.start.x, profile.start.y];
        let end = [profile.end.x, profile.end.y];
        let d = [
            (end[0] - start[0]) / profile.axis_length,
            (end[1] - start[1]) / profile.axis_length,
        ];
        for (key, a, b) in [
            ("start", [start[0] - d[0], start[1] - d[1]], start),
            ("middle", start, end),
            ("end", end, [end[0] + d[0], end[1] + d[1]]),
        ] {
            if let Some(text) = self.dominant(a, b, cancel)? {
                values.insert(key.into(), text.into());
            }
        }
        Ok(values.into())
    }
}
