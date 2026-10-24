//! B-spline blossom extraction, positive-weight rational Beziers and cubic fit curves.
//! Geometry is flattened after the complete world transform, using the control hull.
use crate::{
    cad_geometry::{self as g, Affine, V3},
    model::Vertex,
};
use serde::{Deserialize, Serialize};

const MAX_DEGREE: usize = 32;
const MAX_SOURCE_POINTS: usize = 250_000;
const MAX_EXTRACTION_WORK: usize = 50_000_000;
type H4 = [f64; 4];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spline {
    pub degree: u32,
    pub flags: u32,
    pub knots: Vec<f64>,
    pub controls: Vec<V3>,
    pub weights: Vec<f64>,
    pub fit_points: Vec<V3>,
    pub start_tangent: Option<V3>,
    pub end_tangent: Option<V3>,
    pub normal: Option<V3>,
    pub knot_tolerance: f64,
    pub control_tolerance: f64,
    pub fit_tolerance: f64,
}

#[derive(Default)]
pub struct Polyline {
    pub points: Vec<Vertex>,
    /// Index of the first vertex of each subsequent disconnected path.
    pub breaks: Vec<usize>,
    work: usize,
    stack: Vec<([H4; MAX_DEGREE + 1], u8)>,
}
impl Polyline {
    fn push(&mut self, p: V3, budget: usize) -> Result<(), String> {
        if self.points.len() >= budget {
            return Err("SPLINE overschrijdt het puntenbudget".into());
        }
        self.points
            .try_reserve(1)
            .map_err(|_| "Onvoldoende geheugen voor SPLINE")?;
        self.points.push(g::vertex(g::finite(p)?));
        Ok(())
    }
}

impl Spline {
    pub fn validate(&self) -> Result<(), String> {
        if self.controls.len().max(self.fit_points.len()) > MAX_SOURCE_POINTS
            || self.knots.len() > MAX_SOURCE_POINTS + MAX_DEGREE + 1
        {
            return Err("Te veel SPLINE-bronpunten/knopen".into());
        }
        for &p in self
            .controls
            .iter()
            .chain(&self.fit_points)
            .chain(self.start_tangent.iter())
            .chain(self.end_tangent.iter())
            .chain(self.normal.iter())
        {
            g::finite(p)?;
        }
        if !self.control_tolerance.is_finite()
            || self.control_tolerance < 0.
            || !self.fit_tolerance.is_finite()
            || self.fit_tolerance < 0.
            || !self.knot_tolerance.is_finite()
            || self.knot_tolerance < 0.
            || self.knots.iter().any(|v| !v.is_finite())
            || self.weights.iter().any(|v| !v.is_finite() || *v <= 0.)
        {
            return Err(
                "Ongeldige SPLINE-knopen, tolerantie of gewichten (positieve gewichten vereist)"
                    .into(),
            );
        }
        if self.controls.is_empty() {
            if self.fit_points.len() < 2 {
                return Err("SPLINE vereist controlepunten of minstens twee fitpunten".into());
            }
            return Ok(());
        }
        let p = self.degree as usize;
        if p == 0 || p > MAX_DEGREE || self.controls.len() <= p {
            return Err("Ongeldige SPLINE-graad of aantal controlepunten".into());
        }
        if !self.weights.is_empty() && self.weights.len() != self.controls.len() {
            return Err("SPLINE-gewichten passen niet bij de controlepunten".into());
        }
        if !self.knots.is_empty() {
            if self.knots.len() != self.controls.len() + p + 1
                || self.knots.windows(2).any(|v| v[0] > v[1])
                || self.knots[p] >= self.knots[self.controls.len()]
            {
                return Err("Ongeldige SPLINE-knoopvector".into());
            }
            let mut repeats = 0;
            let mut previous = None;
            for &k in &self.knots {
                repeats = if previous == Some(k) { repeats + 1 } else { 1 };
                if repeats > p + 1 {
                    return Err("SPLINE-knoopmultipliciteit is te groot".into());
                }
                previous = Some(k);
            }
        }
        let work = (self.controls.len() - p).saturating_mul((p + 1) * p * (p + 1) / 2);
        if work > MAX_EXTRACTION_WORK {
            return Err("SPLINE overschrijdt het berekeningsbudget".into());
        }
        Ok(())
    }

    pub fn flatten(
        &self,
        world: Affine,
        tolerance: f64,
        budget: usize,
    ) -> Result<Polyline, String> {
        self.validate()?;
        if !tolerance.is_finite() || tolerance <= 0. {
            return Err("Ongeldige SPLINE-afvlaktolerantie".into());
        }
        let mut output = Polyline::default();
        if self.controls.is_empty() {
            self.flatten_fit(world, tolerance, budget, &mut output)?;
        } else {
            self.flatten_controls(world, tolerance, budget, &mut output)?;
        }
        Ok(output)
    }

