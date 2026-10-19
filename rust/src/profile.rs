//! Independent profile geometry, matching the original V9 template pipeline.
//! Features remain borrowed by index: no copied datasets or polyline vertices.
use crate::python_values::{casefold, trim, uppercase};
use crate::{
    model::{Dataset, Feature, Geometry, Vertex},
    Rule,
};
use std::{borrow::Cow, sync::atomic::AtomicBool};
include!("profile_unicode.rs");

pub type XY = [f64; 2];

#[derive(Clone, Copy)]
pub struct Options<'a> {
    pub roads: &'a [Vec<XY>],
    pub terrain_boundaries: &'a [Vec<XY>],
    pub forced_start: Option<XY>,
    pub reverse: bool,
    pub marker_scale: f64,
}
impl Default for Options<'_> {
    fn default() -> Self {
        Self {
            roads: &[],
            terrain_boundaries: &[],
            forced_start: None,
            reverse: false,
            marker_scale: 0.1,
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct ProfilePoint<'a> {
    pub feature_index: usize,
    pub position: Vertex,
    pub chainage: f64,
    pub point_z: f64,
    pub layer_name: Cow<'a, str>,
    pub color: u16,
    pub description: String,
    pub is_endpoint: bool,
}
#[derive(Debug, serde::Serialize)]
pub struct Profile<'a> {
    pub start: Vertex,
    pub end: Vertex,
    pub axis_dx: f64,
    pub axis_dy: f64,
    pub axis_length: f64,
    pub reference_level: f64,
    pub points: Vec<ProfilePoint<'a>>,
}
impl Profile<'_> {
    /// Project a measured vertex into a profile drawing without changing its Z.
    pub fn project(
        &self,
        vertex: Vertex,
        start_x: f64,
        reference_y: f64,
        display_min: f64,
        horizontal_scale: f64,
        vertical_scale: f64,
    ) -> Option<XY> {
        if self.axis_length <= 1e-9 {
            return None;
        }
        let z = vertex.z?;
        let chainage = ((vertex.x - self.start.x) * self.axis_dx
            + (vertex.y - self.start.y) * self.axis_dy)
            / self.axis_length;
        let result = [
            start_x + (chainage - display_min) * horizontal_scale,
            reference_y + (z - self.reference_level) * vertical_scale,
        ];
        result.iter().all(|v| v.is_finite()).then_some(result)
    }
    pub fn visible_range(&self) -> XY {
        self.points
            .iter()
            .filter(|p| !p.is_endpoint)
            .fold([0., self.axis_length], |range, p| {
                [range[0].min(p.chainage), range[1].max(p.chainage)]
            })
    }
    pub fn terrain_z(&self, chainage: f64) -> f64 {
        let start = self.start.z.unwrap_or(0.);
        if self.axis_length <= 1e-9 {
            return start;
        }
        let ratio = chainage.clamp(0., self.axis_length) / self.axis_length;
        start + (self.end.z.unwrap_or(0.) - start) * ratio
    }
}

