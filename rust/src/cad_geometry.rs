//! WCS/OCS, composable affine transforms and bounded curve tessellation.
use crate::model::Vertex;
use serde::{Deserialize, Serialize};

pub type V3 = [f64; 3];
pub fn add(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| a[i] + b[i])
}
pub fn sub(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| a[i] - b[i])
}
pub fn scale(a: V3, k: f64) -> V3 {
    a.map(|v| v * k)
}
pub fn dot(a: V3, b: V3) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn length(a: V3) -> f64 {
    a[0].hypot(a[1]).hypot(a[2])
}
pub fn finite(a: V3) -> Result<V3, String> {
    if a.iter().all(|v| v.is_finite()) {
        Ok(a)
    } else {
        Err("CAD-transformatie geeft niet-eindige coördinaten".into())
    }
}
pub fn xyz(p: Vertex) -> V3 {
    [p.x, p.y, p.z.unwrap_or(0.0)]
}
pub fn vertex(p: V3) -> Vertex {
    Vertex {
        x: p[0],
        y: p[1],
        z: Some(p[2]),
    }
}
fn unit(a: V3) -> Result<V3, String> {
    finite(a)?;
    let n = length(a);
    if n == 0.0 || !n.is_finite() {
        return Err("Ongeldige CAD-extrusierichting".into());
    }
    Ok(a.map(|v| v / n))
}