    fn flatten_controls(
        &self,
        world: Affine,
        tolerance: f64,
        budget: usize,
        output: &mut Polyline,
    ) -> Result<(), String> {
        let p = self.degree as usize;
        let generated;
        let knots = if self.knots.is_empty() {
            let end = (self.controls.len() - p) as f64;
            generated = (0..self.controls.len() + p + 1)
                .map(|i| {
                    if i <= p {
                        0.
                    } else if i >= self.controls.len() {
                        end
                    } else {
                        (i - p) as f64
                    }
                })
                .collect::<Vec<_>>();
            &generated
        } else {
            &self.knots
        };
        let max_weight = if self.weights.is_empty() {
            1.
        } else {
            self.weights.iter().copied().fold(0., f64::max)
        };
        let mut controls = Vec::with_capacity(self.controls.len());
        for (i, &point) in self.controls.iter().enumerate() {
            let point = world.checked_point(point)?;
            let w = self.weights.get(i).copied().unwrap_or(1.) / max_weight;
            if w == 0. {
                return Err("SPLINE-gewichten hebben een te groot numeriek bereik".into());
            }
            controls.push([point[0] * w, point[1] * w, point[2] * w, w]);
        }
        for k in p..self.controls.len() {
            let (a, b) = (knots[k], knots[k + 1]);
            if a == b {
                continue;
            }
            // Polar-form/de Boor evaluation at p-j copies of a and j copies of
            // b gives Bezier control j for this span, including periodic and
            // unclamped splines. No repeated insertion or whole-curve copies.
            let mut bezier = [[0.; 4]; MAX_DEGREE + 1];
            for (j, target) in bezier.iter_mut().enumerate().take(p + 1) {
                let mut work = [[0.; 4]; MAX_DEGREE + 1];
                work[..=p].copy_from_slice(&controls[k - p..=k]);
                for r in 1..=p {
                    let t = if r <= p - j { a } else { b };
                    for i in (r..=p).rev() {
                        let low = knots[k - p + i];
                        let high = knots[k + 1 + i - r];
                        let alpha = (t - low) / (high - low);
                        if !alpha.is_finite() || !(0. ..=1.).contains(&alpha) {
                            return Err("SPLINE-knoopbereik is numeriek te groot".into());
                        }
                        work[i] = mix(work[i - 1], work[i], alpha);
                    }
                }
                *target = work[p];
            }
            // A fully repeated internal knot is a genuine path break. Never
            // draw an invented line between two disconnected curve pieces.
            let disconnected = k > p && knots[k - p] == a;
            append_bezier(bezier, p, disconnected, tolerance, budget, output)?;
        }
        Ok(())
    }