fn same(a: Vertex, b: Vertex) -> bool {
    (a.x - b.x).abs() <= 1e-6 && (a.y - b.y).abs() <= 1e-6
}
fn numeric(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| {
            let n = c as u32;
            DIGIT_RANGES
                .get(DIGIT_RANGES.partition_point(|(_, end)| *end < n))
                .is_some_and(|(start, _)| *start <= n)
        })
}
fn attribute3(f: &Feature) -> &str {
    trim(&f.attributes[2]).trim_matches(['\'', '"'])
}
fn double(f: &Feature) -> bool {
    attribute3(f)
        .split(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}' | ',' | ';'))
        .any(|word| word.eq_ignore_ascii_case("dubbel"))
}
/// Python re.search(r"[-+]?\d+(?:[.,]\d+)?") with frozen Unicode 14 digits.
pub fn diameter_mm(f: &Feature) -> Option<f64> {
    first_number(trim(&f.attributes[1]), true)
}
pub(crate) fn first_number(raw: &str, signed: bool) -> Option<f64> {
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        let mut text = String::new();
        if let Some(d) = crate::object_layout::decimal(ch) {
            text.push(char::from(b'0' + d as u8));
        } else if signed
            && matches!(ch, '-' | '+')
            && chars
                .peek()
                .and_then(|c| crate::object_layout::decimal(*c))
                .is_some()
        {
            text.push(ch);
        } else {
            continue;
        }
        while let Some(d) = chars.peek().and_then(|c| crate::object_layout::decimal(*c)) {
            text.push(char::from(b'0' + d as u8));
            chars.next();
        }
        if chars.peek().is_some_and(|c| matches!(c, '.' | ',')) {
            chars.next();
            if chars
                .peek()
                .and_then(|c| crate::object_layout::decimal(*c))
                .is_some()
            {
                text.push('.');
                while let Some(d) = chars.peek().and_then(|c| crate::object_layout::decimal(*c)) {
                    text.push(char::from(b'0' + d as u8));
                    chars.next();
                }
            }
        }
        return text.parse().ok();
    }
    None
}
pub fn marker_scale(f: &Feature, fallback: f64) -> f64 {
    diameter_mm(f)
        .filter(|v| *v > 0.)
        .map_or(fallback, |v| v / 1000.)
        .max(0.005)
}
pub fn marker_offsets(f: &Feature, scale: f64) -> Vec<f64> {
    if double(f) {
        let offset = (scale * 0.58).max(0.01);
        vec![-offset, offset]
    } else {
        vec![0.]
    }
}
pub fn description(f: &Feature, layer: &str, configured: &str) -> String {
    let custom = trim(configured);
    let upper_layer = uppercase(layer);
    let upper_source = uppercase(trim(&f.source_name));
    let mut result = if !custom.is_empty() {
        custom.to_owned()
    } else if upper_layer.contains("WARM") || upper_source.contains("WARM") {
        "Warmtenet".into()
    } else if layer == "0" {
        "ONBEKEND".into()
    } else {
        [
            ("GLASVEZEL", "Glasvezel"),
            ("DATA", "Datakabel"),
            ("WATER", "Waterleiding"),
            ("ET_LS", "Laagspanning"),
            ("ET_MS", "Middenspanning"),
            ("GAS_LD", "Gasleiding LD"),
            ("GAS_HD", "Gasleiding HD"),
            ("MONITORING", "Monitoring"),
            ("VRIJVERVAL", "Riool vrijverval"),
            ("DRUK", "Riool druk"),
            ("GEVAARLIJKE", "Buisleiding GS"),
        ]
        .iter()
        .find(|(part, _)| upper_layer.contains(part))
        .map_or_else(
            || {
                if upper_source.is_empty() {
                    layer.into()
                } else {
                    upper_source
                }
            },
            |(_, text)| (*text).into(),
        )
    };
    if matches!(f.geometry, Geometry::Polyline(_)) && attribute3(f).eq_ignore_ascii_case("dekband")
    {
        if !trim(&f.source_name).is_empty() {
            result = trim(&f.source_name).into();
        }
        if !casefold(&result).ends_with("dekband") {
            result.push_str(" dekband");
        }
    }
    result
}

fn position(f: &Feature, cancel: &AtomicBool) -> Result<Option<Vertex>, String> {
    if let Geometry::Point(p) = f.geometry {
        return Ok(p.z.map(|_| p));
    }
    let Geometry::Polyline(vertices) = &f.geometry else {
        unreachable!()
    };
    let mut previous = None;
    let mut count = 0;
    let mut length = 0.;
    for (i, p) in vertices.iter().enumerate() {
        if i % 1024 == 0 {
            crate::input::check(cancel)?;
        }
        if p.z.is_none() {
            continue;
        }
        if let Some(a) = previous {
            let a: Vertex = a;
            length += (p.x - a.x).hypot(p.y - a.y);
        }
        previous = Some(*p);
        count += 1;
    }
    if count < 2 {
        return Ok(None);
    }
    if !length.is_finite() {
        return Err("Profielgeometrie is te groot".into());
    }
    if length <= 1e-9 {
        return Ok(vertices
            .iter()
            .filter(|p| p.z.is_some())
            .nth(count / 2)
            .copied());
    }
    let target = length * 0.5;
    let mut traversed = 0.;
    previous = None;
    for (i, p) in vertices.iter().enumerate() {
        if i % 1024 == 0 {
            crate::input::check(cancel)?;
        }
        if p.z.is_none() {
            continue;
        }
        if let Some(a) = previous {
            let a: Vertex = a;
            let segment = (p.x - a.x).hypot(p.y - a.y);
            if traversed + segment >= target && segment > 1e-9 {
                let ratio = (target - traversed) / segment;
                return Ok(Some(Vertex {
                    x: a.x + (p.x - a.x) * ratio,
                    y: a.y + (p.y - a.y) * ratio,
                    z: Some(a.z.unwrap() + (p.z.unwrap() - a.z.unwrap()) * ratio),
                }));
            }
            traversed += segment;
        }
        previous = Some(*p);
    }
    Ok(previous)
}

