//! Stateful decoding of supported DXF entities, reused for modelspace and blocks.
//! Attribute subclass semantics follow ezdxf 1.4.4 entities/attrib.py (MIT,
//! Copyright 2019-2024 Manfred Moitzi; see third_party/ezdxf-LICENSE.txt).
use crate::{
    cad_geometry::{self as g, Affine, ArcCurve, CurvePath, Placement, Segment, V3},
    dxf_read::DxfFeature,
    dxf_tags::Group,
    model::Vertex,
};
use std::collections::BTreeMap;

const MAX_POINTS: usize = 10_000_000;
const MAX_ENTITIES: usize = 2_000_000;
const TOLERANCE: f64 = 0.25;
pub(crate) fn point(groups: &[Group], code: i32) -> Result<V3, String> {
    Ok([
        number(groups, code).ok_or("CAD-punt mist X")?,
        number(groups, code + 10).ok_or("CAD-punt mist Y")?,
        number(groups, code + 20).unwrap_or(0.),
    ])
}
fn normal(groups: &[Group]) -> Result<Affine, String> {
    Affine::ocs([
        number(groups, 210).unwrap_or(0.),
        number(groups, 220).unwrap_or(0.),
        number(groups, 230).unwrap_or(1.),
    ])
}
pub(crate) fn integer(groups: &[Group], code: i32, default: u32) -> Result<u32, String> {
    field(groups, code)
        .map(|v| v.trim().parse().map_err(|_| "Ongeldig CAD-aantal".into()))
        .unwrap_or(Ok(default))
}
fn flags(groups: &[Group]) -> i32 {
    field(groups, 70)
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0)
}

fn spline(groups: &[Group], feature: &mut DxfFeature, budget: usize) -> Result<(), String> {
    fn points(groups: &[Group], base: i32) -> Result<Vec<V3>, String> {
        let mut output = Vec::new();
        let mut current: Option<(V3, bool)> = None;
        for tag in groups {
            if tag.code == base {
                if let Some((p, has_y)) = current.take() {
                    if !has_y {
                        return Err("SPLINE-punt mist Y".into());
                    }
                    output.push(p);
                }
                current = Some((
                    [
                        tag.value
                            .trim()
                            .parse()
                            .map_err(|_| "Ongeldig SPLINE-punt")?,
                        0.,
                        0.,
                    ],
                    false,
                ));
            } else if tag.code == base + 10 || tag.code == base + 20 {
                let (p, has_y) = current.as_mut().ok_or("SPLINE-punt mist X")?;
                let index = if tag.code == base + 10 {
                    *has_y = true;
                    1
                } else {
                    2
                };
                p[index] = tag
                    .value
                    .trim()
                    .parse()
                    .map_err(|_| "Ongeldig SPLINE-punt")?;
            }
        }
        if let Some((p, has_y)) = current {
            if !has_y {
                return Err("SPLINE-punt mist Y".into());
            }
            output.push(p);
        }
        Ok(output)
    }
    fn values(groups: &[Group], code: i32) -> Result<Vec<f64>, String> {
        groups
            .iter()
            .filter(|g| g.code == code)
            .map(|g| {
                g.value
                    .trim()
                    .parse()
                    .map_err(|_| "Ongeldige SPLINE-waarde".to_owned())
            })
            .collect()
    }
    let spline = crate::spline::Spline {
        degree: integer(groups, 71, 3)?,
        flags: integer(groups, 70, 0)?,
        controls: points(groups, 10)?,
        fit_points: points(groups, 11)?,
        knots: values(groups, 40)?,
        weights: values(groups, 41)?,
        start_tangent: field(groups, 12).map(|_| point(groups, 12)).transpose()?,
        end_tangent: field(groups, 13).map(|_| point(groups, 13)).transpose()?,
        normal: groups
            .iter()
            .any(|g| matches!(g.code, 210 | 220 | 230))
            .then(|| {
                [
                    number(groups, 210).unwrap_or(0.),
                    number(groups, 220).unwrap_or(0.),
                    number(groups, 230).unwrap_or(1.),
                ]
            }),
        knot_tolerance: number(groups, 42).unwrap_or(1e-7),
        control_tolerance: number(groups, 43).unwrap_or(1e-7),
        fit_tolerance: number(groups, 44).unwrap_or(1e-10),
    };
    for (code, count) in [
        (72, spline.knots.len()),
        (73, spline.controls.len()),
        (74, spline.fit_points.len()),
    ] {
        if field(groups, code).is_some() && integer(groups, code, 0)? as usize != count {
            return Err("SPLINE-aantallen komen niet overeen met de bronvelden".into());
        }
    }
    let output = spline.flatten(Affine::IDENTITY, TOLERANCE, budget)?;
    feature.points = output.points;
    feature.closed = Some(spline.flags & 1 != 0);
    let cad = feature.cad.get_or_insert_with(Default::default);
    cad.path_breaks = output.breaks;
    cad.spline = Some(Box::new(spline));
    Ok(())
}

