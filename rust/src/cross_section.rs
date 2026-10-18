//! Local cable/pipe marker spans, matching marxact_local_cross_section_patch.py.
//! A concave boundary can contain multiple spans; measured lateral offset selects
//! the correct one. No global-width fallback may draw across a polygon gap.
use crate::{model::Vertex, virtual_trench::VirtualTrench};

pub type XY = (f64, f64);
const EPSILON: f64 = 1e-9;
const TOLERANCE: f64 = 1e-8;
fn cross(a: XY, b: XY) -> f64 {
    a.0 * b.1 - a.1 * b.0
}

fn inside(p: XY, polygon: &[XY]) -> bool {
    let mut result = false;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + polygon.len() - 1) % polygon.len()];
        if (a.1 > p.1) != (b.1 > p.1) {
            let x = (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0;
            if p.0 < x {
                result = !result;
            }
        }
    }
    result
}

pub fn local_segment(polygon: &[XY], origin: XY, normal: XY, preferred: f64) -> Option<(XY, XY)> {
    if polygon.len() < 3
        || polygon.iter().any(|p| !p.0.is_finite() || !p.1.is_finite())
        || ![origin.0, origin.1, normal.0, normal.1, preferred]
            .iter()
            .all(|n| n.is_finite())
    {
        return None;
    }
    let length = normal.0.hypot(normal.1);
    if length <= EPSILON || !length.is_finite() {
        return None;
    }
    let direction = (normal.0 / length, normal.1 / length);
    let mut hits = Vec::new();
    let point = |t: f64| (origin.0 + direction.0 * t, origin.1 + direction.1 * t);
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        let edge = (b.0 - a.0, b.1 - a.1);
        let relative = (a.0 - origin.0, a.1 - origin.1);
        let denominator = cross(direction, edge);
        if denominator.abs() <= EPSILON {
            if cross(relative, direction).abs() <= TOLERANCE {
                hits.push(relative.0 * direction.0 + relative.1 * direction.1);
                hits.push((b.0 - origin.0) * direction.0 + (b.1 - origin.1) * direction.1);
            }
            continue;
        }
        let t = cross(relative, edge) / denominator;
        let segment = cross(relative, direction) / denominator;
        if (-TOLERANCE..=1.0 + TOLERANCE).contains(&segment) {
            hits.push(t);
        }
    }
    hits.sort_by(f64::total_cmp);
    hits.dedup_by(|a, b| (*a - *b).abs() <= TOLERANCE);
    let intervals: Vec<_> = hits
        .windows(2)
        .filter(|v| v[1] - v[0] > TOLERANCE && inside(point((v[0] + v[1]) * 0.5), polygon))
        .map(|v| (v[0], v[1]))
        .collect();
    let containing = intervals
        .iter()
        .filter(|v| v.0 - TOLERANCE <= preferred && preferred <= v.1 + TOLERANCE)
        .min_by(|a, b| {
            ((a.0 + a.1) * 0.5 - preferred)
                .abs()
                .total_cmp(&((b.0 + b.1) * 0.5 - preferred).abs())
        });
    let best = containing.or_else(|| {
        intervals.iter().min_by(|a, b| {
            (preferred - a.0)
                .abs()
                .min((preferred - a.1).abs())
                .total_cmp(&(preferred - b.0).abs().min((preferred - b.1).abs()))
        })
    })?;
    Some((point(best.0), point(best.1)))
}

#[derive(Clone, Debug)]
pub struct Marker {
    pub start: Vertex,
    pub end: Vertex,
    pub object_index: usize,
    pub chainage: f64,
}

/// Prepare once after geometry edits; callers can reuse these spans during pan/zoom.
pub fn markers(trench: &VirtualTrench) -> Vec<Marker> {
    markers_with_cancel(trench, &crate::input::NEVER_CANCEL).unwrap_or_default()
}
pub fn markers_with_cancel(
    trench: &VirtualTrench,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Vec<Marker>, String> {
    crate::input::check(cancel)?;
    let length = trench.length();
    if length <= EPSILON || !length.is_finite() {
        return Ok(Vec::new());
    }
    let normal = (
        -(trench.end.y - trench.start.y) / length,
        (trench.end.x - trench.start.x) / length,
    );
    let polygon = trench.measured_boundary();
    let mut output = Vec::new();
    for (index, object) in trench.objects.iter().enumerate() {
        crate::input::check(cancel)?;
        let p = object.position;
        let (x, y, chainage, _) = trench.project(p);
        let preferred = (p.x - x) * normal.0 + (p.y - y) * normal.1;
        let Some((start, end)) = local_segment(&polygon, (x, y), normal, preferred) else {
            continue;
        };
        output.push(Marker {
            start: Vertex {
                x: start.0,
                y: start.1,
                z: p.z,
            },
            end: Vertex {
                x: end.0,
                y: end.1,
                z: p.z,
            },
            object_index: index,
            chainage,
        })
    }
    Ok(output)
}