fn path_projection(
    p: XY,
    paths: &[Vec<XY>],
    perpendicular: bool,
    cancel: &AtomicBool,
) -> Result<Option<(XY, f64)>, String> {
    let mut best = None;
    let mut best_score = f64::INFINITY;
    let mut count = 0;
    for path in paths {
        crate::input::check(cancel)?;
        for segment in path.windows(2) {
            if count % 1024 == 0 {
                crate::input::check(cancel)?;
            }
            count += 1;
            let [a, b] = [segment[0], segment[1]];
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let length_sq = dx * dx + dy * dy;
            if !length_sq.is_finite() {
                return Err("Profielcontext is te groot".into());
            }
            let degenerate = if perpendicular {
                length_sq <= 1e-12
            } else {
                dx.abs() <= 1e-9 && dy.abs() <= 1e-9
            };
            let q = if degenerate {
                if perpendicular {
                    continue;
                } else {
                    a
                }
            } else {
                let factor = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length_sq;
                if perpendicular && !(0. ..=1.).contains(&factor) {
                    continue;
                }
                let factor = factor.clamp(0., 1.);
                [a[0] + factor * dx, a[1] + factor * dy]
            };
            let distance = (q[0] - p[0]).hypot(q[1] - p[1]);
            if !distance.is_finite() {
                return Err("Profielcontext is te groot".into());
            }
            // The original perpendicular search compares squared distances,
            // before hypot rounds two almost equal distances to the same f64.
            let score = if perpendicular {
                (q[0] - p[0]) * (q[0] - p[0]) + (q[1] - p[1]) * (q[1] - p[1])
            } else {
                distance
            };
            if best.is_none() || score < best_score {
                best = Some((q, distance));
                best_score = score;
            }
        }
    }
    Ok(best)
}

struct Entry<'a> {
    index: usize,
    position: Vertex,
    rule: Option<&'a Rule>,
}

