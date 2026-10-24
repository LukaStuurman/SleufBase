//! MarXact direction/midpoint rules from the Python direction and midpoint patches.
//! Automatic axes snap to edge midpoints; manual corrections reclip to the boundary.

use crate::marxact::{nearest_boundary_z, signed_area};
use crate::model::Vertex;
use crate::virtual_trench::VirtualTrench;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const EPSILON: f64 = 1e-10;
const INTERSECTION_TOLERANCE: f64 = 1e-8;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarXactAlignment {
    pub auto_start: Vertex,
    pub auto_end: Vertex,
    pub auto_width: f64,
    pub rotation_degrees: f64,
}

impl MarXactAlignment {
    pub fn new(start: Vertex, end: Vertex, width: f64) -> Self {
        Self {
            auto_start: start,
            auto_end: end,
            auto_width: width,
            rotation_degrees: 0.0,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !finite(self.auto_start)
            || !finite(self.auto_end)
            || !self.auto_width.is_finite()
            || self.auto_width < 0.1
            || (self.auto_end.x - self.auto_start.x).hypot(self.auto_end.y - self.auto_start.y)
                <= 1e-9
            || !self.rotation_degrees.is_finite()
            || !(-90.0..=90.0).contains(&self.rotation_degrees)
        {
            return Err("Ongeldige MarXact-uitlijning in project".into());
        }
        Ok(())
    }
}

fn finite(p: Vertex) -> bool {
    p.x.is_finite() && p.y.is_finite() && p.z.is_none_or(f64::is_finite)
}

#[derive(Clone, Copy)]
struct Axis {
    cx: f64,
    cy: f64,
    ux: f64,
    uy: f64,
}

fn pca(points: &[(f64, f64)]) -> Option<Axis> {
    if points.len() < 2 {
        return None;
    }
    let len = points.len() as f64;
    let cx = points.iter().map(|p| p.0).sum::<f64>() / len;
    let cy = points.iter().map(|p| p.1).sum::<f64>() / len;
    let xx = points.iter().map(|p| (p.0 - cx).powi(2)).sum::<f64>() / len;
    let xy = points.iter().map(|p| (p.0 - cx) * (p.1 - cy)).sum::<f64>() / len;
    let yy = points.iter().map(|p| (p.1 - cy).powi(2)).sum::<f64>() / len;
    if xx + yy <= EPSILON {
        return None;
    }
    let angle = 0.5 * (2.0 * xy).atan2(xx - yy);
    Some(Axis {
        cx,
        cy,
        ux: angle.cos(),
        uy: angle.sin(),
    })
}

fn cross(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    ax * by - ay * bx
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) * 0.5
    } else {
        values[mid]
    }
}

fn robust_axis(points: &[(f64, f64)]) -> Option<Axis> {
    let mut seen = BTreeSet::new();
    let unique: Vec<_> = points
        .iter()
        .copied()
        .filter(|p| {
            seen.insert((
                (p.0 * 1e6).round_ties_even() as i64,
                (p.1 * 1e6).round_ties_even() as i64,
            ))
        })
        .collect();
    let axis = pca(&unique)?;
    if unique.len() < 4 {
        return Some(axis);
    }
    let residuals: Vec<_> = unique
        .iter()
        .map(|p| cross(p.0 - axis.cx, p.1 - axis.cy, axis.ux, axis.uy).abs())
        .collect();
    let med = median(residuals.clone());
    let mad = median(residuals.iter().map(|r| (r - med).abs()).collect());
    let threshold = 0.03f64.max(med + 2.75 * mad.max(0.005));
    let inliers: Vec<_> = unique
        .iter()
        .zip(&residuals)
        .filter(|(_, r)| **r <= threshold)
        .map(|(p, _)| *p)
        .collect();
    if inliers.len() < 2.max((unique.len() as f64 * 0.6).ceil() as usize)
        || inliers.len() == unique.len()
    {
        Some(axis)
    } else {
        pca(&inliers).or(Some(axis))
    }
}

fn snap_direction(mut axis: Axis, polygon: &[Vertex]) -> Axis {
    let target = axis.uy.atan2(axis.ux);
    let mut candidates = Vec::new();
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let length = dx.hypot(dy);
        if length <= 0.05 {
            continue;
        }
        let angle = dy.atan2(dx);
        let diff = (angle - target).rem_euclid(std::f64::consts::PI);
        if diff.min(std::f64::consts::PI - diff) <= 12.5f64.to_radians() {
            candidates.push((angle, length));
        }
    }
    if candidates.len() < 2 {
        return axis;
    }
    let x = candidates
        .iter()
        .map(|(a, w)| (2.0 * a).cos() * w)
        .sum::<f64>();
    let y = candidates
        .iter()
        .map(|(a, w)| (2.0 * a).sin() * w)
        .sum::<f64>();
    if x.hypot(y) <= EPSILON {
        return axis;
    }
    let angle = 0.5 * y.atan2(x);
    let (mut ux, mut uy) = (angle.cos(), angle.sin());
    if ux * axis.ux + uy * axis.uy < 0.0 {
        ux = -ux;
        uy = -uy;
    }
    axis.ux = ux;
    axis.uy = uy;
    axis
}

