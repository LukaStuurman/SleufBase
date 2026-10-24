//! Native BGT linework reconstruction, polygon operations and clipping.
use crate::{bgt_surface::Geometry, Bounds};
use crate::{input, profile::XY};
use i_overlay::{
    core::{fill_rule::FillRule, overlay_rule::OverlayRule, solver::Solver},
    float::{
        clip::FloatClip,
        overlay::{FloatOverlay, OverlayOptions},
        simplify::SimplifyShape,
        slice::FloatSlice,
    },
    string::clip::ClipRule,
};
use std::{collections::HashMap, sync::atomic::AtomicBool};
pub type Paths = Vec<Vec<XY>>;
pub type Areas = Vec<Paths>;
fn options() -> OverlayOptions<f64, i64> {
    let mut o = OverlayOptions::ogc();
    o.preserve_input_collinear = true;
    o.preserve_output_collinear = true;
    o.clean_result = false;
    o
}
/// Restore unchanged input vertices by their EXACT engine roundtrip key. This
/// is not a distance snap: new intersections and ambiguous keys stay untouched.
/// Without this, repeated float adapters can turn a coincident boundary into a
/// long, one-ULP-wide gap and leave spurious lines after polygon subtraction.
fn restore(paths: &mut Paths, source: Vec<&XY>) {
    use i_overlay::i_float::adapter::FloatPointAdapter;
    let adapter = FloatPointAdapter::<XY, i64>::with_iter_conservative(source.iter().copied());
    let mut originals = HashMap::<[u64; 2], Option<XY>>::new();
    for &p in source {
        let key = coordinate_key(adapter.int_to_float(&adapter.float_to_int(&p)));
        originals
            .entry(key)
            .and_modify(|v| {
                if *v != Some(p) {
                    *v = None;
                }
            })
            .or_insert(Some(p));
    }
    for p in paths.iter_mut().flatten() {
        if let Some(Some(original)) = originals.get(&coordinate_key(*p)) {
            *p = *original;
        }
    }
}
fn restore_areas(areas: &mut Areas, source: Vec<&XY>) {
    let lengths = areas.iter().map(Vec::len).collect::<Vec<_>>();
    let mut paths = std::mem::take(areas)
        .into_iter()
        .flatten()
        .collect::<Paths>();
    restore(&mut paths, source);
    let mut paths = paths.into_iter();
    *areas = lengths
        .into_iter()
        .map(|n| paths.by_ref().take(n).collect())
        .collect();
}
fn area_edges(areas: &Areas) -> Vec<[XY; 2]> {
    areas
        .iter()
        .flatten()
        .flat_map(|ring| {
            ring.iter()
                .zip(ring.iter().cycle().skip(1))
                .take(ring.len())
                .filter(|(a, b)| a != b)
                .map(|(&a, &b)| [a, b])
        })
        .collect()
}
/// Recompute newly constructed overlay nodes from their incident source
/// segments in extended precision. Existing vertices are never moved. Only a
/// unique proper intersection within floating roundoff of the engine node is
/// accepted; close parallel lines are not snapped together.
fn fix_nodes<'a>(
    points: impl IntoIterator<Item = &'a mut XY>,
    edges: &[[XY; 2]],
    cancel: &AtomicBool,
) -> Result<(), String> {
    let originals = edges
        .iter()
        .flatten()
        .map(|&p| coordinate_key(p))
        .collect::<std::collections::HashSet<_>>();
    let mut work = 0usize;
    for p in points {
        if originals.contains(&coordinate_key(*p)) {
            continue;
        }
        let mut nearby = Vec::new();
        let mut tolerance: f64 = 0.;
        for &[a, b] in edges {
            work += 1;
            if work.is_multiple_of(1024) {
                input::check(cancel)?;
            }
            if work > 20_000_000 {
                return Err("Te veel BGT-snijpuntcontroles".into());
            }
            let scale = a
                .iter()
                .chain(b.iter())
                .chain(p.iter())
                .fold(1f64, |m, v| m.max(v.abs()));
            let e = scale * f64::EPSILON * 8.;
            if p[0] < a[0].min(b[0]) - e
                || p[0] > a[0].max(b[0]) + e
                || p[1] < a[1].min(b[1]) - e
                || p[1] > a[1].max(b[1]) + e
            {
                continue;
            }
            if orient(a, b, *p).abs() <= e * (b[0] - a[0]).hypot(b[1] - a[1]) {
                nearby.push([a, b]);
                tolerance = tolerance.max(e);
            }
        }
        let mut candidate = None;
        let mut ambiguous = false;
        for (i, &[a, b]) in nearby.iter().enumerate() {
            for &[c, d] in &nearby[i + 1..] {
                work += 1;
                if work.is_multiple_of(1024) {
                    input::check(cancel)?;
                }
                if work > 20_000_000 {
                    return Err("Te veel BGT-snijpuntcontroles".into());
                }
                let u = orient(a, b, c);
                let v = orient(a, b, d);
                let w = orient(c, d, a);
                let x = orient(c, d, b);
                if u == 0.
                    || v == 0.
                    || w == 0.
                    || x == 0.
                    || u.is_sign_positive() == v.is_sign_positive()
                    || w.is_sign_positive() == x.is_sign_positive()
                {
                    continue;
                }
                let q = intersection(a, b, c, d);
                if (q[0] - p[0]).abs() > tolerance || (q[1] - p[1]).abs() > tolerance {
                    continue;
                }
                if let Some(previous) = candidate {
                    if previous != q {
                        ambiguous = true;
                    }
                } else {
                    candidate = Some(q);
                }
            }
        }
        if !ambiguous {
            if let Some(q) = candidate {
                *p = q;
            }
        }
    }
    Ok(())
}
fn simplify(areas: &Areas, cancel: &AtomicBool) -> Result<Areas, String> {
    let mut result =
        areas.simplify_shape_custom_as::<i64>(FillRule::NonZero, options(), Solver::default());
    restore_areas(&mut result, areas.iter().flatten().flatten().collect());
    fix_nodes(
        result.iter_mut().flatten().flatten(),
        &area_edges(areas),
        cancel,
    )?;
    Ok(result)
}
fn clip(
    lines: &Paths,
    areas: &Areas,
    rule: ClipRule,
    cancel: &AtomicBool,
) -> Result<Paths, String> {
    let mut result = lines.clip_by_as::<i64>(areas, FillRule::NonZero, rule);
    restore(
        &mut result,
        areas
            .iter()
            .flatten()
            .flatten()
            .chain(lines.iter().flatten())
            .collect(),
    );
    let mut edges = area_edges(areas);
    edges.extend(
        lines
            .iter()
            .flat_map(|p| p.windows(2))
            .filter(|s| s[0] != s[1])
            .map(|s| [s[0], s[1]]),
    );
    fix_nodes(result.iter_mut().flatten(), &edges, cancel)?;
    Ok(result)
}
pub(crate) fn overlay(
    a: &Areas,
    b: &Areas,
    rule: OverlayRule,
    cancel: &AtomicBool,
) -> Result<Areas, String> {
    let mut result =
        FloatOverlay::<XY, i64>::from_subj_and_clip_custom(a, b, options(), Solver::default())
            .overlay(rule, FillRule::NonZero);
    restore_areas(
        &mut result,
        a.iter()
            .flatten()
            .flatten()
            .chain(b.iter().flatten().flatten())
            .collect(),
    );
    let mut edges = area_edges(a);
    edges.extend(area_edges(b));
    fix_nodes(result.iter_mut().flatten().flatten(), &edges, cancel)?;
    Ok(result)
}
fn ring_key(ring: &[XY]) -> Vec<[u64; 2]> {
    let mut k = Vec::new();
    for p in ring {
        let p = p.map(|v| if v == 0. { 0 } else { v.to_bits() });
        if k.last() != Some(&p) {
            k.push(p);
        }
    }
    while k.len() > 1 && k.first() == k.last() {
        k.pop();
    }
    if k.is_empty() {
        return k;
    }
    let at = k.iter().enumerate().min_by_key(|(_, p)| *p).unwrap().0;
    k.rotate_left(at);
    let mut reverse = k.iter().rev().copied().collect::<Vec<_>>();
    let at = reverse
        .iter()
        .enumerate()
        .min_by_key(|(_, p)| *p)
        .unwrap()
        .0;
    reverse.rotate_left(at);
    if reverse < k {
        reverse
    } else {
        k
    }
}
/// Polygonize noded linework: discard the unbounded face, select alternating
/// nested faces and dissolve adjacent faces. Closed holes disconnected from
/// their exterior alternate; chords/touching rings belong to the same face set.
fn build_area(paths: &Paths, cancel: &AtomicBool) -> Result<Areas, String> {
    input::check(cancel)?;
    let mut low = [f64::INFINITY; 2];
    let mut high = [f64::NEG_INFINITY; 2];
    for p in paths.iter().flatten() {
        for i in 0..2 {
            low[i] = low[i].min(p[i]);
            high[i] = high[i].max(p[i]);
        }
    }
    if !low[0].is_finite() {
        return Ok(Vec::new());
    }
    let pad = (high[0] - low[0]).max(high[1] - low[1]).max(1.) * 0.125 + 1.;
    let frame = vec![
        [low[0] - pad, low[1] - pad],
        [high[0] + pad, low[1] - pad],
        [high[0] + pad, high[1] + pad],
        [low[0] - pad, high[1] + pad],
    ];
    let mut faces =
        frame.slice_custom_by_as::<i64>(paths, FillRule::NonZero, options(), Solver::default());
    restore_areas(
        &mut faces,
        frame.iter().chain(paths.iter().flatten()).collect(),
    );
    input::check(cancel)?;
    let faces = faces
        .into_iter()
        .filter(|face| {
            !face[0].iter().any(|p| {
                p[0] < low[0] - pad * 0.5
                    || p[0] > high[0] + pad * 0.5
                    || p[1] < low[1] - pad * 0.5
                    || p[1] > high[1] + pad * 0.5
            })
        })
        .collect::<Areas>();
    let mut holes = HashMap::new();
    for (i, face) in faces.iter().enumerate() {
        for ring in &face[1..] {
            holes.insert(ring_key(ring), i);
        }
    }
    let parents = faces
        .iter()
        .map(|f| holes.get(&ring_key(&f[0])).copied())
        .collect::<Vec<_>>();
    // Memoize nesting depth: walking every ancestor for every face would be
    // quadratic for deeply nested rings. Visit each parent chain only once.
    let mut depths = vec![None; parents.len()];
    let mut marks = vec![0; parents.len()];
    for i in 0..parents.len() {
        if depths[i].is_some() {
            continue;
        }
        let mut path = Vec::new();
        let mut current = Some(i);
        let mut depth = 0usize;
        while let Some(p) = current {
            if let Some(known) = depths[p] {
                depth = known + 1;
                break;
            }
            if marks[p] == i + 1 {
                return Err("Cyclische BGT-vlakhiërarchie".into());
            }
            marks[p] = i + 1;
            path.push(p);
            current = parents[p];
            if path.len().is_multiple_of(1024) {
                input::check(cancel)?;
            }
        }
        for p in path.into_iter().rev() {
            depths[p] = Some(depth);
            depth += 1;
        }
    }
    let selected = faces
        .into_iter()
        .enumerate()
        .filter_map(|(i, face)| (depths[i].unwrap() % 2 == 0).then_some(face))
        .collect::<Areas>();
    if selected.is_empty() {
        Ok(selected)
    } else {
        simplify(&selected, cancel)
    }
}
#[derive(Debug, serde::Serialize)]
pub struct Repaired {
    pub areas: Areas,
    pub lines: Paths,
    pub points: Vec<XY>,
}
pub fn repair_polygon(mut lines: Paths, cancel: &AtomicBool) -> Result<Repaired, String> {
    input::check(cancel)?;
    let mut count = 0;
    for path in &mut lines {
        for p in path.iter() {
            count += 1;
            if count % 1024 == 0 {
                input::check(cancel)?;
            }
            if count > 2_000_000 || p.iter().any(|v| !v.is_finite() || v.abs() > 1e12) {
                return Err("Ongeldige BGT-reparatiegeometrie".into());
            }
        }
        if path.first() != path.last() {
            path.push(path[0]);
        }
    }
    // Compute common junctions once, before polygonization. All faces and cut
    // edges then refer to the same intersection coordinate.
    let points = lines
        .iter()
        .filter(|p| !p.is_empty() && p.iter().all(|v| v == &p[0]))
        .map(|p| p[0])
        .collect::<Vec<_>>();
    lines = node_lines(lines, cancel)?;
    let mut areas = Vec::new();
    let max_iterations = lines.iter().map(Vec::len).sum::<usize>() + 1;
    for _ in 0..max_iterations {
        input::check(cancel)?;
        let next = build_area(&lines, cancel)?;
        if next.is_empty() {
            return Ok(Repaired {
                areas,
                lines,
                points,
            });
        }
        let mut remaining = clip(
            &lines,
            &next,
            ClipRule {
                invert: false,
                boundary_included: false,
            },
            cancel,
        )?;
        remaining.extend(clip(
            &lines,
            &next,
            ClipRule {
                invert: true,
                boundary_included: true,
            },
            cancel,
        )?);
        areas = if areas.is_empty() {
            next
        } else {
            overlay(&areas, &next, OverlayRule::Xor, cancel)?
        };
        if remaining.is_empty() {
            return Ok(Repaired {
                areas,
                lines: remaining,
                points,
            });
        }
        lines = remaining;
    }
    Err("BGT-topologieherstel maakt geen voortgang".into())
}

