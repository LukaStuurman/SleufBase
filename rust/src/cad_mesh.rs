//! Indexed CAD surfaces, retained independently of their cached visible edges.
//! The preview draws the control cage; subdivision and surface shading are separate.
use crate::{cad_geometry as g, model::Vertex};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

const MAX_POINTS: usize = 10_000_000;
const MAX_FACES: usize = 2_000_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
// Externally tagged variants deserialize f64 directly. Serde's buffered
// internally-tagged Content path is incompatible with arbitrary_precision.
#[serde(deny_unknown_fields)]
pub enum Source {
    Polyface {
        flags: u32,
        declared: [u32; 2],
    },
    Polygon {
        flags: u32,
        size: [u32; 2],
        density: [u32; 2],
        smooth_type: u32,
    },
    Mesh {
        version: u32,
        blend_crease: bool,
    },
    Face {
        invisible_edges: u32,
    },
    Solid {
        normal: g::V3,
        thickness: f64,
    },
    Trace {
        normal: g::V3,
        thickness: f64,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mesh {
    pub source: Source,
    /// WCS coordinates before containing INSERT transforms.
    pub vertices: Vec<Vertex>,
    /// Signed one-based indices: a negative index hides its outgoing edge.
    pub faces: Vec<Vec<i32>>,
    /// Explicit MESH edges use zero-based indices, as in the DXF source.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<[u32; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub creases: Vec<f64>,
    #[serde(default)]
    pub subdivision: u32,
    /// Nondefault vertex/face appearance and XDATA retained for future surface rendering.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vertex_metadata: BTreeMap<usize, Vec<crate::cad::Tag>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub face_metadata: BTreeMap<usize, Vec<crate::cad::Tag>>,
}
impl Mesh {
    pub fn validate(&self) -> Result<(), String> {
        if self.vertices.len() > MAX_POINTS
            || self.faces.len() > MAX_FACES
            || self.edges.len() > MAX_POINTS / 2
        {
            return Err("CAD-mesh overschrijdt het bronbudget".into());
        }
        for vertex in &self.vertices {
            g::finite(g::xyz(*vertex))?;
        }
        let mut tag_count = 0usize;
        for (metadata, count) in [
            (&self.vertex_metadata, self.vertices.len()),
            (&self.face_metadata, self.faces.len()),
        ] {
            for (index, tags) in metadata {
                tag_count = tag_count
                    .checked_add(tags.len())
                    .ok_or("Te veel meshmetadata")?;
                if *index >= count
                    || tag_count > 1_000_000
                    || tags.iter().any(|t| {
                        t.value.len() > 1_048_576
                            || !matches!(t.code, 6 | 8 | 60 | 62 | 370 | 420 | 1000..=1071)
                    })
                {
                    return Err("Ongeldige meshmetadata".into());
                }
            }
        }
        let mut indices = 0usize;
        for face in &self.faces {
            indices = indices
                .checked_add(face.len())
                .ok_or("Te veel CAD-meshindices")?;
            if face.len() < 3
                || indices > MAX_POINTS
                || face
                    .iter()
                    .any(|i| *i == 0 || i.unsigned_abs() as usize > self.vertices.len())
            {
                return Err("Ongeldige CAD-meshvlakindices".into());
            }
        }
        if self
            .edges
            .iter()
            .flatten()
            .any(|i| *i as usize >= self.vertices.len())
            || (!self.creases.is_empty() && self.creases.len() != self.edges.len())
            || self.creases.iter().any(|v| !v.is_finite())
        {
            return Err("Ongeldige CAD-meshranden of vouwgewichten".into());
        }
        match &self.source {
            Source::Solid { normal, thickness } | Source::Trace { normal, thickness } => {
                g::Affine::ocs(*normal)?;
                if !thickness.is_finite() {
                    return Err("Ongeldige CAD-vlakdikte".into());
                }
            }
            Source::Polygon { size, .. }
                if size[0] < 2
                    || size[1] < 2
                    || u64::from(size[0]) * u64::from(size[1]) != self.vertices.len() as u64 =>
            {
                return Err("CAD-polygonmesh heeft ongeldige rasterafmetingen".into());
            }
            _ => {}
        }
        Ok(())
    }
    /// Keep first-seen order while deduplicating shared visible topological edges.
    /// Each edge is a separate path; hidden edges cannot become connecting chords.
    pub fn wireframe(&self, budget: usize) -> Result<(Vec<Vertex>, Vec<usize>), String> {
        self.validate()?;
        let mut seen = HashSet::new();
        let mut points = Vec::new();
        let mut breaks = Vec::new();
        let mut edge = |a: usize, b: usize| -> Result<(), String> {
            if a == b || g::xyz(self.vertices[a]) == g::xyz(self.vertices[b]) {
                return Ok(());
            }
            let key = (a.min(b), a.max(b));
            if seen.contains(&key) {
                return Ok(());
            }
            if points.len().saturating_add(2) > budget.min(MAX_POINTS) {
                return Err("CAD-meshranden overschrijden het weergavebudget".into());
            }
            if breaks.len() >= 1_000_000 {
                return Err("CAD-meshranden overschrijden het padbudget".into());
            }
            seen.try_reserve(1)
                .map_err(|_| "Onvoldoende geheugen voor CAD-meshranden")?;
            points
                .try_reserve(2)
                .map_err(|_| "Onvoldoende geheugen voor CAD-meshpunten")?;
            if !points.is_empty() {
                breaks
                    .try_reserve(1)
                    .map_err(|_| "Onvoldoende geheugen voor CAD-meshpaden")?;
                breaks.push(points.len());
            }
            seen.insert(key);
            points.extend([self.vertices[a], self.vertices[b]]);
            Ok(())
        };
        for face in &self.faces {
            for (at, &index) in face.iter().enumerate() {
                if index > 0 {
                    edge(
                        index as usize - 1,
                        face[(at + 1) % face.len()].unsigned_abs() as usize - 1,
                    )?;
                }
            }
        }
        for &[a, b] in &self.edges {
            edge(a as usize, b as usize)?;
        }
        Ok((points, breaks))
    }
    pub(crate) fn apply(
        self,
        feature: &mut crate::dxf_read::DxfFeature,
        budget: usize,
    ) -> Result<(), String> {
        let remaining = budget
            .checked_sub(self.vertices.len())
            .ok_or("CAD-mesh overschrijdt het bronbudget")?;
        let (points, breaks) = self.wireframe(remaining)?;
        feature.points = points;
        feature.closed = Some(false);
        let cad = feature.cad.get_or_insert_with(Default::default);
        cad.path_breaks = breaks;
        cad.mesh = Some(std::sync::Arc::new(self));
        Ok(())
    }
}

/// Native files retain indexed sources. Rebuild their derived edge geometry on
/// open; a stale cached point list is never authoritative over source topology.
pub(crate) fn restore_features(
    features: &mut [crate::dxf_read::DxfFeature],
    used: &mut usize,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    for feature in features {
        crate::input::check(cancel)?;
        if let Some(cad) = &mut feature.cad {
            if let Some(mesh) = &cad.mesh {
                *used = used
                    .checked_add(mesh.vertices.len())
                    .ok_or("Te veel CAD-meshpunten")?;
                let (points, breaks) = mesh.wireframe(MAX_POINTS.saturating_sub(*used))?;
                feature.points = points;
                cad.path_breaks = breaks;
            }
        }
        *used = used
            .checked_add(feature.points.len())
            .filter(|v| *v <= MAX_POINTS)
            .ok_or("Te veel CAD-bronpunten")?;
    }
    Ok(())
}
