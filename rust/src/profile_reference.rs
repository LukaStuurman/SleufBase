//! Original reference point resolution, RD labels and profile indicators.
use crate::{
    profile::Profile,
    profile_drawing::{self, Layout, LAYER},
    profile_surface::{self, Body, Segment, TextBox},
    python_values,
    template_geometry::{Entity, Paint, Shape},
};
use serde_json::Value;
use std::sync::atomic::AtomicBool;
#[derive(Debug, serde::Serialize)]
pub struct Reference {
    pub raw: [f64; 2],
    pub chainage: f64,
    pub description: String,
}
pub struct Indicator {
    pub entities: Vec<Entity>,
    pub segments: Vec<Segment>,
    pub boxes: Vec<TextBox>,
}
fn finite(values: &[f64]) -> Result<(), String> {
    if values.iter().any(|v| !v.is_finite()) {
        Err("Niet-eindig profielreferentiepunt".into())
    } else {
        Ok(())
    }
}
pub fn rd_coordinate(value: f64) -> Result<String, String> {
    finite(&[value])?;
    let value = value.round_ties_even();
    Ok(if value == 0. {
        "0".into()
    } else {
        format!("{value:.0}")
    })
}
pub fn resolve(profile: &Profile<'_>, metadata: &Value) -> Result<Reference, String> {
    let selected = metadata
        .get("template_reference_point")
        .filter(|v| v.is_object())
        .and_then(|v| {
            Some([
                python_values::number(v.get("x"))?,
                python_values::number(v.get("y"))?,
            ])
        });
    let raw = selected.unwrap_or([profile.start.x, profile.start.y]);
    finite(&[
        raw[0],
        raw[1],
        profile.axis_length,
        profile.axis_dx,
        profile.axis_dy,
        profile.start.x,
        profile.start.y,
    ])?;
    let chainage = if profile.axis_length <= 1e-9 {
        0.
    } else {
        ((raw[0] - profile.start.x) * profile.axis_dx
            + (raw[1] - profile.start.y) * profile.axis_dy)
            / profile.axis_length
    };
    finite(&[chainage])?;
    Ok(Reference {
        raw,
        chainage,
        description: format!("RD {}, {}", rd_coordinate(raw[0])?, rd_coordinate(raw[1])?),
    })
}
impl Reference {
    pub fn display(&self, metadata: &Value, center: [f64; 2]) -> Result<[f64; 2], String> {
        let degrees = python_values::number(metadata.get("rotation_degrees")).unwrap_or(0.);
        finite(&[degrees, self.raw[0], self.raw[1]])?;
        if degrees.abs() < 1e-9 {
            return Ok(self.raw);
        }
        finite(&center)?;
        let angle = degrees.to_radians();
        let x = self.raw[0] - center[0];
        let y = self.raw[1] - center[1];
        let result = [
            center[0] + x * angle.cos() - y * angle.sin(),
            center[1] + x * angle.sin() + y * angle.cos(),
        ];
        finite(&result)?;
        Ok(result)
    }
}
fn polyline(vertices: Vec<[f64; 2]>) -> Entity {
    Entity {
        layer: LAYER.into(),
        layout: None,
        paint: Paint {
            color: Some(8),
            true_color: None,
            lineweight: Some(0),
        },
        shape: Shape::Polyline {
            vertices,
            closed: false,
        },
    }
}
fn text(content: String, point: [f64; 2]) -> Entity {
    Entity {
        layer: LAYER.into(),
        layout: None,
        paint: Paint {
            color: Some(8),
            ..Default::default()
        },
        shape: Shape::Text {
            text: content,
            insert: point,
            height: 0.03,
            rotation: 0.,
            style: "KADASTRALE_LABELS".into(),
            halign: 1,
            valign: 2,
        },
    }
}
fn terrain(chainage: f64, axis: f64, start: f64, end: f64) -> f64 {
    if chainage <= 0. || axis <= 1e-9 {
        start
    } else if chainage >= axis {
        end
    } else {
        start + (end - start) * (chainage / axis)
    }
}
pub fn draw(
    profile: &Profile<'_>,
    layout: Layout,
    right_edge: f64,
    reference: &Reference,
    cancel: &AtomicBool,
) -> Result<Indicator, String> {
    crate::input::check(cancel)?;
    let mut result = Indicator {
        entities: Vec::new(),
        segments: Vec::new(),
        boxes: Vec::new(),
    };
    finite(&[
        layout.scale,
        layout.start_x,
        layout.end_x,
        layout.reference_y,
        profile.reference_level,
        profile.start.z.unwrap_or(0.),
        profile.end.z.unwrap_or(0.),
        profile.axis_length,
        right_edge,
        reference.chainage,
    ])?;
    if layout.scale <= 1e-9 {
        return Ok(result);
    }
    let left = layout.start_x - 2.;
    let low = (left - layout.start_x) / layout.scale;
    let high = profile.axis_length + (right_edge - layout.end_x) / layout.scale;
    let sy = layout.reference_y
        + (profile.start.z.unwrap_or(0.) - profile.reference_level) * layout.scale;
    let ey =
        layout.reference_y + (profile.end.z.unwrap_or(0.) - profile.reference_level) * layout.scale;
    finite(&[left, low, high, sy, ey])?;
    let chainage = reference.chainage;
    if low <= chainage && chainage <= high {
        let x = layout.start_x + chainage * layout.scale;
        let y = terrain(chainage, profile.axis_length, sy, ey);
        finite(&[x, y])?;
        result.entities.push(Entity {
            layer: LAYER.into(),
            layout: None,
            paint: Paint {
                color: Some(8),
                true_color: None,
                lineweight: Some(0),
            },
            shape: Shape::Circle {
                center: [x, y],
                radius: 0.04,
            },
        });
        let reach = 0.04 * 0.98;
        result
            .entities
            .push(polyline(vec![[x - reach, y], [x + reach, y]]));
        result
            .entities
            .push(polyline(vec![[x, y - reach], [x, y + reach]]));
        result.boxes.push([x - 0.04, y - 0.04, x + 0.04, y + 0.04]);
        let label = "Referentiepunt";
        let width = profile_surface::width(label, 0.03);
        let (dx, dy) = if chainage < 0. {
            (0.22, 0.08)
        } else if chainage > profile.axis_length {
            (-0.22, 0.08)
        } else {
            let nx = -(ey - sy);
            let ny = layout.end_x - layout.start_x;
            let n = nx.hypot(ny);
            let (mut ux, mut uy) = if n <= 1e-9 {
                (0., 1.)
            } else {
                (nx / n, ny / n)
            };
            if uy < 0. {
                ux = -ux;
                uy = -uy;
            }
            if ux.abs() < 0.25 {
                ux = if chainage <= profile.axis_length * 0.5 {
                    0.35
                } else {
                    -0.35
                };
                let n = ux.hypot(uy);
                if n > 1e-9 {
                    ux /= n;
                    uy /= n;
                }
            }
            let mut delta = (ux * 0.22, uy * 0.22);
            if delta.0.hypot(delta.1) <= 1e-9 {
                delta = (0.06, 0.08);
            }
            delta
        };
        let length = dx.hypot(dy);
        let a = [x + dx / length * 0.04, y + dy / length * 0.04];
        let b = [a[0] + dx, a[1] + dy];
        let p = if chainage < 0. {
            [b[0] + 0.05 + width * 0.5, b[1]]
        } else if chainage > profile.axis_length {
            [b[0] - 0.05 - width * 0.5, b[1]]
        } else {
            [(a[0] + b[0]) * 0.5, a[1].max(b[1]) + 0.025]
        };
        finite(&[a[0], a[1], b[0], b[1], p[0], p[1]])?;
        result.entities.push(polyline(vec![a, b]));
        result.segments.push([a[0], a[1], b[0], b[1]]);
        result.entities.push(text(label.into(), p));
        result
            .boxes
            .push(profile_surface::rotated_box(label, p, 0.03, 0.));
    } else {
        let is_left = chainage < low;
        let boundary = if is_left { low } else { high };
        let x = if is_left { left } else { right_edge };
        let y = terrain(boundary, profile.axis_length, sy, ey);
        let distance = if is_left {
            low - chainage
        } else {
            chainage - high
        };
        finite(&[distance, y])?;
        if distance <= 1e-9 {
            return Ok(result);
        }
        let sign = if is_left { 1. } else { -1. };
        let tail = x + sign * 0.18;
        let points = vec![
            [tail, y],
            [x, y],
            [x + sign * 0.05, y + 0.05 * 0.7],
            [x, y],
            [x + sign * 0.05, y - 0.05 * 0.7],
        ];
        result.segments.extend([
            [tail, y, x, y],
            [points[2][0], points[2][1], x, y],
            [points[4][0], points[4][1], x, y],
        ]);
        let label = format!("{} m", profile_drawing::format_distance(distance)?);
        let width = profile_surface::width(&label, 0.03);
        let cx = if is_left {
            (layout.start_x - width * 0.5).min(tail + 0.05 + width * 0.5)
        } else {
            (layout.end_x + width * 0.5).max(tail - 0.05 - width * 0.5)
        };
        let p = [cx, y + 0.08];
        finite(&p)?;
        result.entities.push(polyline(points));
        result.entities.push(text(label.clone(), p));
        result
            .boxes
            .push(profile_surface::rotated_box(&label, p, 0.03, 0.));
    }
    crate::input::check(cancel)?;
    for b in &result.boxes {
        finite(b)?;
    }
    Ok(result)
}
impl Indicator {
    pub fn append(self, body: &mut Body) {
        body.entities.extend(self.entities);
        body.obstacles.soft.extend(self.segments);
        body.obstacles.text.extend(self.boxes);
    }
}