const MAX_VERTICES: usize = 2_000_000;
pub(crate) fn vertex_count(g: &Geometry, cancel: &AtomicBool) -> Result<usize, String> {
    fn visit(
        g: &Geometry,
        depth: usize,
        count: &mut usize,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        input::check(cancel)?;
        if depth > 16 {
            return Err("BGT-geometrie is te diep genest".into());
        }
        let mut point = |p: &XY| -> Result<(), String> {
            *count += 1;
            if (*count).is_multiple_of(1024) {
                input::check(cancel)?;
            }
            if *count > MAX_VERTICES || p.iter().any(|v| !v.is_finite() || v.abs() > 1e12) {
                return Err("Ongeldige of te grote BGT-geometrie".into());
            }
            Ok(())
        };
        match g {
            Geometry::Point(p) => point(p)?,
            Geometry::MultiPoint(p) | Geometry::LineString(p) => {
                for p in p {
                    point(p)?;
                }
            }
            Geometry::MultiLineString(p) | Geometry::Polygon(p) => {
                for p in p.iter().flatten() {
                    point(p)?;
                }
            }
            Geometry::MultiPolygon(p) => {
                for p in p.iter().flatten().flatten() {
                    point(p)?;
                }
            }
            Geometry::Collection(items) => {
                for item in items {
                    visit(item, depth + 1, count, cancel)?;
                }
            }
        }
        Ok(())
    }
    let mut count = 0;
    visit(g, 0, &mut count, cancel)?;
    Ok(count)
}
fn check_paths(paths: &Paths, cancel: &AtomicBool) -> Result<(), String> {
    let mut count = 0usize;
    for p in paths.iter().flatten() {
        count += 1;
        if count.is_multiple_of(1024) {
            input::check(cancel)?;
        }
        if count > MAX_VERTICES || p.iter().any(|v| !v.is_finite() || v.abs() > 1e12) {
            return Err("Ongeldige of te grote BGT-geometrie".into());
        }
    }
    Ok(())
}
fn outside(lines: &Paths, areas: &Areas, cancel: &AtomicBool) -> Result<Paths, String> {
    if areas.is_empty() {
        return Ok(lines.clone());
    }
    // For inverted clipping the library's boundary flag is inverted too:
    // true excludes polygon boundaries, as required by geometry difference.
    clip(
        lines,
        areas,
        ClipRule {
            invert: true,
            boundary_included: true,
        },
        cancel,
    )
}
fn orient(a: XY, b: XY, c: XY) -> f64 {
    robust::orient2d(
        robust::Coord { x: a[0], y: a[1] },
        robust::Coord { x: b[0], y: b[1] },
        robust::Coord { x: c[0], y: c[1] },
    )
}
fn on_segment(p: XY, a: XY, b: XY) -> bool {
    p[0] >= a[0].min(b[0])
        && p[0] <= a[0].max(b[0])
        && p[1] >= a[1].min(b[1])
        && p[1] <= a[1].max(b[1])
        && orient(a, b, p) == 0.
}
fn ring_covers(p: XY, ring: &[XY]) -> (bool, bool) {
    let mut inside = false;
    if let Some(&last) = ring.last() {
        let mut previous = last;
        for &q in ring {
            if on_segment(p, previous, q) {
                return (true, true);
            }
            if (previous[1] > p[1]) != (q[1] > p[1])
                && ((q[1] > previous[1]) == (orient(previous, q, p) > 0.))
            {
                inside = !inside;
            }
            previous = q;
        }
    }
    (inside, false)
}
fn covered(p: XY, areas: &Areas) -> bool {
    areas.iter().any(|area| {
        let Some(shell) = area.first() else {
            return false;
        };
        let (inside, boundary) = ring_covers(p, shell);
        boundary
            || (inside
                && !area[1..].iter().any(|h| {
                    let (i, b) = ring_covers(p, h);
                    i && !b
                }))
    })
}
fn clip_segment(a: XY, b: XY, bounds: Bounds) -> Option<(XY, XY)> {
    let mut low: f64 = 0.;
    let mut high: f64 = 1.;
    let mut low_edge = None;
    let mut high_edge = None;
    for (i, min, max) in [
        (0, bounds.min_x, bounds.max_x),
        (1, bounds.min_y, bounds.max_y),
    ] {
        let d = b[i] - a[i];
        if d == 0. {
            if a[i] < min || a[i] > max {
                return None;
            }
        } else {
            let u = (min - a[i]) / d;
            let v = (max - a[i]) / d;
            if u.min(v) > low {
                low = u.min(v);
                low_edge = Some((i, if d > 0. { min } else { max }));
            }
            if u.max(v) < high {
                high = u.max(v);
                high_edge = Some((i, if d > 0. { max } else { min }));
            }
            if low > high {
                return None;
            }
        }
    }
    let at = |t: f64, edge: Option<(usize, f64)>| {
        if t == 0. {
            a
        } else if t == 1. {
            b
        } else {
            let (i, value) = edge.unwrap();
            let mut q = [0., 0.];
            q[i] = value;
            let mut qq = q;
            qq[1 - i] = 1.;
            let mut p = intersection(a, b, q, qq);
            p[i] = value;
            [
                p[0].clamp(bounds.min_x, bounds.max_x),
                p[1].clamp(bounds.min_y, bounds.max_y),
            ]
        }
    };
    Some((at(low, low_edge), at(high, high_edge)))
}
fn coordinate_key(p: XY) -> [u64; 2] {
    p.map(|v| if v == 0. { 0 } else { v.to_bits() })
}