fn path(
    points: &[(V3, f64)],
    closed: bool,
    ocs: Affine,
) -> Result<(Vec<Vertex>, Option<Box<CurvePath>>), String> {
    if points.is_empty() {
        return Ok((Vec::new(), None));
    }
    let has_bulge = points
        .iter()
        .take(if closed {
            points.len()
        } else {
            points.len() - 1
        })
        .any(|(_, b)| *b != 0.);
    if !has_bulge {
        let mut result = points
            .iter()
            .map(|(p, _)| ocs.checked_point(*p).map(g::vertex))
            .collect::<Result<Vec<_>, _>>()?;
        if closed && result.len() > 1 {
            result.push(result[0]);
        }
        return Ok((result, None));
    }
    let mut segments = Vec::with_capacity(points.len());
    for i in 0..if closed {
        points.len()
    } else {
        points.len() - 1
    } {
        segments.push(g::bulge(
            points[i].0,
            points[(i + 1) % points.len()].0,
            points[i].1,
            ocs,
        )?);
    }
    let curve = CurvePath { segments };
    let result = curve.flatten(Affine::IDENTITY, TOLERANCE, MAX_POINTS)?;
    Ok((result, Some(Box::new(curve))))
}
fn lwpolyline(groups: &[Group], feature: &mut DxfFeature) -> Result<(), String> {
    let elevation = number(groups, 38).unwrap_or(0.);
    let mut values = Vec::new();
    let mut current: Option<(f64, Option<f64>, f64)> = None;
    for tag in groups {
        match tag.code {
            10 => {
                if let Some((x, y, b)) = current.take() {
                    values.push(([x, y.ok_or("LWPOLYLINE mist Y")?, elevation], b));
                }
                current = Some((
                    tag.value
                        .trim()
                        .parse::<f64>()
                        .map_err(|_| "Ongeldige LWPOLYLINE X")?,
                    None,
                    0.,
                ));
            }
            20 => {
                if let Some((_, y, _)) = &mut current {
                    *y = Some(
                        tag.value
                            .trim()
                            .parse()
                            .map_err(|_| "Ongeldige LWPOLYLINE Y")?,
                    );
                }
            }
            42 => {
                if let Some((_, _, b)) = &mut current {
                    *b = tag
                        .value
                        .trim()
                        .parse()
                        .map_err(|_| "Ongeldige LWPOLYLINE bulge")?;
                }
            }
            _ => {}
        }
    }
    if let Some((x, y, b)) = current {
        values.push(([x, y.ok_or("LWPOLYLINE mist Y")?, elevation], b));
    }
    let closed = flags(groups) & 1 != 0;
    let (points, curve) = path(&values, closed, normal(groups)?)?;
    feature.points = points;
    feature.closed = Some(closed);
    if curve.is_some() {
        feature.cad.get_or_insert_with(Default::default).curve = curve;
    }
    Ok(())
}
fn arc(groups: &[Group], kind: &str, feature: &mut DxfFeature) -> Result<(), String> {
    let center = point(groups, 10)?;
    let ocs = normal(groups)?;
    let radius = number(groups, 40).ok_or("CAD-boog mist straal")?.abs();
    if radius == 0. {
        return Ok(());
    }
    let (start, sweep) = if kind == "CIRCLE" {
        (0., std::f64::consts::TAU)
    } else {
        let a = number(groups, 50).ok_or("ARC mist beginhoek")?;
        let b = number(groups, 51).ok_or("ARC mist eindhoek")?;
        if a == b {
            return Ok(());
        }
        let span = (b - a).rem_euclid(360.);
        (
            a.rem_euclid(360.).to_radians(),
            if span == 0. {
                std::f64::consts::TAU
            } else {
                span.to_radians()
            },
        )
    };
    let (sin, cos) = start.sin_cos();
    let u = ocs.vector([radius * cos, radius * sin, 0.]);
    let v = ocs.vector([-radius * sin, radius * cos, 0.]);
    let curve = CurvePath {
        segments: vec![Segment::Arc(ArcCurve {
            origin: g::finite(g::add(ocs.point(center), u))?,
            u,
            v,
            sweep,
        })],
    };
    feature.points = curve.flatten(Affine::IDENTITY, TOLERANCE, MAX_POINTS)?;
    feature.closed = Some(kind == "CIRCLE");
    feature.cad.get_or_insert_with(Default::default).curve = Some(Box::new(curve));
    Ok(())
}
fn ellipse(groups: &[Group], feature: &mut DxfFeature) -> Result<(), String> {
    let center = point(groups, 10)?;
    let major = point(groups, 11)?;
    let ocs = normal(groups)?;
    let n = ocs.vector([0., 0., 1.]);
    let length = g::length(major);
    let ratio = number(groups, 40).ok_or("ELLIPSE mist asverhouding")?;
    if length == 0. || ratio <= 0. || ratio > 1. {
        return Err("Ongeldige ELLIPSE-assen".into());
    }
    let minor_direction = g::cross(n, major);
    let minor_length = g::length(minor_direction);
    if minor_length == 0. {
        return Err("ELLIPSE-as evenwijdig aan normaal".into());
    }
    let minor = g::scale(minor_direction, length * ratio / minor_length);
    let start = number(groups, 41).unwrap_or(0.);
    let end = number(groups, 42).unwrap_or(std::f64::consts::TAU);
    let mut sweep = (end - start).rem_euclid(std::f64::consts::TAU);
    if sweep == 0. {
        sweep = std::f64::consts::TAU;
    }
    let (s, c) = start.rem_euclid(std::f64::consts::TAU).sin_cos();
    let u = g::add(g::scale(major, c), g::scale(minor, s));
    let v = g::add(g::scale(major, -s), g::scale(minor, c));
    let curve = CurvePath {
        segments: vec![Segment::Arc(ArcCurve {
            origin: g::add(center, u),
            u,
            v,
            sweep,
        })],
    };
    feature.points = curve.flatten(Affine::IDENTITY, TOLERANCE, MAX_POINTS)?;
    feature.cad.get_or_insert_with(Default::default).curve = Some(Box::new(curve));
    Ok(())
}

