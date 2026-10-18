//! DXF surface decoding: OCS quads and WCS polygon/polyface/control meshes.
use crate::{
    cad_geometry as g,
    cad_mesh::{Mesh, Source},
    dxf_entities::{field, integer, number, point},
    dxf_read::DxfFeature,
    dxf_tags::Group,
};
const MAX_POINTS: usize = 10_000_000;
const MAX_FACES: usize = 2_000_000;

fn empty(source: Source) -> Mesh {
    Mesh {
        source,
        vertices: vec![],
        faces: vec![],
        edges: vec![],
        creases: vec![],
        subdivision: 0,
        vertex_metadata: Default::default(),
        face_metadata: Default::default(),
    }
}
pub(crate) fn quad(
    groups: &[Group],
    kind: &str,
    feature: &mut DxfFeature,
    budget: usize,
) -> Result<(), String> {
    let normal = [
        number(groups, 210).unwrap_or(0.),
        number(groups, 220).unwrap_or(0.),
        number(groups, 230).unwrap_or(1.),
    ];
    let thickness = number(groups, 39).unwrap_or(0.);
    let source = match kind {
        "3DFACE" => Source::Face {
            invisible_edges: integer(groups, 70, 0)?,
        },
        "SOLID" => Source::Solid { normal, thickness },
        _ => Source::Trace { normal, thickness },
    };
    let mut mesh = empty(source);
    let mut vertices = [
        point(groups, 10)?,
        point(groups, 11)?,
        point(groups, 12)?,
        [0.; 3],
    ];
    vertices[3] = if field(groups, 13).is_some() {
        point(groups, 13)?
    } else {
        vertices[2]
    };
    if kind != "3DFACE" {
        vertices.swap(2, 3);
        let ocs = g::Affine::ocs(normal)?;
        for p in &mut vertices {
            *p = ocs.checked_point(*p)?;
        }
    }
    mesh.vertices = vertices.into_iter().map(g::vertex).collect();
    let invisible = if kind == "3DFACE" {
        integer(groups, 70, 0)?
    } else {
        0
    };
    mesh.faces.push(
        (0..4)
            .map(|i| {
                if invisible & (1 << i) != 0 {
                    -(i + 1)
                } else {
                    i + 1
                }
            })
            .collect(),
    );
    mesh.apply(feature, budget)
}
pub(crate) struct PolyMesh {
    mesh: Mesh,
    pub style_overrides: bool,
}
impl PolyMesh {
    pub(crate) fn new(groups: &[Group], flags: i32) -> Result<Self, String> {
        let source = if flags & 64 != 0 {
            Source::Polyface {
                flags: flags as u32,
                declared: [integer(groups, 71, 0)?, integer(groups, 72, 0)?],
            }
        } else {
            Source::Polygon {
                flags: flags as u32,
                size: [integer(groups, 71, 0)?, integer(groups, 72, 0)?],
                density: [integer(groups, 73, 0)?, integer(groups, 74, 0)?],
                smooth_type: integer(groups, 75, 0)?,
            }
        };
        Ok(Self {
            mesh: empty(source),
            style_overrides: false,
        })
    }
    pub(crate) fn vertex(&mut self, groups: &[Group]) -> Result<(), String> {
        let metadata: Vec<_> = groups
            .iter()
            .filter(|g| match g.code {
                6 => !g.value.eq_ignore_ascii_case("BYLAYER"),
                8 => g.value != "0",
                60 => g.value.trim() != "0",
                62 => g.value.trim() != "256",
                370 => g.value.trim() != "-1",
                420 | 1000..=1071 => true,
                _ => false,
            })
            .map(|g| crate::cad::Tag {
                code: g.code,
                value: g.value.to_string(),
            })
            .collect();
        self.style_overrides |= !metadata.is_empty();
        let flags = integer(groups, 70, 0)?;
        if matches!(self.mesh.source, Source::Polyface { .. }) && flags & 64 == 0 {
            let mut face = Vec::with_capacity(4);
            for code in 71..=74 {
                let index: i32 = field(groups, code)
                    .unwrap_or("0")
                    .trim()
                    .parse()
                    .map_err(|_| "Ongeldige polyface-index")?;
                if index == 0 {
                    break;
                }
                face.push(index);
            }
            if face.len() < 3 {
                return Err("Polyface-vlak heeft minder dan drie indices".into());
            }
            if self.mesh.faces.len() >= MAX_FACES {
                return Err("Te veel polyface-vlakken".into());
            }
            if !metadata.is_empty() {
                self.mesh
                    .face_metadata
                    .insert(self.mesh.faces.len(), metadata);
            }
            self.mesh.faces.push(face);
        } else {
            if self.mesh.vertices.len() >= MAX_POINTS {
                return Err("Te veel CAD-meshvertices".into());
            }
            if !metadata.is_empty() {
                self.mesh
                    .vertex_metadata
                    .insert(self.mesh.vertices.len(), metadata);
            }
            self.mesh.vertices.push(g::vertex(point(groups, 10)?));
        }
        Ok(())
    }
    pub(crate) fn finish(mut self, feature: &mut DxfFeature, budget: usize) -> Result<(), String> {
        if let Source::Polygon {
            flags,
            size: [m, n],
            ..
        } = self.mesh.source
        {
            if m < 2 || n < 2 || u64::from(m) * u64::from(n) != self.mesh.vertices.len() as u64 {
                return Err("CAD-polygonmesh heeft ongeldige rasterafmetingen".into());
            }
            let rows = if flags & 1 != 0 { m } else { m - 1 };
            let cols = if flags & 32 != 0 { n } else { n - 1 };
            if u64::from(rows) * u64::from(cols) > MAX_FACES as u64 {
                return Err("Te veel polygonmesh-vlakken".into());
            }
            for a in 0..rows {
                for b in 0..cols {
                    let c = (a + 1) % m;
                    let d = (b + 1) % n;
                    self.mesh.faces.push(
                        [a * n + b, a * n + d, c * n + d, c * n + b]
                            .map(|i| (i + 1) as i32)
                            .to_vec(),
                    );
                }
            }
        }
        self.mesh.apply(feature, budget)
    }
    pub(crate) fn smoothed(&self) -> bool {
        matches!(
            self.mesh.source,
            Source::Polygon {
                smooth_type: 5 | 6 | 8,
                ..
            }
        )
    }
}