// Compensated arithmetic for intersections: the low part retains roundoff
// from sums/products, including large RD offsets. This avoids losing a whole
// boundary vote because ordinary interpolation moved an endpoint by one ULP.
#[derive(Clone, Copy)]
struct Extended(f64, f64);
impl Extended {
    fn from(x: f64) -> Self {
        Self(x, 0.)
    }
    fn add(self, b: Self) -> Self {
        let s = self.0 + b.0;
        let v = s - self.0;
        let e = (self.0 - (s - v)) + (b.0 - v) + self.1 + b.1;
        let hi = s + e;
        Self(hi, e - (hi - s))
    }
    fn sub(self, b: Self) -> Self {
        self.add(Self(-b.0, -b.1))
    }
    fn mul(self, b: Self) -> Self {
        let p = self.0 * b.0;
        let e = self.0.mul_add(b.0, -p) + self.0 * b.1 + self.1 * b.0 + self.1 * b.1;
        let hi = p + e;
        Self(hi, e - (hi - p))
    }
    fn div(self, b: Self) -> f64 {
        let q = self.0 / b.0;
        let r = self.sub(b.mul(Self::from(q)));
        let qq = r.0 / b.0;
        let r = r.sub(b.mul(Self::from(qq)));
        let sum = Self::from(q).add(Self::from(qq)).add(Self::from(r.0 / b.0));
        sum.0 + sum.1
    }
}
fn intersection(a: XY, b: XY, c: XY, d: XY) -> XY {
    let [ax, ay] = a.map(Extended::from);
    let [bx, by] = b.map(Extended::from);
    let [cx, cy] = c.map(Extended::from);
    let [dx, dy] = d.map(Extended::from);
    let px = ay.sub(by);
    let py = bx.sub(ax);
    let pw = ax.mul(by).sub(bx.mul(ay));
    let qx = cy.sub(dy);
    let qy = dx.sub(cx);
    let qw = cx.mul(dy).sub(dx.mul(cy));
    let w = px.mul(qy).sub(qx.mul(py));
    [
        py.mul(qw).sub(qy.mul(pw)).div(w),
        qx.mul(pw).sub(px.mul(qw)).div(w),
    ]
}