struct Polyline {
    mesh: Option<crate::dxf_mesh::PolyMesh>,
    feature: DxfFeature,
    points: Vec<(V3, f64)>,
    ocs: Affine,
    elevation: f64,
    flags: i32,
}
#[derive(Default)]
pub(crate) struct Entities {
    pub features: Vec<DxfFeature>,
    pub unsupported: BTreeMap<String, usize>,
    polyline: Option<Polyline>,
    insert: Option<usize>,
    points: usize,
    count: usize,
}
impl Entities {
    pub fn pending(&self) -> bool {
        self.polyline.is_some() || self.insert.is_some()
    }
    fn append(&mut self, feature: DxfFeature) -> Result<(), String> {
        if let Some(cad) = &feature.cad {
            cad.validate_geometry(feature.points.len())?;
        }
        self.points = self
            .points
            .checked_add(
                feature.points.len().saturating_add(
                    feature
                        .cad
                        .as_deref()
                        .and_then(|p| p.mesh.as_ref())
                        .map_or(0, |m| m.vertices.len()),
                ),
            )
            .ok_or("Te veel CAD-punten")?;
        self.count += 1;
        if self.points > MAX_POINTS || self.count > MAX_ENTITIES {
            return Err("DXF heeft te veel punten/entiteiten".into());
        }
        self.features.push(feature);
        Ok(())
    }
    pub fn push(&mut self, kind: &str, groups: &[Group]) -> Result<(), String> {
        if self.polyline.is_some() && kind != "VERTEX" && kind != "SEQEND" {
            return Err("DXF-polyline is onderbroken zonder SEQEND".into());
        }
        if self.insert.is_some() && kind != "ATTRIB" && kind != "SEQEND" {
            return Err("DXF-attributen missen SEQEND".into());
        }
        match kind {
            "POLYLINE" => {
                let f = flags(groups);
                let mut feature = common(groups, kind)?;
                feature.closed = Some(f & 1 != 0);
                self.polyline = Some(Polyline {
                    mesh: if f & (16 | 64) != 0 {
                        Some(crate::dxf_mesh::PolyMesh::new(groups, f)?)
                    } else {
                        None
                    },
                    feature,
                    points: Vec::new(),
                    ocs: if f & (8 | 16 | 64) != 0 {
                        Affine::IDENTITY
                    } else {
                        normal(groups)?
                    },
                    elevation: number(groups, 30).unwrap_or(0.),
                    flags: f,
                });
                return Ok(());
            }
            "VERTEX" => {
                let parent = self.polyline.as_mut().ok_or("DXF VERTEX zonder POLYLINE")?;
                if let Some(mesh) = &mut parent.mesh {
                    return mesh.vertex(groups);
                }
                if parent.points.len() >= MAX_POINTS {
                    return Err("Te veel CAD-polylinepunten".into());
                }
                let mut p = point(groups, 10)?;
                if parent.flags & 8 == 0 {
                    p[2] = parent.elevation;
                }
                parent.points.push((
                    p,
                    if parent.flags & 8 != 0 {
                        0.
                    } else {
                        number(groups, 42).unwrap_or(0.)
                    },
                ));
                return Ok(());
            }
            "SEQEND" => {
                self.insert = None;
                if let Some(mut parent) = self.polyline.take() {
                    if let Some(mesh) = parent.mesh.take() {
                        if mesh.smoothed() {
                            *self.unsupported.entry("Polygonmesh: controlekooi; glad oppervlak nog niet weergegeven".into()).or_default()+=1;
                        }
                        if mesh.style_overrides {
                            *self
                                .unsupported
                                .entry("Mesh: vertex-/vlakeigenschappen nog niet toegepast".into())
                                .or_default() += 1;
                        }
                        mesh.finish(&mut parent.feature, MAX_POINTS.saturating_sub(self.points))?;
                        return self.append(parent.feature);
                    }
                    let transform = if parent.flags & 8 != 0 {
                        Affine::IDENTITY
                    } else {
                        parent.ocs
                    };
                    let (points, curve) = path(&parent.points, parent.flags & 1 != 0, transform)?;
                    parent.feature.points = points;
                    if curve.is_some() {
                        parent
                            .feature
                            .cad
                            .get_or_insert_with(Default::default)
                            .curve = curve;
                    }
                    if parent.feature.points.len() >= 2 {
                        self.append(parent.feature)?;
                    }
                }
                return Ok(());
            }
            "ATTRIB" => {
                let index = self.insert.ok_or("DXF ATTRIB zonder INSERT-sequentie")?;
                let name = field(groups, 2).ok_or("DXF ATTRIB mist tagnaam")?;
                self.features[index]
                    .cad
                    .get_or_insert_with(Default::default)
                    .attributes
                    .insert(name.to_owned(), field(groups, 1).unwrap_or("").to_owned());
                let mut feature = common(groups, kind)?;
                text(groups, kind, &mut feature)?;
                if self.count >= MAX_ENTITIES {
                    return Err("Te veel CAD-attributen".into());
                }
                self.count += 1;
                self.features[index]
                    .cad
                    .get_or_insert_with(Default::default)
                    .linked_attributes
                    .push(feature);
                return Ok(());
            }
            _ => {}
        }
        if !matches!(
            kind,
            "LINE"
                | "LWPOLYLINE"
                | "CIRCLE"
                | "ARC"
                | "ELLIPSE"
                | "SPLINE"
                | "POINT"
                | "INSERT"
                | "MULTILEADER"
                | "MLEADER"
                | "3DFACE"
                | "SOLID"
                | "TRACE"
                | "MESH"
                | "TEXT"
                | "ATTDEF"
        ) {
            *self.unsupported.entry(kind.to_owned()).or_default() += 1;
            return Ok(());
        }
        let mut feature = common(groups, kind)?;
        match kind {
            "TEXT" | "ATTDEF" => text(groups, kind, &mut feature)?,
            "LINE" => {
                feature.points = vec![g::vertex(point(groups, 10)?), g::vertex(point(groups, 11)?)]
            }
            "POINT" => {
                let p = g::vertex(point(groups, 10)?);
                feature.points = vec![p];
            }
            "LWPOLYLINE" => lwpolyline(groups, &mut feature)?,
            "CIRCLE" | "ARC" => arc(groups, kind, &mut feature)?,
            "ELLIPSE" => ellipse(groups, &mut feature)?,
            "SPLINE" => spline(groups, &mut feature, MAX_POINTS.saturating_sub(self.points))?,
            "3DFACE" | "SOLID" | "TRACE" => {
                crate::dxf_mesh::quad(
                    groups,
                    kind,
                    &mut feature,
                    MAX_POINTS.saturating_sub(self.points),
                )?;
                if kind != "3DFACE" && number(groups, 39).is_some_and(|v| v != 0.) {
                    *self
                        .unsupported
                        .entry("SOLID/TRACE: dikte-extrusie nog niet weergegeven".into())
                        .or_default() += 1;
                }
            }
            "MESH" => {
                if crate::dxf_mesh::mesh(
                    groups,
                    &mut feature,
                    MAX_POINTS.saturating_sub(self.points),
                )? {
                    *self
                        .unsupported
                        .entry("MESH: controlekooi; subdivisie nog niet weergegeven".into())
                        .or_default() += 1;
                }
            }
            "INSERT" => {
                feature.block_name = field(groups, 2).ok_or("INSERT mist bloknaam")?.to_owned();
                let placement = Placement {
                    insert: point(groups, 10)?,
                    normal: [
                        number(groups, 210).unwrap_or(0.),
                        number(groups, 220).unwrap_or(0.),
                        number(groups, 230).unwrap_or(1.),
                    ],
                    scale: [
                        number(groups, 41).unwrap_or(1.),
                        number(groups, 42).unwrap_or(1.),
                        number(groups, 43).unwrap_or(1.),
                    ],
                    rotation: number(groups, 50).unwrap_or(0.),
                    columns: integer(groups, 70, 1)?,
                    rows: integer(groups, 71, 1)?,
                    column_spacing: number(groups, 44).unwrap_or(0.),
                    row_spacing: number(groups, 45).unwrap_or(0.),
                };
                placement.validate()?;
                let p = g::vertex(normal(groups)?.checked_point(placement.insert)?);
                feature.points = vec![p, p];
                feature.cad.get_or_insert_with(Default::default).placement =
                    Some(Box::new(placement));
            }
            "MULTILEADER" | "MLEADER" => {
                let (content, tip) = mleader(groups)?;
                feature.raw_text = content;
                if let Some(tip) = tip {
                    feature.points = vec![tip, tip];
                }
            }
            _ => unreachable!(),
        }
        if feature.points.len() >= 2
            || (kind == "POINT" && feature.points.len() == 1)
            || feature.cad.as_deref().is_some_and(|p| p.mesh.is_some())
            || kind == "TEXT"
            || kind == "ATTDEF"
        {
            self.append(feature)?;
            if kind == "INSERT" && crate::cad_space::attributes_follow(groups)? {
                self.insert = Some(self.features.len() - 1);
            }
        }
        Ok(())
    }
}
pub(crate) fn field<'a>(groups: &'a [Group], code: i32) -> Option<&'a str> {
    groups
        .iter()
        .find(|g| g.code == code)
        .map(|g| g.value.as_ref())
}
pub(crate) fn number(groups: &[Group], code: i32) -> Option<f64> {
    field(groups, code)?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|x| x.is_finite())
}
pub(crate) fn vertex(groups: &[Group], xcode: i32, ycode: i32, zcode: i32) -> Option<Vertex> {
    Some(Vertex {
        x: number(groups, xcode)?,
        y: number(groups, ycode)?,
        z: number(groups, zcode),
    })
}
fn common(groups: &[Group], kind: &str) -> Result<DxfFeature, String> {
    Ok(DxfFeature {
        entity_type: kind.to_owned(),
        handle: field(groups, 5).unwrap_or("").into(),
        layer: field(groups, 8).unwrap_or("0").into(),
        color_aci: field(groups, 62)
            .and_then(|v| v.trim().parse::<i32>().ok())
            .unwrap_or(256),
        points: Vec::new(),
        raw_text: String::new(),
        block_name: String::new(),
        closed: None,
        cad: crate::cad::properties(groups)?,
    })
}