fn uint(tag: &Group) -> Result<usize, String> {
    tag.value
        .trim()
        .parse()
        .map_err(|_| "Ongeldig CAD-meshaantal of index".into())
}
/// Consume the indexed lists by their own count and group code; group 90 also
/// occurs after the lists and must never be interpreted as a face/edge index.
pub(crate) fn mesh(
    groups: &[Group],
    feature: &mut DxfFeature,
    budget: usize,
) -> Result<bool, String> {
    let start = groups
        .iter()
        .position(|g| g.code == 92)
        .ok_or("MESH mist vertexaantal")?;
    let mut at = start;
    fn take<'a, 'b>(
        groups: &'a [Group<'b>],
        at: &mut usize,
        code: i32,
    ) -> Result<&'a Group<'b>, String> {
        let tag = groups
            .get(*at)
            .filter(|g| g.code == code)
            .ok_or_else(|| format!("MESH mist groep {code} in zijn gegevenslijst"))?;
        *at += 1;
        Ok(tag)
    }
    let count = uint(take(groups, &mut at, 92)?)?;
    if count > MAX_POINTS || count > groups.len().saturating_sub(at) / 3 {
        return Err("Ongeldig MESH-vertexaantal".into());
    }
    let mut mesh = empty(Source::Mesh {
        version: integer(groups, 71, 2)?,
        blend_crease: integer(groups, 72, 0)? != 0,
    });
    mesh.subdivision = integer(groups, 91, 0)?;
    for _ in 0..count {
        let mut p = [0.; 3];
        for (i, code) in [10, 20, 30].into_iter().enumerate() {
            p[i] = take(groups, &mut at, code)?
                .value
                .trim()
                .parse()
                .map_err(|_| "Ongeldige MESH-coördinaat")?;
        }
        mesh.vertices.push(g::vertex(g::finite(p)?));
    }
    let count = uint(take(groups, &mut at, 93)?)?;
    let end = at
        .checked_add(count)
        .filter(|end| *end <= groups.len())
        .ok_or("Ongeldige MESH-vlaklijstlengte")?;
    while at < end {
        let n = uint(take(groups, &mut at, 90)?)?;
        if n < 3 || n > end - at || mesh.faces.len() >= MAX_FACES {
            return Err("Ongeldige MESH-vlaklengte".into());
        }
        let mut face = Vec::new();
        for _ in 0..n {
            let index = uint(take(groups, &mut at, 90)?)?;
            if index >= mesh.vertices.len() {
                return Err("MESH-vlakindex buiten vertexlijst".into());
            }
            face.push((index + 1) as i32);
        }
        mesh.faces.push(face);
    }
    let count = uint(take(groups, &mut at, 94)?)?;
    if count > MAX_POINTS / 2 || count > groups.len().saturating_sub(at) / 2 {
        return Err("Ongeldig MESH-randaantal".into());
    }
    for _ in 0..count {
        let a = uint(take(groups, &mut at, 90)?)?;
        let b = uint(take(groups, &mut at, 90)?)?;
        if a >= mesh.vertices.len() || b >= mesh.vertices.len() {
            return Err("MESH-randindex buiten vertexlijst".into());
        }
        mesh.edges.push([a as u32, b as u32]);
    }
    let count = uint(take(groups, &mut at, 95)?)?;
    if count > groups.len().saturating_sub(at) || count != mesh.edges.len() {
        return Err("MESH-vouwgewichten passen niet bij de randen".into());
    }
    for _ in 0..count {
        mesh.creases.push(
            take(groups, &mut at, 140)?
                .value
                .trim()
                .parse()
                .map_err(|_| "Ongeldig MESH-vouwgewicht")?,
        );
    }
    let overrides = groups
        .get(at)
        .filter(|g| g.code == 90)
        .map(uint)
        .transpose()?
        .unwrap_or(0);
    if overrides != 0 {
        return Err(
            "MESH met overschreven subobjecteigenschappen wordt nog niet ondersteund".into(),
        );
    }
    let smoothed = mesh.subdivision > 0;
    mesh.apply(feature, budget)?;
    Ok(smoothed)
}