/// Node intersecting linework and dissolve duplicate/overlapping segments.
/// The x sweep skips disjoint segments; a work limit bounds dense adversarial
/// intersections instead of allocating an unbounded arrangement.
fn node_lines(paths: Paths, cancel: &AtomicBool) -> Result<Paths, String> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut segments = paths
        .iter()
        .flat_map(|p| p.windows(2))
        .filter(|p| p[0] != p[1])
        .map(|p| (p[0], p[1], vec![p[0], p[1]]))
        .collect::<Vec<_>>();
    segments.sort_by(|a, b| a.0[0].min(a.1[0]).total_cmp(&b.0[0].min(b.1[0])));
    let mut work = 0usize;
    let mut nodes = segments.len() * 2;
    for i in 0..segments.len() {
        input::check(cancel)?;
        let (a, b, _) = &segments[i];
        let (a, b) = (*a, *b);
        for j in i + 1..segments.len() {
            let (c, d, _) = &segments[j];
            let (c, d) = (*c, *d);
            if c[0].min(d[0]) > a[0].max(b[0]) {
                break;
            }
            work += 1;
            if work.is_multiple_of(1024) {
                input::check(cancel)?;
            }
            if work > 20_000_000 {
                return Err("BGT-lijnwerk bevat te veel kruisingen".into());
            }
            if c[1].min(d[1]) > a[1].max(b[1]) || c[1].max(d[1]) < a[1].min(b[1]) {
                continue;
            }
            let oa = orient(a, b, c);
            let ob = orient(a, b, d);
            let oc = orient(c, d, a);
            let od = orient(c, d, b);
            let mut intersections = Vec::new();
            for (p, u, v) in [(a, c, d), (b, c, d), (c, a, b), (d, a, b)] {
                if on_segment(p, u, v) {
                    intersections.push(p);
                }
            }
            if oa != 0.
                && ob != 0.
                && oc != 0.
                && od != 0.
                && oa.is_sign_positive() != ob.is_sign_positive()
                && oc.is_sign_positive() != od.is_sign_positive()
            {
                intersections.push(intersection(a, b, c, d));
            }
            nodes += intersections.len() * 2;
            if nodes > MAX_VERTICES * 2 {
                return Err("BGT-lijnwerk na knippen te groot".into());
            }
            segments[i].2.extend(&intersections);
            segments[j].2.extend(intersections);
        }
    }
    let mut unique = BTreeSet::new();
    let mut edges = Vec::new();
    let mut graph = BTreeMap::<[u64; 2], Vec<usize>>::new();
    for (a, b, mut splits) in segments {
        let axis = usize::from((b[1] - a[1]).abs() > (b[0] - a[0]).abs());
        splits.sort_by(|p, q| p[axis].total_cmp(&q[axis]));
        splits.dedup();
        for pair in splits.windows(2) {
            let mut keys = [coordinate_key(pair[0]), coordinate_key(pair[1])];
            keys.sort();
            if keys[0] == keys[1] || !unique.insert(keys) {
                continue;
            }
            let index = edges.len();
            edges.push([pair[0], pair[1]]);
            for k in keys {
                graph.entry(k).or_default().push(index);
            }
        }
    }
    let mut used = vec![false; edges.len()];
    let mut paths = Vec::new();
    // Start at junctions/endpoints first, then consume isolated closed loops.
    let starts = graph
        .iter()
        .filter(|(_, e)| e.len() != 2)
        .flat_map(|(&p, e)| e.iter().map(move |&i| (p, i)))
        .chain(
            edges
                .iter()
                .enumerate()
                .map(|(i, e)| (coordinate_key(e[0]), i)),
        );
    for (start, mut edge) in starts {
        if used[edge] {
            continue;
        }
        let mut at = start;
        let mut path = Vec::new();
        loop {
            used[edge] = true;
            let e = edges[edge];
            let (from, to) = if coordinate_key(e[0]) == at {
                (e[0], e[1])
            } else {
                (e[1], e[0])
            };
            if path.is_empty() {
                path.push(from);
            }
            path.push(to);
            at = coordinate_key(to);
            let adjacent = &graph[&at];
            if adjacent.len() != 2 {
                break;
            }
            let Some(next) = adjacent.iter().find(|&&i| !used[i]) else {
                break;
            };
            edge = *next;
        }
        paths.push(path);
    }
    check_paths(&paths, cancel)?;
    Ok(paths)
}