/// Attribute text codes live in AcDbText, whereas flags/valign are in
/// AcDbAttribute. R2018 repeats 71/72/11 after that subclass; never use them
/// as text generation/alignment codes. R12 has no subclass markers.
fn subclass<'a, 'b>(groups: &'a [Group<'b>], name: &str) -> &'a [Group<'b>] {
    if let Some(at) = groups.iter().position(|g| g.code == 100 && g.value == name) {
        let rest = &groups[at + 1..];
        let end = rest
            .iter()
            .position(|g| g.code == 100 || g.code == 101)
            .unwrap_or(rest.len());
        &rest[..end]
    } else {
        let end = groups
            .iter()
            .position(|g| g.code == 101 || g.code == 100 && g.value == "AcDbXrecord")
            .unwrap_or(groups.len());
        &groups[..end]
    }
}
fn text(groups: &[Group], kind: &str, feature: &mut DxfFeature) -> Result<(), String> {
    let attribute_class = if kind == "ATTDEF" {
        "AcDbAttributeDefinition"
    } else {
        "AcDbAttribute"
    };
    let attributed = matches!(kind, "ATTRIB" | "ATTDEF");
    let data = if attributed {
        subclass(groups, "AcDbText")
    } else {
        groups
    };
    let attr = if attributed {
        Some(subclass(groups, attribute_class))
    } else {
        None
    };
    let layout = crate::dxf_text_metrics::Text {
        content: field(data, 1).unwrap_or("").to_owned(),
        insert: point(data, 10)?,
        align_point: field(data, 11).map(|_| point(data, 11)).transpose()?,
        height: number(data, 40).unwrap_or(2.5),
        width: number(data, 41).unwrap_or(1.),
        rotation: number(data, 50).unwrap_or(0.),
        oblique: number(data, 51).unwrap_or(0.),
        generation: u8::try_from(integer(data, 71, 0)?).map_err(|_| "Ongeldige TEXT-spiegeling")?,
        halign: u8::try_from(integer(data, 72, 0)?).map_err(|_| "Ongeldige TEXT-uitlijning")?,
        valign: u8::try_from(integer(
            attr.unwrap_or(data),
            if attr.is_some() { 74 } else { 73 },
            0,
        )?)
        .map_err(|_| "Ongeldige TEXT-uitlijning")?,
        normal: [
            number(data, 210).unwrap_or(0.),
            number(data, 220).unwrap_or(0.),
            number(data, 230).unwrap_or(1.),
        ],
    };
    feature.raw_text = layout.content.clone();
    let attribute = if let Some(attr) = attr {
        let extra = groups
            .iter()
            .position(|g| g.code == 101 || g.code == 100 && g.value == "AcDbXrecord")
            .map_or_else(Vec::new, |at| {
                groups[at..]
                    .iter()
                    .map(|g| crate::cad::Tag {
                        code: g.code,
                        value: g.value.to_string(),
                    })
                    .collect()
            });
        Some(Box::new(crate::cad_text::Attribute {
            tag: field(attr, 2).ok_or("DXF ATTRIB mist tagnaam")?.into(),
            prompt: (kind == "ATTDEF").then(|| field(attr, 3).unwrap_or("").to_owned()),
            flags: integer(attr, 70, 0)?,
            field_length: integer(attr, 73, 0)?,
            lock_position: attr
                .iter()
                .rev()
                .find(|g| g.code == 280)
                .is_some_and(|g| g.value.trim() == "1"),
            attribute_type: if groups
                .iter()
                .any(|g| g.code == 100 && g.value == attribute_class)
            {
                u8::try_from(integer(attr, 71, 1)?).map_err(|_| "Ongeldig ATTRIB-type")?
            } else {
                1
            },
            extra,
        }))
    } else {
        None
    };
    // Without subclasses a 71 is the R12 text-generation flag, not the
    // R2018 attribute type (which is located in AcDbAttribute).
    let source = crate::cad_text::Source {
        layout,
        style: field(data, 7).unwrap_or("Standard").into(),
        font: String::new(),
        big_font: String::new(),
        contours: Default::default(),
        attribute,
    };
    source.validate()?;
    let cad = feature.cad.get_or_insert_with(Default::default);
    if source
        .attribute
        .as_deref()
        .is_some_and(|a| a.flags & 1 != 0)
    {
        cad.invisible = true;
    }
    cad.text = Some(Box::new(source));
    Ok(())
}