fn boundary(points: &[Vertex]) -> Result<&[Vertex], String> {
    if points.iter().any(|p| !finite(*p)) {
        return Err("MarXact-grens bevat ongeldige coördinaten".into());
    }
    let points = if points.len() > 1
        && points
            .first()
            .zip(points.last())
            .is_some_and(|(a, b)| a.x == b.x && a.y == b.y)
    {
        &points[..points.len() - 1]
    } else {
        points
    };
    if points.len() < 3 || signed_area(points).abs() < 0.02 {
        return Err("MarXact-grens moet een gesloten vlak met meetbare oppervlakte zijn".into());
    }
    Ok(points)
}

fn width_for_axis(points: &[Vertex], axis: Axis) -> f64 {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for p in points {
        let value = -(p.x - axis.cx) * axis.uy + (p.y - axis.cy) * axis.ux;
        min = min.min(value);
        max = max.max(value);
    }
    (max - min).max(0.1)
}

fn centerline_from_axis(
    points: &[Vertex],
    objects: &[Vertex],
    axis: Axis,
) -> Result<(Vertex, Vertex, f64), String> {
    let projection = |p: Vertex| (p.x - axis.cx) * axis.ux + (p.y - axis.cy) * axis.uy;
    let mut hits = Vec::new();
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let (rx, ry) = (a.x - axis.cx, a.y - axis.cy);
        let denominator = cross(axis.ux, axis.uy, dx, dy);
        if denominator.abs() <= EPSILON {
            if cross(rx, ry, axis.ux, axis.uy).abs() <= INTERSECTION_TOLERANCE {
                hits.extend([projection(a), projection(b)]);
            }
            continue;
        }
        let t = cross(rx, ry, dx, dy) / denominator;
        let segment_t = cross(rx, ry, axis.ux, axis.uy) / denominator;
        if (-INTERSECTION_TOLERANCE..=1.0 + INTERSECTION_TOLERANCE).contains(&segment_t) {
            hits.push(t);
        }
    }
    hits.sort_by(f64::total_cmp);
    hits.dedup_by(|a, b| (*a - *b).abs() <= INTERSECTION_TOLERANCE);
    let (mut lo, mut hi) = if hits.len() >= 2 {
        (hits[0], hits[hits.len() - 1])
    } else {
        (
            points
                .iter()
                .copied()
                .map(projection)
                .fold(f64::INFINITY, f64::min),
            points
                .iter()
                .copied()
                .map(projection)
                .fold(f64::NEG_INFINITY, f64::max),
        )
    };
    if hits.len() >= 2 && !objects.is_empty() {
        let omin = objects
            .iter()
            .copied()
            .map(projection)
            .fold(f64::INFINITY, f64::min);
        let omax = objects
            .iter()
            .copied()
            .map(projection)
            .fold(f64::NEG_INFINITY, f64::max);
        let lower = hits
            .iter()
            .copied()
            .rfind(|t| *t <= omin + INTERSECTION_TOLERANCE)
            .unwrap_or(lo);
        let upper = hits
            .iter()
            .copied()
            .find(|t| *t >= omax - INTERSECTION_TOLERANCE)
            .unwrap_or(hi);
        if upper - lower > INTERSECTION_TOLERANCE {
            lo = lower;
            hi = upper;
        }
    }
    if !lo.is_finite() || !hi.is_finite() || hi - lo <= 1e-9 {
        return Err("De afgeleide middenlijn heeft geen lengte".into());
    }
    let point = |t: f64| {
        let x = axis.cx + axis.ux * t;
        let y = axis.cy + axis.uy * t;
        Vertex {
            x,
            y,
            z: nearest_boundary_z(x, y, points),
        }
    };
    let result = (point(lo), point(hi), width_for_axis(points, axis));
    MarXactAlignment::new(result.0, result.1, result.2).validate()?;
    Ok(result)
}