#[derive(Clone, Copy, Debug)]
pub struct Affine(pub [[f64; 4]; 3]);
impl Affine {
    pub const IDENTITY: Self = Self([[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]]);
    pub fn vector(self, p: V3) -> V3 {
        std::array::from_fn(|i| self.0[i][0] * p[0] + self.0[i][1] * p[1] + self.0[i][2] * p[2])
    }
    pub fn point(self, p: V3) -> V3 {
        add(self.vector(p), [self.0[0][3], self.0[1][3], self.0[2][3]])
    }
    pub fn checked_point(self, p: V3) -> Result<V3, String> {
        finite(self.point(p))
    }
    /// self(local(point)): a parent's transform composed with a child placement.
    pub fn compose(self, local: Self) -> Result<Self, String> {
        let mut result = Self::IDENTITY;
        for row in 0..3 {
            for column in 0..3 {
                result.0[row][column] = (0..3).map(|k| self.0[row][k] * local.0[k][column]).sum();
            }
            result.0[row][3] = self.point([local.0[0][3], local.0[1][3], local.0[2][3]])[row];
        }
        if result.0.iter().flatten().any(|v| !v.is_finite()) {
            return Err("CAD-bloktransformatie is te groot".into());
        }
        Ok(result)
    }
    /// Autodesk arbitrary-axis algorithm, including the exact 1/64 polar cap.
    pub fn ocs(normal: V3) -> Result<Self, String> {
        let z = unit(normal)?;
        let x = unit(if z[0].abs() < 1. / 64. && z[1].abs() < 1. / 64. {
            cross([0., 1., 0.], z)
        } else {
            cross([0., 0., 1.], z)
        })?;
        let y = unit(cross(z, x))?;
        Ok(Self(std::array::from_fn(|i| [x[i], y[i], z[i], 0.])))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub insert: V3,
    pub scale: V3,
    pub rotation: f64,
    pub normal: V3,
    pub columns: u32,
    pub rows: u32,
    pub column_spacing: f64,
    pub row_spacing: f64,
}
impl Placement {
    pub fn validate(&self) -> Result<(), String> {
        finite(self.insert)?;
        finite(self.scale)?;
        if self.scale.contains(&0.)
            || !self.rotation.is_finite()
            || !self.column_spacing.is_finite()
            || !self.row_spacing.is_finite()
            || self.rows == 0
            || self.columns == 0
        {
            return Err("Ongeldige CAD-blokplaatsing".into());
        }
        Affine::ocs(self.normal)?;
        Ok(())
    }
    pub fn grid_size(&self) -> u64 {
        u64::from(if self.row_spacing == 0. { 1 } else { self.rows })
            * u64::from(if self.column_spacing == 0. {
                1
            } else {
                self.columns
            })
    }
    pub fn matrix(&self, base: V3, row: u32, column: u32) -> Result<Affine, String> {
        self.validate()?;
        finite(base)?;
        let ocs = Affine::ocs(self.normal)?;
        let (s, c) = self.rotation.rem_euclid(360.).to_radians().sin_cos();
        let [sx, sy, sz] = self.scale;
        let x = ocs.vector([c * sx, s * sx, 0.]);
        let y = ocs.vector([-s * sy, c * sy, 0.]);
        let z = ocs.vector([0., 0., sz]);
        let mut m = Affine(std::array::from_fn(|i| [x[i], y[i], z[i], 0.]));
        // MINSERT spacing rotates with the insert but is never scaled.
        let a = f64::from(column) * self.column_spacing;
        let b = f64::from(row) * self.row_spacing;
        let at = ocs.point(add(self.insert, [c * a - s * b, s * a + c * b, 0.]));
        let translation = finite(sub(at, m.vector(base)))?;
        for (i, t) in translation.into_iter().enumerate() {
            m.0[i][3] = t;
        }
        Ok(m)
    }
}

/// Stable anchored arc: origin + u*(cos(t)-1) + v*sin(t), 0 <= t <= sweep.
/// Keeping this instead of only preview chords permits correct refinement after
/// nested nonuniform scaling, shear, reflection, or insertion into another plane.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArcCurve {
    pub origin: V3,
    pub u: V3,
    pub v: V3,
    pub sweep: f64,
}
impl ArcCurve {
    pub fn validate(&self) -> Result<(), String> {
        finite(self.origin)?;
        finite(self.u)?;
        finite(self.v)?;
        if !self.sweep.is_finite() || self.sweep.abs() > std::f64::consts::TAU + 1e-10 {
            return Err("Ongeldige CAD-booghoek".into());
        }
        Ok(())
    }
    pub fn transformed(&self, m: Affine) -> Result<Self, String> {
        let result = Self {
            origin: m.checked_point(self.origin)?,
            u: finite(m.vector(self.u))?,
            v: finite(m.vector(self.v))?,
            sweep: self.sweep,
        };
        result.validate()?;
        Ok(result)
    }
    pub fn at(&self, t: f64) -> V3 {
        // cos(t)-1 suffers cancellation for small bulge angles.
        add(
            self.origin,
            add(
                scale(self.u, -2. * (t / 2.).sin().powi(2)),
                scale(self.v, t.sin()),
            ),
        )
    }
    fn radius_bound(&self) -> f64 {
        let n = length(self.u).max(length(self.v));
        if n == 0. {
            return 0.;
        }
        let u = self.u.map(|v| v / n);
        let v = self.v.map(|v| v / n);
        let a = dot(u, u);
        let b = dot(v, v);
        let c = dot(u, v);
        n * ((a + b + (a - b).hypot(2. * c)) / 2.).sqrt()
    }
    pub fn flatten(&self, tolerance: f64, budget: usize) -> Result<Vec<Vertex>, String> {
        let mut output = Vec::new();
        self.append_to(&mut output, tolerance, budget, false)?;
        Ok(output)
    }
    fn append_to(
        &self,
        result: &mut Vec<Vertex>,
        tolerance: f64,
        budget: usize,
        skip_first: bool,
    ) -> Result<(), String> {
        self.validate()?;
        if !tolerance.is_finite() || tolerance <= 0. {
            return Err("Ongeldige CAD-boogtolerantie".into());
        }
        let radius = self.radius_bound();
        if !radius.is_finite() {
            return Err("CAD-boog is te groot".into());
        }
        let step = if radius <= tolerance {
            std::f64::consts::PI
        } else {
            4. * ((tolerance / radius) / 2.).sqrt().asin()
        };
        let minimum = if self.sweep.abs() == std::f64::consts::TAU {
            4
        } else {
            1
        };
        let count = (self.sweep.abs() / step).ceil().max(minimum as f64);
        let skip = usize::from(skip_first);
        let available = budget.saturating_sub(result.len());
        if !count.is_finite() || count > available.saturating_add(skip).saturating_sub(1) as f64 {
            return Err("CAD-boog overschrijdt het puntenbudget".into());
        }
        let count = count as usize;
        let added = count
            .checked_add(1)
            .and_then(|n| n.checked_sub(skip))
            .ok_or("CAD-boog heeft te veel punten")?;
        result
            .try_reserve(added)
            .map_err(|_| "Onvoldoende geheugen voor CAD-boog")?;
        for i in skip..=count {
            result.push(vertex(finite(
                self.at(self.sweep * i as f64 / count as f64),
            )?));
        }
        if minimum == 4 {
            if let Some(last) = result.last_mut() {
                *last = vertex(self.origin);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Segment {
    Line { start: V3, end: V3 },
    Arc(ArcCurve),
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurvePath {
    pub segments: Vec<Segment>,
}
impl CurvePath {
    pub fn validate(&self) -> Result<(), String> {
        if self.segments.len() > 1_000_000 {
            return Err("Te veel CAD-boogsegmenten".into());
        }
        for segment in &self.segments {
            match segment {
                Segment::Line { start, end } => {
                    finite(*start)?;
                    finite(*end)?;
                }
                Segment::Arc(arc) => arc.validate()?,
            }
        }
        Ok(())
    }
    pub fn flatten(&self, m: Affine, tolerance: f64, budget: usize) -> Result<Vec<Vertex>, String> {
        let mut result = Vec::new();
        for segment in &self.segments {
            let skip = !result.is_empty();
            match segment {
                Segment::Line { start, end } => {
                    let added = if skip { 1 } else { 2 };
                    if result.len().saturating_add(added) > budget {
                        return Err("CAD-pad overschrijdt het puntenbudget".into());
                    }
                    result
                        .try_reserve(added)
                        .map_err(|_| "Onvoldoende geheugen voor CAD-pad")?;
                    if !skip {
                        result.push(vertex(m.checked_point(*start)?));
                    }
                    result.push(vertex(m.checked_point(*end)?));
                }
                Segment::Arc(arc) => {
                    arc.transformed(m)?
                        .append_to(&mut result, tolerance, budget, skip)?
                }
            }
        }
        Ok(result)
    }
}

pub fn bulge(start: V3, end: V3, b: f64, ocs: Affine) -> Result<Segment, String> {
    finite(start)?;
    finite(end)?;
    if !b.is_finite() {
        return Err("Ongeldige CAD-bulge".into());
    }
    if b == 0. || start == end {
        return Ok(Segment::Line {
            start: ocs.checked_point(start)?,
            end: ocs.checked_point(end)?,
        });
    }
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let k = (1. / b - b) / 4.;
    let u = [-dx / 2. + dy * k, -dy / 2. - dx * k, 0.];
    let v = [-u[1], u[0], 0.];
    let arc = ArcCurve {
        origin: ocs.checked_point(start)?,
        u: finite(ocs.vector(u))?,
        v: finite(ocs.vector(v))?,
        sweep: 4. * b.atan(),
    };
    arc.validate()?;
    Ok(Segment::Arc(arc))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn connected_lines_and_arcs_use_exact_output_budget_without_duplicate_join_vertices() {
        let path = CurvePath {
            segments: vec![
                Segment::Line {
                    start: [-1., 0., 0.],
                    end: [0.; 3],
                },
                Segment::Arc(ArcCurve {
                    origin: [0.; 3],
                    u: [0., -1., 0.],
                    v: [1., 0., 0.],
                    sweep: std::f64::consts::FRAC_PI_2,
                }),
            ],
        };
        let points = path.flatten(Affine::IDENTITY, 0.25, 4).unwrap();
        assert_eq!(points.len(), 4);
        assert!((points[3].x - 1.).abs() < 1e-10);
        assert!((points[3].y - 1.).abs() < 1e-10);
        assert!(path.flatten(Affine::IDENTITY, 0.25, 3).is_err());
    }
    #[test]
    fn very_flat_long_bulge_keeps_its_measured_half_meter_deflection() {
        let Segment::Arc(arc) = bulge([0., 0., 2.], [1e8, 0., 2.], 1e-8, Affine::IDENTITY).unwrap()
        else {
            panic!()
        };
        let middle = arc.at(arc.sweep / 2.);
        assert!((middle[0] - 5e7).abs() < 1e-7);
        assert!((middle[1] + 0.5).abs() < 1e-9);
        assert_eq!(middle[2], 2.);
        let end = arc.at(arc.sweep);
        assert!((end[0] - 1e8).abs() < 1e-7);
        assert!(end[1].abs() < 1e-8);
    }
    #[test]
    fn huge_radius_with_tiny_sweep_is_bounded_without_overflowing_step_calculation() {
        let mut arc = ArcCurve {
            origin: [0.; 3],
            u: [1e308, 0., 0.],
            v: [0., 1e308, 0.],
            sweep: 1e-305,
        };
        let points = arc.flatten(0.25, 2).unwrap();
        assert_eq!(points.len(), 2);
        assert!((points[1].y - 1000.).abs() < 1e-9);
        arc.sweep = std::f64::consts::TAU;
        assert!(arc.flatten(0.25, 1000).is_err());
        assert!(arc.flatten(f64::NAN, 1000).is_err());
    }
}