/// Read MULTILEADER scopes instead of confusing the text/landing base point
/// with a measured arrow. The same group codes have different meanings inside
/// CONTEXT_DATA, LEADER and LEADER_LINE; 304 is both MTEXT and a scope marker.
fn mleader(groups: &[Group]) -> Result<(String, Option<Vertex>), String> {
    if !groups
        .iter()
        .any(|g| g.code == 300 && g.value == "CONTEXT_DATA{")
    {
        // Keep compatibility with simple flat vendor exports and older projects.
        let content = groups
            .iter()
            .filter(|g| g.code == 304)
            .map(|g| g.value.as_ref())
            .collect::<Vec<_>>()
            .join("");
        return Ok((content, vertex(groups, 10, 20, 30)));
    }
    let invalid = || "Ongeldige geneste MULTILEADER-context".to_owned();
    let mut scope = Vec::new();
    let mut content = String::new();
    let mut tip = None;
    let mut line_start = 0;
    for (index, g) in groups.iter().enumerate() {
        match (g.code, g.value.as_ref()) {
            (300, "CONTEXT_DATA{") => {
                if !scope.is_empty() {
                    return Err(invalid());
                }
                scope.push(300);
            }
            (302, "LEADER{") => {
                if scope.as_slice() != [300] {
                    return Err(invalid());
                }
                scope.push(302);
            }
            (304, "LEADER_LINE{") => {
                if scope.as_slice() != [300, 302] {
                    return Err(invalid());
                }
                scope.push(304);
                line_start = index + 1;
            }
            (305, "}") => {
                if scope.pop() != Some(304) {
                    return Err(invalid());
                }
                if tip.is_none() {
                    let line = &groups[line_start..index];
                    if let Some(first) = line.iter().position(|g| g.code == 10) {
                        let end = line[first + 1..]
                            .iter()
                            .position(|g| g.code == 10)
                            .map_or(line.len(), |next| first + 1 + next);
                        tip = Some(vertex(&line[first..end], 10, 20, 30).ok_or_else(invalid)?);
                    }
                }
            }
            (303, "}") => {
                if scope.pop() != Some(302) {
                    return Err(invalid());
                }
            }
            (301, "}") => {
                if scope.pop() != Some(300) {
                    return Err(invalid());
                }
            }
            (304, text) if scope.as_slice() == [300] => content.push_str(text),
            _ => {}
        }
    }
    if !scope.is_empty() {
        return Err(invalid());
    }
    Ok((content, tip))
}