fn snap_midpoint(points: &[Vertex], hit: Vertex) -> Vertex {
    let mut candidates = Vec::with_capacity(points.len());
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let len2 = dx * dx + dy * dy;
        let t = if len2 <= 1e-16 {
            0.0
        } else {
            (((hit.x - a.x) * dx + (hit.y - a.y) * dy) / len2).clamp(0.0, 1.0)
        };
        let distance = (hit.x - a.x - t * dx).hypot(hit.y - a.y - t * dy);
        let mx = (a.x + b.x) * 0.5;
        let my = (a.y + b.y) * 0.5;
        candidates.push((distance, (hit.x - mx).hypot(hit.y - my), i, mx, my));
    }
    let min = candidates.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
    let allowed = 1e-6f64.max(min + 1e-6);
    let best = candidates
        .iter()
        .filter(|c| c.0 <= allowed)
        .min_by(|a, b| {
            a.1.total_cmp(&b.1)
                .then(a.0.total_cmp(&b.0))
                .then(a.2.cmp(&b.2))
        })
        .expect("validated boundary has edges");
    Vertex {
        x: best.3,
        y: best.4,
        z: nearest_boundary_z(best.3, best.4, points),
    }
}

/// Python automatic alignment including robust object PCA, 12.5° edge-direction
/// stabilization, concave intersection bracketing, and the midpoint patch.
pub fn automatic_centerline(
    points: &[Vertex],
    objects: &[Vertex],
) -> Result<(Vertex, Vertex, f64), String> {
    let points = boundary(points)?;
    if objects.iter().any(|p| !finite(*p)) {
        return Err("MarXact-objectpositie is ongeldig".into());
    }
    let coords: Vec<_> = points.iter().map(|p| (p.x, p.y)).collect();
    let positions: Vec<_> = objects.iter().map(|p| (p.x, p.y)).collect();
    let object_axis = robust_axis(&positions).map(|a| snap_direction(a, points));
    let axis = object_axis
        .or_else(|| pca(&coords))
        .ok_or("Geen richting uit grens af te leiden")?;
    let exact = centerline_from_axis(
        points,
        if object_axis.is_some() { objects } else { &[] },
        axis,
    )?;
    let start = snap_midpoint(points, exact.0);
    let end = snap_midpoint(points, exact.1);
    let length = (end.x - start.x).hypot(end.y - start.y);
    if length <= 1e-8 {
        return Ok(exact);
    }
    let snapped_axis = Axis {
        cx: (start.x + end.x) * 0.5,
        cy: (start.y + end.y) * 0.5,
        ux: (end.x - start.x) / length,
        uy: (end.y - start.y) / length,
    };
    Ok((start, end, width_for_axis(points, snapped_axis)))
}

/// Recompute from measured geometry, replacing the baseline and clearing rotation.
/// Failure leaves the trench unchanged.
pub fn recalculate(trench: &mut VirtualTrench) -> Result<(), String> {
    trench.validate()?;
    let objects: Vec<_> = trench.objects.iter().map(|p| p.position).collect();
    let (start, end, width) = automatic_centerline(&trench.boundary_3d, &objects)?;
    trench.start = start;
    trench.end = end;
    trench.width_meters = width;
    trench.alignment = Some(MarXactAlignment::new(start, end, width));
    Ok(())
}

/// Absolute correction relative to the saved automatic axis, never cumulative.
/// Manual endpoints stay at exact intersections rather than snapping to midpoints.
pub fn apply_rotation(trench: &mut VirtualTrench, degrees: f64) -> Result<(), String> {
    trench.validate()?;
    if !degrees.is_finite() {
        return Err("MarXact-rotatie moet een eindig getal zijn".into());
    }
    let points = boundary(&trench.boundary_3d)?;
    let mut baseline = trench
        .alignment
        .clone()
        .unwrap_or_else(|| MarXactAlignment::new(trench.start, trench.end, trench.width_meters));
    baseline.validate()?;
    let objects: Vec<_> = trench.objects.iter().map(|p| p.position).collect();
    let (cx, cy) = if objects.is_empty() {
        (
            (baseline.auto_start.x + baseline.auto_end.x) * 0.5,
            (baseline.auto_start.y + baseline.auto_end.y) * 0.5,
        )
    } else {
        (
            objects.iter().map(|p| p.x).sum::<f64>() / objects.len() as f64,
            objects.iter().map(|p| p.y).sum::<f64>() / objects.len() as f64,
        )
    };
    let rotation = degrees.clamp(-90.0, 90.0);
    let angle = (baseline.auto_end.y - baseline.auto_start.y)
        .atan2(baseline.auto_end.x - baseline.auto_start.x)
        + rotation.to_radians();
    let (start, end, width) = centerline_from_axis(
        points,
        &objects,
        Axis {
            cx,
            cy,
            ux: angle.cos(),
            uy: angle.sin(),
        },
    )?;
    baseline.rotation_degrees = rotation;
    trench.start = start;
    trench.end = end;
    trench.width_meters = width;
    trench.alignment = Some(baseline);
    Ok(())
}
