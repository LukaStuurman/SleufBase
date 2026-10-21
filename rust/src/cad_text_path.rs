//! Source glyph contours, retained separately from their flattened preview.
//! Curve flatness is measured after the complete block/OCS transform.
use crate::{
    cad_geometry::{self as g, Affine, V3},
    model::Vertex,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;

pub(crate) const MAX_CONTROLS: usize = 2_000_000;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Path {
    /// 0: move, 1: line, 2: quadratic, 3: cubic. One start per contour.
    pub kinds: Vec<u8>,
    pub points: Vec<V3>,
}
#[derive(Debug)]
pub struct Flattened {
    pub points: Vec<Vertex>,
    pub breaks: Vec<usize>,
}
impl Path {
    pub fn validate(&self) -> Result<(), String> {
        if self.points.len() > MAX_CONTROLS || self.kinds.len() > MAX_CONTROLS {
            return Err("CAD-tekst overschrijdt contourbudget".into());
        }
        let mut used = 0usize;
        for &kind in &self.kinds {
            if kind > 3 || used == 0 && kind != 0 {
                return Err("Ongeldige CAD-lettercontour".into());
            }
            used = used
                .checked_add(usize::from(kind.max(1)))
                .ok_or("CAD-contour loopt over")?;
        }
        if used != self.points.len() {
            return Err("Onvolledige CAD-lettercontour".into());
        }
        for &p in &self.points {
            g::finite(p)?;
        }
        Ok(())
    }
    pub(crate) fn push(&mut self, kind: u8, points: &[V3], budget: usize) -> Result<(), String> {
        if self.points.len().saturating_add(points.len()) > budget.min(MAX_CONTROLS) {
            return Err("CAD-tekst overschrijdt contourbudget".into());
        }
        for &p in points {
            g::finite(p)?;
        }
        self.kinds.push(kind);
        self.points.extend_from_slice(points);
        Ok(())
    }
    pub(crate) fn append(
        &mut self,
        other: &Self,
        matrix: Affine,
        budget: usize,
    ) -> Result<(), String> {
        if self.points.len().saturating_add(other.points.len()) > budget.min(MAX_CONTROLS) {
            return Err("CAD-tekst overschrijdt contourbudget".into());
        }
        // Match glyph concatenation: coincident endpoints need no new move.
        // This also preserves continuous stroke glyphs without inventing ink.
        let skip = if other.kinds.first() == Some(&0) {
            if let (Some(a), Some(&b)) = (self.points.last(), other.points.first()) {
                let b = matrix.checked_point(b)?;
                (0..3)
                    .all(|i| (a[i] - b[i]).abs() <= 1e-12f64.max(1e-9 * a[i].abs().max(b[i].abs())))
            } else {
                false
            }
        } else {
            false
        };
        for &p in &other.points[usize::from(skip)..] {
            self.points.push(matrix.checked_point(p)?);
        }
        self.kinds
            .extend_from_slice(&other.kinds[usize::from(skip)..]);
        Ok(())
    }
    pub(crate) fn transform(&mut self, matrix: Affine) -> Result<(), String> {
        for p in &mut self.points {
            *p = matrix.checked_point(*p)?;
        }
        Ok(())
    }
    pub fn flatten(
        &self,
        matrix: Affine,
        tolerance: f64,
        budget: usize,
        cancel: &AtomicBool,
    ) -> Result<Flattened, String> {
        self.validate()?;
        if !tolerance.is_finite() || tolerance <= 0. {
            return Err("Ongeldige CAD-tekstnauwkeurigheid".into());
        }
        let mut out = Flattened {
            points: Vec::new(),
            breaks: Vec::new(),
        };
        let mut at = 0;
        let mut current = [0.; 3];
        let mut work = 0usize;
        let emit = |out: &mut Flattened, p: V3| -> Result<(), String> {
            if out.points.len() >= budget {
                return Err("CAD-tekst overschrijdt puntbudget".into());
            }
            out.points.push(g::vertex(p));
            Ok(())
        };
        for (index, &kind) in self.kinds.iter().enumerate() {
            if index.is_multiple_of(1024) {
                crate::input::check(cancel)?;
            }
            let n = usize::from(kind.max(1));
            let mut p = [[0.; 3]; 4];
            p[0] = current;
            for i in 0..n {
                p[i + 1] = matrix.checked_point(self.points[at + i])?;
            }
            at += n;
            if kind == 0 {
                if !out.points.is_empty() {
                    out.breaks.push(out.points.len());
                }
                // A move alone is not ink, nor a selectable point. Its first
                // point is emitted only when a following segment needs it.
                current = p[1];
                continue;
            }
            if out.points.is_empty() || out.breaks.last() == Some(&out.points.len()) {
                emit(&mut out, current)?;
            }
            current = p[n];
            if kind == 1 {
                emit(&mut out, current)?;
                continue;
            }
            let mut stack = vec![(p, n, 0u8)];
            while let Some((p, n, depth)) = stack.pop() {
                work += 1;
                if work.is_multiple_of(1024) {
                    crate::input::check(cancel)?;
                }
                if work > 20_000_000 {
                    return Err("CAD-tekst overschrijdt curvebudget".into());
                }
                let chord = g::sub(p[n], p[0]);
                let length2 = g::dot(chord, chord);
                let flat = (1..n).all(|i| {
                    let delta = g::sub(p[i], p[0]);
                    let t = if length2 == 0. {
                        0.
                    } else {
                        (g::dot(delta, chord) / length2).clamp(0., 1.)
                    };
                    g::length(g::sub(delta, g::scale(chord, t))) <= tolerance
                });
                if flat {
                    emit(&mut out, p[n])?;
                    continue;
                }
                if depth >= 32 {
                    return Err("CAD-lettercurve kan niet nauwkeurig worden weergegeven".into());
                }
                let mut tmp = p;
                let mut left = p;
                let mut right = p;
                left[0] = p[0];
                right[n] = p[n];
                for level in 1..=n {
                    for i in 0..=n - level {
                        tmp[i] = g::add(g::scale(tmp[i], 0.5), g::scale(tmp[i + 1], 0.5));
                    }
                    left[level] = tmp[0];
                    right[n - level] = tmp[n - level];
                }
                stack.push((right, n, depth + 1));
                stack.push((left, n, depth + 1));
            }
        }
        // Trailing and consecutive pen-up moves never produce empty paths.
        out.breaks.retain(|&b| b < out.points.len());
        out.breaks.dedup();
        crate::input::check(cancel)?;
        Ok(out)
    }
}