    fn flatten_fit(
        &self,
        world: Affine,
        tolerance: f64,
        budget: usize,
        output: &mut Polyline,
    ) -> Result<(), String> {
        // A natural/clamped cubic in chord-length parameters is the unique C2
        // curve used by ezdxf's CAD-fit interpolation. Thomas elimination is
        // linear in the number of fit points, rather than a dense matrix solve.
        let q = &self.fit_points;
        let n = q.len();
        let h = q
            .windows(2)
            .map(|p| {
                let d = g::length(g::sub(p[1], p[0]));
                if d > 0. && d.is_finite() {
                    Ok(d)
                } else {
                    Err("SPLINE heeft samenvallende of te ver uiteen liggende fitpunten".to_owned())
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let slopes = q
            .windows(2)
            .zip(&h)
            .map(|(p, h)| g::finite(g::scale(g::sub(p[1], p[0]), 1. / h)))
            .collect::<Result<Vec<_>, _>>()?;
        let mut upper = vec![0.; n];
        let mut rhs = vec![[0.; 3]; n];
        let tangents = self
            .start_tangent
            .zip(self.end_tangent)
            .map(|(a, b)| Ok::<_, String>((unit(a)?, unit(b)?)))
            .transpose()?;
        if let Some((start, _)) = tangents {
            upper[0] = 0.5;
            rhs[0] = g::scale(g::sub(slopes[0], start), 3. / h[0]);
        }
        for i in 1..n - 1 {
            let diagonal = 2. * (h[i - 1] + h[i]) - h[i - 1] * upper[i - 1];
            if !diagonal.is_finite() || diagonal <= 0. {
                return Err("SPLINE-fitstelsel is numeriek ongeldig".into());
            }
            upper[i] = h[i] / diagonal;
            rhs[i] = g::finite(g::scale(
                g::sub(
                    g::scale(g::sub(slopes[i], slopes[i - 1]), 6.),
                    g::scale(rhs[i - 1], h[i - 1]),
                ),
                1. / diagonal,
            ))?;
        }
        if let Some((_, end)) = tangents {
            let last = n - 1;
            let diagonal = h[last - 1] * (2. - upper[last - 1]);
            rhs[last] = g::finite(g::scale(
                g::sub(
                    g::scale(g::sub(end, slopes[last - 1]), 6.),
                    g::scale(rhs[last - 1], h[last - 1]),
                ),
                1. / diagonal,
            ))?;
        }
        for i in (0..n - 1).rev() {
            rhs[i] = g::finite(g::sub(rhs[i], g::scale(rhs[i + 1], upper[i])))?;
        }
        for i in 0..n - 1 {
            let d0 = g::sub(
                slopes[i],
                g::scale(g::add(g::scale(rhs[i], 2.), rhs[i + 1]), h[i] / 6.),
            );
            let d1 = g::add(
                slopes[i],
                g::scale(g::add(rhs[i], g::scale(rhs[i + 1], 2.)), h[i] / 6.),
            );
            let points = [
                q[i],
                g::add(q[i], g::scale(d0, h[i] / 3.)),
                g::sub(q[i + 1], g::scale(d1, h[i] / 3.)),
                q[i + 1],
            ];
            let mut bezier = [[0.; 4]; MAX_DEGREE + 1];
            for (target, point) in bezier.iter_mut().zip(points) {
                let p = world.checked_point(point)?;
                *target = [p[0], p[1], p[2], 1.];
            }
            append_bezier(bezier, 3, false, tolerance, budget, output)?;
        }
        Ok(())
    }
}

fn unit(p: V3) -> Result<V3, String> {
    let magnitude = g::length(p);
    if magnitude <= 0. || !magnitude.is_finite() {
        return Err("Ongeldige SPLINE-eindtangent".into());
    }
    Ok(p.map(|v| v / magnitude))
}
fn mix(a: H4, b: H4, alpha: f64) -> H4 {
    if alpha == 0. {
        a
    } else if alpha == 1. {
        b
    } else {
        std::array::from_fn(|i| a[i] * (1. - alpha) + b[i] * alpha)
    }
}
fn point(h: H4) -> Result<V3, String> {
    if !h[3].is_finite() || h[3] <= 0. {
        return Err("SPLINE-gewicht is numeriek onbruikbaar".into());
    }
    g::finite([h[0] / h[3], h[1] / h[3], h[2] / h[3]])
}
fn distance(p: V3, a: V3, b: V3) -> Result<f64, String> {
    let d = g::sub(b, a);
    let length = g::length(d);
    if !length.is_finite() {
        return Err("SPLINE-coördinatenbereik is te groot".into());
    }
    let offset = g::sub(p, a);
    let result = if length == 0. {
        g::length(offset)
    } else {
        let direction = d.map(|v| v / length);
        let t = g::dot(offset, direction).clamp(0., length);
        g::length(g::sub(offset, g::scale(direction, t)))
    };
    if result.is_finite() {
        Ok(result)
    } else {
        Err("SPLINE-coördinatenbereik is te groot".into())
    }
}
fn append_bezier(
    control: [H4; MAX_DEGREE + 1],
    degree: usize,
    disconnected: bool,
    tolerance: f64,
    budget: usize,
    output: &mut Polyline,
) -> Result<(), String> {
    if output.points.is_empty() || disconnected {
        if !output.points.is_empty() {
            output.breaks.push(output.points.len());
        }
        output.push(point(control[0])?, budget)?;
    }
    output.stack.clear();
    output.stack.push((control, 0));
    while let Some((control, depth)) = output.stack.pop() {
        output.work = output
            .work
            .saturating_add(degree * (degree + 1) / 2 + degree);
        if output.work > MAX_EXTRACTION_WORK {
            return Err("SPLINE-afvlakking overschrijdt het berekeningsbudget".into());
        }
        let (a, b) = (point(control[0])?, point(control[degree])?);
        let mut flat = true;
        for &p in &control[1..degree] {
            if distance(point(p)?, a, b)? > tolerance {
                flat = false;
                break;
            }
        }
        if flat {
            output.push(b, budget)?;
        } else {
            if depth >= 64 || output.points.len() >= budget {
                return Err("SPLINE-afvlakking overschrijdt het diepte-/puntenbudget".into());
            }
            let mut work = control;
            let mut left = control;
            let mut right = control;
            for r in 1..=degree {
                for i in 0..=degree - r {
                    work[i] = mix(work[i], work[i + 1], 0.5);
                }
                left[r] = work[0];
                right[degree - r] = work[degree - r];
            }
            output.stack.push((right, depth + 1));
            output.stack.push((left, depth + 1));
        }
    }
    Ok(())
}
