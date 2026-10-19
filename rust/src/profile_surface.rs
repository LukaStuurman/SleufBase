//! Profile terrain, soil cell, measured polylines and standalone marker stage.
use crate::{
    dxf_document::Document,
    profile::Profile,
    profile_drawing::Drawing,
    python_values::{self, trim},
    template_geometry::{self, Entity, Layer, Paint, Shape},
    Bounds, Dataset, Geometry,
};
use serde_json::Value;
use std::{collections::BTreeMap, sync::atomic::AtomicBool};
pub const MARKER: &str = "SAL-VERWIJZING_PROFIELPUNT-MARKER-SOD";
const TERRAIN: &str = "B-WE-OG-TERREIN_PROFIELLIJN-GD";
const STYLE: &str = "KADASTRALE_LABELS";
pub type Segment = [f64; 4];
pub type TextBox = [f64; 4];
#[derive(Clone, Debug, serde::Serialize)]
pub struct Fill {
    pub text: String,
    pub rgb: u32,
}
#[derive(Default, Debug, serde::Serialize)]
pub struct Obstacles {
    pub hard: Vec<Segment>,
    pub soft: Vec<Segment>,
    pub text: Vec<TextBox>,
}
pub struct Body {
    pub entities: Vec<Entity>,
    pub markers: Vec<Vec<Entity>>,
    pub layers: Vec<Layer>,
    pub obstacles: Obstacles,
}
#[derive(Clone, Copy, Default)]
pub struct Options {
    pub clip_markers: bool,
    pub marker_in_leader: bool,
}
fn finite(v: &[f64]) -> Result<(), String> {
    if v.iter().any(|v| !v.is_finite()) {
        Err("Niet-eindige profielgeometrie".into())
    } else {
        Ok(())
    }
}
fn clean(value: &str) -> Result<String, String> {
    let value = trim(value);
    if value.len() > 4096 || value.chars().any(char::is_control) {
        Err("Ongeldige oppervlaktekst".into())
    } else {
        Ok(value.to_owned())
    }
}
fn value_text(value: Option<&Value>) -> Result<String, String> {
    match value.filter(|v| python_values::truth(v)) {
        Some(v) => clean(&python_values::string(Some(v))?),
        None => Ok(String::new()),
    }
}
fn color(value: Option<&Value>) -> Result<u32, String> {
    let raw = match value.filter(|v| python_values::truth(v)) {
        Some(v) => python_values::string(Some(v))?,
        None => String::new(),
    };
    if raw.len() > 4096 {
        return Ok(0x808080);
    }
    let s = python_values::uppercase(trim(&raw));
    let mut chars = s.chars();
    if chars.next() != Some('#') {
        return Ok(0x808080);
    }
    let mut color = 0u32;
    let mut count = 0;
    for c in chars {
        let digit = c.to_digit(16).or_else(|| crate::object_layout::decimal(c));
        let Some(digit) = digit else {
            return Ok(0x808080);
        };
        if count == 6 {
            return Ok(0x808080);
        }
        color = (color << 4) | digit;
        count += 1;
    }
    Ok(if count == 6 { color } else { 0x808080 })
}
/// Automatic texts are the resolved BGT values. Their geometry/network lookup
/// is a separate stage; explicit custom text/color retains original precedence.
pub fn resolve(metadata: &Value, automatic: &Value, use_custom: bool) -> Result<[Fill; 3], String> {
    let mut fills = Vec::new();
    for key in ["start", "middle", "end"] {
        let automatic = value_text(automatic.get(key))?;
        let automatic = if automatic.is_empty() {
            "INVULLEN".into()
        } else {
            automatic
        };
        let custom = metadata
            .get("template_maaiveld_segments")
            .and_then(|v| v.get(key))
            .filter(|v| v.is_object() && use_custom);
        let text = if let Some(item) = custom {
            let text = value_text(item.get("text"))?;
            if text.is_empty() || python_values::casefold(&text) == "invullen" {
                automatic
            } else {
                text
            }
        } else {
            automatic
        };
        fills.push(Fill {
            text,
            rgb: match custom {
                Some(item) => color(item.get("color"))?,
                None => 0x808080,
            },
        });
    }
    Ok(fills.try_into().unwrap())
}
fn polyline(vertices: Vec<[f64; 2]>, layer: &str, color: Option<u16>, rgb: Option<u32>) -> Entity {
    Entity {
        layer: layer.into(),
        layout: None,
        paint: Paint {
            color,
            true_color: rgb,
            lineweight: Some(0),
        },
        shape: Shape::Polyline {
            vertices,
            closed: false,
        },
    }
}
fn text(content: String, point: [f64; 2], height: f64, rotation: f64, centered: bool) -> Entity {
    Entity {
        layer: "0".into(),
        layout: None,
        paint: Paint::default(),
        shape: Shape::Text {
            text: content,
            insert: point,
            height,
            rotation,
            style: STYLE.into(),
            halign: if centered { 1 } else { 0 },
            valign: if centered { 2 } else { 0 },
        },
    }
}
pub(crate) fn width(text: &str, height: f64) -> f64 {
    height.max(trim(text).chars().count() as f64 * height * 0.58)
}
pub(crate) fn rotated_box(content: &str, p: [f64; 2], height: f64, angle: f64) -> TextBox {
    let length = width(content, height);
    let angle = angle.to_radians();
    let (sin, cos) = angle.sin_cos();
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for w in [-1., 1.] {
        for h in [-1., 1.] {
            let x = p[0] + cos * length * 0.5 * w - sin * height * 0.5 * h;
            let y = p[1] + sin * length * 0.5 * w + cos * height * 0.5 * h;
            bounds[0] = bounds[0].min(x);
            bounds[1] = bounds[1].min(y);
            bounds[2] = bounds[2].max(x);
            bounds[3] = bounds[3].max(y);
        }
    }
    bounds
}
fn fill_text(
    body: &mut Body,
    a: [f64; 2],
    b: [f64; 2],
    content: &str,
    avoid: Option<TextBox>,
) -> Result<(), String> {
    let content = clean(content)?;
    let content = if content.is_empty() {
        "INVULLEN".into()
    } else {
        content
    };
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let length = dx.hypot(dy);
    finite(&[dx, dy, length])?;
    if length <= 1e-9 {
        return Ok(());
    }
    let ux = dx / length;
    let uy = dy / length;
    let mut cx = (a[0] + b[0]) * 0.5;
    let cy = (a[1] + b[1]) * 0.5;
    if let Some(bounds) = avoid.filter(|_| uy.abs() <= 1e-9) {
        let half = width(&content, 0.035) * 0.5;
        cx = cx.max(bounds[2] + half + 0.05).min(b[0] - half);
    }
    let p = [cx - uy * 0.04, cy + ux * 0.04];
    let angle = dy.atan2(dx).to_degrees();
    let bounds = rotated_box(&content, p, 0.035, angle);
    finite(&bounds)?;
    body.entities.push(text(content, p, 0.035, angle, true));
    body.obstacles.text.push(bounds);
    Ok(())
}
pub fn prepare_marker(document: &mut Document, cancel: &AtomicBool) -> Result<(), String> {
    template_geometry::ensure_circle_block(
        document,
        template_geometry::CircleBlock {
            name: MARKER,
            center: [0., -0.5, 0.],
            radius: 0.5,
            layer: "0",
            paint: Paint {
                color: Some(0),
                true_color: None,
                lineweight: Some(0),
            },
        },
        cancel,
    )
}
pub fn clip_segment(
    a: [f64; 2],
    b: [f64; 2],
    left: f64,
    right: f64,
    bottom: f64,
) -> Result<Option<[[f64; 2]; 2]>, String> {
    finite(&[a[0], a[1], b[0], b[1], left, right, bottom])?;
    if left > right {
        return Err("Omgekeerde profielclipgrens".into());
    }
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    finite(&[dx, dy])?;
    let mut low = 0f64;
    let mut high = 1f64;
    for (p, q) in [(-dx, a[0] - left), (dx, right - a[0]), (-dy, a[1] - bottom)] {
        finite(&[q])?;
        if p.abs() <= 1e-12 {
            if q < 0. {
                return Ok(None);
            }
            continue;
        }
        let ratio = q / p;
        if p < 0. {
            if ratio > high {
                return Ok(None);
            }
            low = low.max(ratio);
        } else {
            if ratio < low {
                return Ok(None);
            }
            high = high.min(ratio);
        }
    }
    let (a, b) = (
        [a[0] + low * dx, a[1] + low * dy],
        [a[0] + high * dx, a[1] + high * dy],
    );
    finite(&[a[0], a[1], b[0], b[1]])?;
    Ok(((b[0] - a[0]).hypot(b[1] - a[1]) > 1e-9).then_some([a, b]))
}
pub fn build(
    profile: &Profile<'_>,
    dataset: &Dataset,
    frame: &Drawing,
    tiff: Bounds,
    fills: &[Fill; 3],
    options: Options,
    cancel: &AtomicBool,
) -> Result<Body, String> {
    crate::input::check(cancel)?;
    let l = frame.layout;
    finite(&[
        tiff.min_x,
        tiff.min_y,
        l.start_x,
        l.end_x,
        l.reference_y,
        l.scale,
        profile.reference_level,
    ])?;
    let start_z = profile.start.z.unwrap_or(0.);
    let end_z = profile.end.z.unwrap_or(0.);
    let sy = l.reference_y + (start_z - profile.reference_level) * l.scale;
    let ey = l.reference_y + (end_z - profile.reference_level) * l.scale;
    let left = l.start_x - 2.;
    finite(&[start_z, end_z, sy, ey, left])?;
    let mut body = Body {
        entities: Vec::new(),
        markers: Vec::with_capacity(frame.bands.len()),
        layers: vec![Layer {
            name: TERRAIN.into(),
            color: 8,
            true_color: None,
            lineweight: Some(0),
        }],
        obstacles: Obstacles::default(),
    };
    for y in [l.reference_y, l.top_band_y, l.bottom_band_y, l.bottom_y] {
        body.obstacles.hard.push([l.start_x, y, l.end_x, y]);
    }
    for (a, b, layer, fill) in [
        ([left, sy], [l.start_x, sy], "0", &fills[0]),
        ([l.end_x, ey], [tiff.min_x, ey], "0", &fills[2]),
        ([l.start_x, sy], [l.end_x, ey], TERRAIN, &fills[1]),
    ] {
        if fill.rgb > 0xffffff {
            return Err("Ongeldige maaiveldkleur".into());
        }
        body.entities
            .push(polyline(vec![a, b], layer, None, Some(fill.rgb)));
        body.obstacles.soft.push([a[0], a[1], b[0], b[1]]);
    }
    let maaiveld = format!("Maaiveld: {start_z:.2} N.A.P.");
    let position = [left, sy + 0.035 * 0.45];
    let obstacle = [
        position[0],
        position[1] - 0.035 * 0.25,
        position[0] + width(&maaiveld, 0.035),
        position[1] + 0.035,
    ];
    finite(&obstacle)?;
    body.entities
        .push(text(maaiveld, position, 0.035, 0., false));
    body.obstacles.text.push(obstacle);
    fill_text(
        &mut body,
        [left, sy],
        [l.start_x, sy],
        &fills[0].text,
        Some(obstacle),
    )?;
    fill_text(
        &mut body,
        [l.start_x, sy],
        [l.end_x, ey],
        &fills[1].text,
        None,
    )?;
    fill_text(
        &mut body,
        [l.end_x, ey],
        [tiff.min_x, ey],
        &fills[2].text,
        None,
    )?;
    let soil_bottom = l.title_y + 0.18040671;
    if sy > soil_bottom + 1e-6 {
        body.entities.push(polyline(
            vec![
                [l.start_x, sy],
                [l.start_x - 0.4, sy],
                [l.start_x - 0.4, soil_bottom],
                [l.start_x, soil_bottom],
                [l.start_x, sy],
            ],
            "0",
            None,
            None,
        ));
        body.entities.push(text(
            "INVULLEN".into(),
            [
                (l.start_x - 0.4 + l.start_x) * 0.5,
                (sy + soil_bottom) * 0.5,
            ],
            0.04,
            0.,
            true,
        ));
    }
    let mut layers = BTreeMap::new();
    let mut vertices = 0usize;
    let mut entity_count = body.entities.len();
    for band in &frame.bands {
        crate::input::check(cancel)?;
        let feature = dataset
            .features
            .get(band.feature_index)
            .ok_or("Ontbrekend profielbronobject")?;
        let point = profile
            .points
            .get(band.point_index)
            .filter(|p| {
                p.feature_index == band.feature_index
                    && p.is_endpoint == band.pinned
                    && p.chainage == band.chainage
            })
            .ok_or("Band hoort niet bij dit profiel")?;
        let y = l.reference_y + (band.point_z - profile.reference_level) * l.scale;
        finite(&[band.anchor_x, y, band.marker_scale])?;
        body.obstacles
            .hard
            .push([band.anchor_x, l.reference_y, band.anchor_x, y]);
        let mut entities = Vec::new();
        if !band.pinned {
            if point.layer_name != "0" {
                // Keep the first spelling and the final color, like ezdxf's
                // case-insensitive layer table. Borrow names until declaration.
                let entry = layers
                    .entry(python_values::casefold(&point.layer_name))
                    .or_insert((point.layer_name.as_ref(), 0u16));
                entry.1 = if point.color == 256 { 7 } else { point.color };
            }
            if let Geometry::Polyline(source) = &feature.geometry {
                vertices = vertices
                    .checked_add(source.len())
                    .filter(|n| *n <= 1_000_000)
                    .ok_or("Te veel profielvertices")?;
                let mut projected = Vec::new();
                for (i, v) in source.iter().enumerate() {
                    if i.is_multiple_of(1024) {
                        crate::input::check(cancel)?;
                    }
                    if v.z.is_some() {
                        let p = profile
                            .project(*v, l.start_x, l.reference_y, 0., l.scale, l.scale)
                            .ok_or("Ongeldige profielvertex")?;
                        projected.push(p);
                    }
                }
                if projected.len() >= 2 {
                    for pair in projected.windows(2) {
                        body.obstacles
                            .hard
                            .push([pair[0][0], pair[0][1], pair[1][0], pair[1][1]]);
                    }
                    entities.push(polyline(
                        projected,
                        &point.layer_name,
                        Some(point.color),
                        None,
                    ));
                }
            } else if !options.marker_in_leader {
                let radius = (band.marker_scale * 0.5).max(1e-6);
                let cy = y - radius;
                if options.clip_markers
                    && (band.anchor_x - radius < l.start_x - 1e-9
                        || band.anchor_x + radius > l.end_x + 1e-9
                        || cy - radius < l.reference_y - 1e-9)
                {
                    for i in 0..64 {
                        let points = [i, i + 1].map(|j| {
                            let a = 2. * std::f64::consts::PI * j as f64 / 64.;
                            [band.anchor_x + radius * a.cos(), cy + radius * a.sin()]
                        });
                        if let Some([a, b]) =
                            clip_segment(points[0], points[1], l.start_x, l.end_x, l.reference_y)?
                        {
                            entities.push(polyline(
                                vec![a, b],
                                &point.layer_name,
                                Some(point.color),
                                None,
                            ));
                        }
                    }
                } else {
                    entities.push(Entity {
                        layer: point.layer_name.to_string(),
                        layout: None,
                        paint: Paint {
                            color: Some(point.color),
                            true_color: None,
                            lineweight: Some(0),
                        },
                        shape: Shape::Block {
                            name: MARKER.into(),
                            insert: [band.anchor_x, y],
                            scale: band.marker_scale,
                            attributes: BTreeMap::new(),
                            attribute_placements: BTreeMap::new(),
                        },
                    });
                }
            }
        }
        entity_count += entities.len();
        if entity_count > 100_000 {
            return Err("Te veel profielmarkeringen".into());
        }
        body.markers.push(entities);
    }
    body.layers
        .extend(layers.into_values().map(|(name, color)| Layer {
            name: name.into(),
            color,
            true_color: None,
            lineweight: Some(0),
        }));
    crate::input::check(cancel)?;
    Ok(body)
}
/// Move prepared geometry into the exact original frame/body/point/connector
/// order; caller retains obstacle data for the subsequent leader stage.
pub fn compose(frame: &mut Drawing, mut body: Body) -> Result<(Vec<Layer>, Obstacles), String> {
    let expected = 11 + frame.bands.len() * 3;
    if body.markers.len() != frame.bands.len() || frame.entities.len() < expected {
        return Err("Profieltekenstappen passen niet bij elkaar".into());
    }
    let mut source = std::mem::take(&mut frame.entities).into_iter();
    let mut out: Vec<_> = source.by_ref().take(11).collect();
    out.append(&mut body.entities);
    for mut marker in body.markers {
        out.extend(source.by_ref().take(3));
        out.append(&mut marker);
    }
    out.extend(source);
    frame.entities = out;
    Ok((body.layers, body.obstacles))
}