fn collect(
    g: &Geometry,
    areas: &mut Areas,
    lines: &mut Paths,
    points: &mut Vec<XY>,
    depth: usize,
    count: &mut usize,
    cancel: &AtomicBool,
) -> Result<(), String> {
    input::check(cancel)?;
    if depth > 16 {
        return Err("BGT-geometrie is te diep genest".into());
    }
    match g {
        Geometry::Polygon(p) => {
            let r = repair_polygon(p.clone(), cancel)?;
            areas.extend(r.areas);
            lines.extend(r.lines);
            points.extend(r.points);
            *count += p.iter().map(Vec::len).sum::<usize>();
        }
        Geometry::MultiPolygon(p) => {
            let paths = p.iter().flatten().cloned().collect::<Paths>();
            *count += paths.iter().map(Vec::len).sum::<usize>();
            let r = repair_polygon(paths, cancel)?;
            areas.extend(r.areas);
            lines.extend(r.lines);
            points.extend(r.points);
        }
        Geometry::LineString(p) => {
            *count += p.len();
            if !p.is_empty() && p.iter().all(|q| q == &p[0]) {
                points.push(p[0]);
            } else {
                lines.push(p.clone());
            }
        }
        Geometry::MultiLineString(p) => {
            *count += p.iter().map(Vec::len).sum::<usize>();
            for p in p {
                if !p.is_empty() && p.iter().all(|q| q == &p[0]) {
                    points.push(p[0]);
                } else {
                    lines.push(p.clone());
                }
            }
        }
        Geometry::Point(p) => {
            *count += 1;
            points.push(*p);
        }
        Geometry::MultiPoint(p) => {
            *count += p.len();
            points.extend(p);
        }
        Geometry::Collection(items) => {
            for item in items {
                collect(item, areas, lines, points, depth + 1, count, cancel)?;
            }
        }
    }
    if *count > MAX_VERTICES {
        return Err("BGT-geometrie te groot".into());
    }
    Ok(())
}