pub fn build<'a>(
    dataset: &Dataset,
    rules: &'a [Rule],
    options: Options<'_>,
) -> Result<Option<Profile<'a>>, String> {
    build_with_cancel(dataset, rules, options, &crate::input::NEVER_CANCEL)
}
pub fn build_with_cancel<'a>(
    dataset: &Dataset,
    rules: &'a [Rule],
    options: Options<'_>,
    cancel: &AtomicBool,
) -> Result<Option<Profile<'a>>, String> {
    crate::input::check(cancel)?;
    if !options.marker_scale.is_finite()
        || options.marker_scale < 0.
        || options
            .forced_start
            .is_some_and(|p| p.iter().any(|v| !v.is_finite()))
    {
        return Err("Ongeldige profielinstellingen".into());
    }
    for path in options.roads.iter().chain(options.terrain_boundaries) {
        crate::input::check(cancel)?;
        for (i, p) in path.iter().enumerate() {
            if i % 1024 == 0 {
                crate::input::check(cancel)?;
            }
            if p.iter().any(|v| !v.is_finite()) {
                return Err("Ongeldige profielcontext".into());
            }
        }
    }
    let mut entries = Vec::new();
    let mut all_points = Vec::new();
    let mut unmatched = Vec::new();
    let mut markers = Vec::new();
    // Preserve Python's point-before-polyline order, including stable equal-chainage ties.
    for polyline in [false, true] {
        for (index, f) in dataset.features.iter().enumerate() {
            if index % 256 == 0 {
                crate::input::check(cancel)?;
            }
            if matches!(f.geometry, Geometry::Polyline(_)) != polyline {
                continue;
            }
            let vertices: &[Vertex] = match &f.geometry {
                Geometry::Point(p) => std::slice::from_ref(p),
                Geometry::Polyline(p) => p,
            };
            for (i, p) in vertices.iter().enumerate() {
                if i % 1024 == 0 {
                    crate::input::check(cancel)?;
                }
                if !p.x.is_finite() || !p.y.is_finite() || p.z.is_some_and(|v| !v.is_finite()) {
                    return Err("Ongeldige profielcoördinaten".into());
                }
            }
            let Some(p) = position(f, cancel)? else {
                continue;
            };
            let rule = crate::matching_rule(&f.source_name, rules);
            if !polyline {
                all_points.push(entries.len());
                if rule.is_none_or(|r| r.target_layer == "0") {
                    unmatched.push(entries.len());
                    let source = uppercase(trim(&f.source_name));
                    if source.is_empty()
                        || numeric(&source)
                        || matches!(source.as_str(), "POINT" | "PUNT")
                    {
                        markers.push(entries.len());
                    }
                }
            }
            entries.push(Entry {
                index,
                position: p,
                rule,
            });
        }
    }
    if all_points.len() < 2 {
        return Ok(None);
    }
    let candidates = if markers.len() >= 2 {
        &markers
    } else if unmatched.len() >= 2 {
        &unmatched
    } else {
        &all_points
    };
    let mut start = candidates[0];
    let mut forced = false;
    if let Some(target) = options.forced_start {
        let mut distance = f64::INFINITY;
        for (n, &i) in candidates.iter().enumerate() {
            if n % 1024 == 0 {
                crate::input::check(cancel)?;
            }
            let p = entries[i].position;
            let d = (p.x - target[0]).hypot(p.y - target[1]);
            if d < distance {
                distance = d;
                start = i;
            }
        }
        forced = distance <= 0.5;
    }
    if !forced {
        start = candidates[0];
        let mut distance = f64::INFINITY;
        for (n, &i) in candidates.iter().enumerate() {
            if n % 1024 == 0 {
                crate::input::check(cancel)?;
            }
            let p = entries[i].position;
            let d = if options.roads.is_empty() {
                0.
            } else {
                path_projection([p.x, p.y], options.roads, false, cancel)?
                    .map_or(f64::INFINITY, |(_, d)| d)
            };
            if d < distance {
                distance = d;
                start = i;
            }
        }
    }
    let mut end = None;
    let mut distance = -1.;
    for (n, &i) in candidates.iter().enumerate() {
        if n % 1024 == 0 {
            crate::input::check(cancel)?;
        }
        let p = entries[i].position;
        let a = entries[start].position;
        if same(p, a) {
            continue;
        }
        let d = (p.x - a.x).hypot(p.y - a.y);
        if d > distance {
            distance = d;
            end = Some(i);
        }
    }
    let Some(mut end) = end else {
        return Ok(None);
    };
    if !forced {
        let a = entries[start].position;
        let b = entries[end].position;
        let middle = [(a.x + b.x) * 0.5, (a.y + b.y) * 0.5];
        let road = path_projection(middle, options.roads, true, cancel)?;
        let terrain = path_projection(middle, options.terrain_boundaries, true, cancel)?;
        let preferred = match (road, terrain) {
            (Some(r), Some(t)) => Some(if t.1 * 0.8 < r.1 { t.0 } else { r.0 }),
            (Some(r), None) => Some(r.0),
            (None, Some(t)) => Some(t.0),
            _ => None,
        };
        let length = (b.x - a.x).hypot(b.y - a.y);
        if length <= 1e-6 {
            return Ok(None);
        }
        if preferred.is_some_and(|p| {
            ((p[0] - a.x) * (b.x - a.x) + (p[1] - a.y) * (b.y - a.y)) / length > length * 0.5
        }) {
            std::mem::swap(&mut start, &mut end);
        }
        if options.reverse {
            std::mem::swap(&mut start, &mut end);
        }
    }
    let a = entries[start].position;
    let b = entries[end].position;
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length = dx.hypot(dy);
    if !length.is_finite() {
        return Err("Profielgeometrie is te groot".into());
    }
    if length <= 1e-6 {
        return Ok(None);
    }
    let chainage = |p: Vertex| ((p.x - a.x) * dx + (p.y - a.y) * dy) / length;
    let mut low = 0_f64;
    let mut high = length;
    for entry in &entries {
        crate::input::check(cancel)?;
        let f = &dataset.features[entry.index];
        match &f.geometry {
            Geometry::Point(_) => {
                let scale = marker_scale(f, options.marker_scale);
                let offset = if double(f) {
                    (scale * 0.58).max(0.01)
                } else {
                    0.
                };
                let half = scale * 0.5 + offset;
                let c = chainage(entry.position);
                low = low.min(c - half);
                high = high.max(c + half);
            }
            Geometry::Polyline(vertices) => {
                for (i, p) in vertices.iter().enumerate() {
                    if i % 1024 == 0 {
                        crate::input::check(cancel)?;
                    }
                    if p.z.is_some() {
                        let c = chainage(*p);
                        low = low.min(c);
                        high = high.max(c);
                    }
                }
            }
        }
    }
    if forced {
        low = 0.;
    }
    let axis_length = high - low;
    let synthetic_start = Vertex {
        x: a.x + dx / length * low,
        y: a.y + dy / length * low,
        z: a.z,
    };
    let synthetic_end = Vertex {
        x: a.x + dx / length * high,
        y: a.y + dy / length * high,
        z: b.z,
    };
    let mut points = Vec::with_capacity(entries.len());
    for (entry, p, c) in [
        (&entries[start], synthetic_start, 0.),
        (&entries[end], synthetic_end, axis_length),
    ] {
        points.push(ProfilePoint {
            feature_index: entry.index,
            position: p,
            chainage: c,
            point_z: p.z.unwrap(),
            layer_name: Cow::Borrowed("0"),
            color: 7,
            description: description(&dataset.features[entry.index], "0", ""),
            is_endpoint: true,
        });
    }
    for entry in entries {
        crate::input::check(cancel)?;
        if same(entry.position, a) || same(entry.position, b) {
            continue;
        }
        let f = &dataset.features[entry.index];
        let (layer, color, label) = entry.rule.map_or_else(
            || {
                if matches!(f.geometry, Geometry::Polyline(_))
                    && attribute3(f).eq_ignore_ascii_case("dekband")
                {
                    ("0", 8, "Dekband")
                } else {
                    ("0", 7, "")
                }
            },
            |r| (r.target_layer.as_ref(), r.color_aci, r.label.as_ref()),
        );
        points.push(ProfilePoint {
            feature_index: entry.index,
            position: entry.position,
            chainage: chainage(entry.position) - low,
            point_z: entry.position.z.unwrap(),
            layer_name: Cow::Borrowed(layer),
            color,
            description: description(f, layer, label),
            is_endpoint: false,
        });
    }
    if ![
        synthetic_start.x,
        synthetic_start.y,
        synthetic_end.x,
        synthetic_end.y,
        axis_length,
    ]
    .iter()
    .all(|v| v.is_finite())
        || points.iter().any(|p| !p.chainage.is_finite())
    {
        return Err("Profielgeometrie is te groot".into());
    }
    points.sort_by(|a, b| {
        a.chainage
            .partial_cmp(&b.chainage)
            .unwrap()
            .then_with(|| a.is_endpoint.cmp(&b.is_endpoint))
            .then_with(|| a.description.cmp(&b.description))
    });
    let min_z = points
        .iter()
        .map(|p| p.point_z)
        .fold(f64::INFINITY, f64::min);
    let reference_level = (((min_z * 10.).floor() / 10. - 0.04) * 100.).round_ties_even() / 100.;
    if !reference_level.is_finite() {
        return Err("Profielhoogte is te groot".into());
    }
    crate::input::check(cancel)?;
    Ok(Some(Profile {
        start: synthetic_start,
        end: synthetic_end,
        axis_dx: synthetic_end.x - synthetic_start.x,
        axis_dy: synthetic_end.y - synthetic_start.y,
        axis_length,
        reference_level,
        points,
    }))
}