/// Repair every source part BEFORE union, then clip. This retains good tile
/// parts when another part of the same source feature is self-intersecting.
/// Lower-dimensional intersections (boundary lines and corner points) survive.
pub fn normalize(
    parts: &[&Geometry],
    bounds: Bounds,
    cancel: &AtomicBool,
) -> Result<Option<Geometry>, String> {
    input::check(cancel)?;
    if [bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y]
        .iter()
        .any(|v| !v.is_finite() || v.abs() > 1e12)
        || bounds.min_x > bounds.max_x
        || bounds.min_y > bounds.max_y
    {
        return Err("Ongeldige BGT-uitsnede".into());
    }
    let mut input_count = 0;
    for g in parts {
        input_count += vertex_count(g, cancel)?;
        if input_count > MAX_VERTICES {
            return Err("BGT-geometrie te groot".into());
        }
    }
    let mut areas = Vec::new();
    let mut lines = Vec::new();
    let mut points = Vec::new();
    let mut count = 0;
    for g in parts {
        collect(
            g,
            &mut areas,
            &mut lines,
            &mut points,
            0,
            &mut count,
            cancel,
        )?;
    }
    check_paths(&lines, cancel)?;
    check_paths(&vec![points.clone()], cancel)?;
    let areas = if areas.is_empty() {
        areas
    } else {
        simplify(&areas, cancel)?
    };
    input::check(cancel)?;
    let degenerate = bounds.min_x == bounds.max_x || bounds.min_y == bounds.max_y;
    let final_areas = if degenerate {
        Vec::new()
    } else {
        let frame = vec![vec![vec![
            [bounds.min_x, bounds.min_y],
            [bounds.max_x, bounds.min_y],
            [bounds.max_x, bounds.max_y],
            [bounds.min_x, bounds.max_y],
        ]]];
        overlay(&areas, &frame, OverlayRule::Intersect, cancel)?
    };
    lines = outside(&lines, &areas, cancel)?;
    // A 2D polygon can collapse only on an edge/corner of this axis-aligned
    // rectangle. Keep those contacts directly instead of subtracting two
    // independently rounded copies of every polygon boundary.
    if degenerate && (bounds.min_x != bounds.max_x || bounds.min_y != bounds.max_y) {
        let cut = vec![vec![
            [bounds.min_x, bounds.min_y],
            [bounds.max_x, bounds.max_y],
        ]];
        lines.extend(clip(
            &cut,
            &areas,
            ClipRule {
                invert: false,
                boundary_included: true,
            },
            cancel,
        )?);
    } else if !degenerate {
        for area in &areas {
            for ring in area {
                if ring.is_empty() {
                    continue;
                }
                for (&a, &b) in ring
                    .iter()
                    .zip(ring.iter().cycle().skip(1))
                    .take(ring.len())
                {
                    if let Some((u, v)) = clip_segment(a, b, bounds) {
                        if u == v {
                            points.push(u);
                        } else if (a[1] == b[1]
                            && ((a[1] == bounds.min_y && b[0] < a[0])
                                || (a[1] == bounds.max_y && b[0] > a[0])))
                            || (a[0] == b[0]
                                && ((a[0] == bounds.min_x && b[1] > a[1])
                                    || (a[0] == bounds.max_x && b[1] < a[1])))
                        {
                            lines.push(vec![u, v]);
                        }
                    }
                }
            }
        }
    }
    let mut clipped = Vec::new();
    for path in &lines {
        for pair in path.windows(2) {
            if let Some((a, b)) = clip_segment(pair[0], pair[1], bounds) {
                if a == b {
                    if !degenerate {
                        points.push(a);
                    }
                } else {
                    clipped.push(vec![a, b]);
                }
            }
        }
    }
    let lines = node_lines(clipped, cancel)?;
    let mut seen = std::collections::HashSet::new();
    points.retain(|&p| {
        p[0] >= bounds.min_x
            && p[0] <= bounds.max_x
            && p[1] >= bounds.min_y
            && p[1] <= bounds.max_y
            && !covered(p, &final_areas)
            && !lines
                .iter()
                .any(|line| line.windows(2).any(|s| on_segment(p, s[0], s[1])))
            && seen.insert(coordinate_key(p))
    });
    let mut result = Vec::new();
    let mut areas = final_areas;
    for ring in areas.iter_mut().flatten() {
        if ring.first() != ring.last() {
            ring.push(ring[0]);
        }
    }
    match areas.len() {
        0 => (),
        1 => result.push(Geometry::Polygon(areas.pop().unwrap())),
        _ => result.push(Geometry::MultiPolygon(areas)),
    }
    match lines.len() {
        0 => (),
        1 => result.push(Geometry::LineString(lines.into_iter().next().unwrap())),
        _ => result.push(Geometry::MultiLineString(lines)),
    }
    match points.len() {
        0 => (),
        1 => result.push(Geometry::Point(points[0])),
        _ => result.push(Geometry::MultiPoint(points)),
    }
    input::check(cancel)?;
    let result = match result.len() {
        0 => None,
        1 => result.pop(),
        _ => Some(Geometry::Collection(result)),
    };
    if let Some(g) = &result {
        vertex_count(g, cancel)?;
    }
    Ok(result)
}
