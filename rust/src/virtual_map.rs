//! Virtual-layer map selection, whole-layer translation and raw RD references.
use crate::{model::Vertex, raster_points, virtual_trench::VirtualTrench};
use serde_json::json;

/// Python's case-insensitive original name pattern and Unicode-14 decimal
/// suffixes, without a fixed-width integer overflow or newer Unicode digits.
pub fn next_name(names: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    let mut maximum = "0".to_owned();
    for name in names {
        let mut chars = crate::python_values::trim(name.as_ref()).chars();
        let matches = "virtuele proefsleuf".chars().all(|expected| {
            let actual = chars.next().map(|c| match c {
                'İ' | 'ı' => 'i',
                'ſ' => 's',
                c => c.to_ascii_lowercase(),
            });
            actual == Some(expected)
        });
        if !matches {
            continue;
        }
        let tail = chars.collect::<String>();
        let number = if tail.is_empty() {
            Some("1".into())
        } else {
            tail.strip_prefix(' ')
                .filter(|s| !s.is_empty())
                .and_then(|s| {
                    s.chars()
                        .map(crate::object_layout::decimal)
                        .collect::<Option<Vec<_>>>()
                })
                .map(|digits| {
                    digits
                        .into_iter()
                        .map(|d| char::from(b'0' + d as u8))
                        .collect::<String>()
                })
        };
        let Some(number) = number else { continue };
        let digits = number.trim_start_matches('0');
        let digits = if digits.is_empty() { "0" } else { digits };
        if digits.len() > maximum.len()
            || digits.len() == maximum.len() && digits > maximum.as_str()
        {
            maximum = digits.into();
        }
    }
    let mut digits = maximum.into_bytes();
    let mut carry = true;
    for d in digits.iter_mut().rev() {
        if *d == b'9' {
            *d = b'0';
        } else {
            *d += 1;
            carry = false;
            break;
        }
    }
    if carry {
        digits.insert(0, b'1');
    }
    format!("Virtuele proefsleuf {}", String::from_utf8(digits).unwrap())
}

fn point(p: [f64; 2]) -> Result<(), String> {
    if p.iter().any(|v| !v.is_finite() || v.abs() > 1e12) {
        Err("Ongeldige RD-kaartcoördinaten".into())
    } else {
        Ok(())
    }
}
pub fn center(t: &VirtualTrench) -> [f64; 2] {
    [(t.start.x + t.end.x) * 0.5, (t.start.y + t.end.y) * 0.5]
}
/// Actual virtual image centre, also when a point lies outside the axis/outline.
pub fn raster_center(t: &VirtualTrench) -> [f64; 2] {
    let mut min = [t.start.x.min(t.end.x), t.start.y.min(t.end.y)];
    let mut max = [t.start.x.max(t.end.x), t.start.y.max(t.end.y)];
    let mut include = |x: f64, y: f64| {
        min[0] = min[0].min(x);
        min[1] = min[1].min(y);
        max[0] = max[0].max(x);
        max[1] = max[1].max(y);
    };
    if t.boundary_3d.len() >= 3 {
        for p in &t.boundary_3d {
            include(p.x, p.y);
        }
    } else {
        for (x, y) in t.polygon() {
            include(x, y);
        }
    }
    for p in &t.objects {
        include(p.position.x, p.position.y);
    }
    let pad = (t.width_meters * 3.).max(1.5);
    // Same padded-bounds arithmetic as the actual original image transform.
    [
        ((min[0] - pad) + (max[0] + pad)) * 0.5,
        ((min[1] - pad) + (max[1] + pad)) * 0.5,
    ]
}
pub fn reference(t: &VirtualTrench) -> Result<Option<[f64; 2]>, String> {
    let Some(raw) = t
        .metadata
        .get(raster_points::REFERENCE)
        .and_then(raster_points::point)
    else {
        return Ok(None);
    };
    crate::profile_reference::Reference {
        raw,
        chainage: 0.,
        description: String::new(),
    }
    .display(&t.metadata, raster_center(t))
    .map(Some)
}
pub fn set_reference(t: &mut VirtualTrench, displayed: [f64; 2]) -> Result<bool, String> {
    point(displayed)?;
    let bytes = raster_points::validate_metadata(&t.metadata)?;
    let angle = crate::python_values::number(t.metadata.get("rotation_degrees")).unwrap_or(0.);
    let raw = crate::profile_reference::Reference {
        raw: displayed,
        chainage: 0.,
        description: String::new(),
    }
    .display(&json!({"rotation_degrees":-angle}), raster_center(t))?;
    point(raw)?;
    let value = json!({"x":raw[0],"y":raw[1]});
    if t.metadata.get(raster_points::REFERENCE) == Some(&value) {
        return Ok(false);
    }
    if bytes + 128 > 2 * 1024 * 1024 {
        return Err("Geen ruimte voor referentiepunt in laagmetadata".into());
    }
    if t.metadata.is_null() {
        t.metadata = json!({});
    }
    t.metadata
        .as_object_mut()
        .unwrap()
        .insert(raster_points::REFERENCE.into(), value);
    Ok(true)
}
pub fn clear_reference(t: &mut VirtualTrench) -> bool {
    t.metadata
        .as_object_mut()
        .and_then(|m| m.remove(raster_points::REFERENCE))
        .is_some()
}
pub fn create(
    label: String,
    start: [f64; 2],
    end: [f64; 2],
    width: f64,
) -> Result<VirtualTrench, String> {
    point(start)?;
    point(end)?;
    if (end[0] - start[0]).hypot(end[1] - start[1]) < 0.5 {
        return Err("Kies een eindpunt op minstens 0,5 m van het beginpunt".into());
    }
    let t = VirtualTrench::new(
        label,
        Vertex {
            x: start[0],
            y: start[1],
            z: None,
        },
        Vertex {
            x: end[0],
            y: end[1],
            z: None,
        },
        width,
    )?;
    crate::virtual_trench_editor::check(&t)?;
    Ok(t)
}
/// Validate every result before mutating. Coordinates, boundaries and alignment
/// baselines move together; Z, reference metadata, source files and allocations
/// stay intact. The original app moved points but left a measured outline behind.
pub fn translate(t: &mut VirtualTrench, dx: f64, dy: f64) -> Result<bool, String> {
    crate::virtual_trench_editor::check(t)?;
    if !dx.is_finite() || !dy.is_finite() || dx.abs() > 1e6 || dy.abs() > 1e6 {
        return Err("Ongeldige verplaatsing".into());
    }
    if dx == 0. && dy == 0. {
        return Ok(false);
    }
    let shifted = |p: Vertex| Vertex {
        x: p.x + dx,
        y: p.y + dy,
        ..p
    };
    for p in std::iter::once(t.start)
        .chain(std::iter::once(t.end))
        .chain(t.objects.iter().map(|o| o.position))
        .chain(t.boundary_3d.iter().copied())
        .chain(t.alignment.iter().flat_map(|a| [a.auto_start, a.auto_end]))
    {
        let p = shifted(p);
        point([p.x, p.y])?;
    }
    let start = shifted(t.start);
    let end = shifted(t.end);
    if (end.x - start.x).hypot(end.y - start.y) <= 1e-9 {
        return Err("Verplaatsing zou de meetas laten samenvallen".into());
    }
    if let Some(a) = &t.alignment {
        let mut next = a.clone();
        next.auto_start = shifted(a.auto_start);
        next.auto_end = shifted(a.auto_end);
        next.validate()?;
    }
    t.start = start;
    t.end = end;
    for p in &mut t.objects {
        p.position = shifted(p.position);
    }
    for p in &mut t.boundary_3d {
        *p = shifted(*p);
    }
    if let Some(a) = &mut t.alignment {
        a.auto_start = shifted(a.auto_start);
        a.auto_end = shifted(a.auto_end);
    }
    Ok(true)
}
/// Original reversed-source nearest hit selection and inclusive width/tolerance.
pub fn pick(
    trenches: &[VirtualTrench],
    p: [f64; 2],
    mpp: f64,
    selection_tolerance: f64,
) -> Option<usize> {
    point(p).ok()?;
    if !mpp.is_finite() || mpp <= 0. || !selection_tolerance.is_finite() {
        return None;
    }
    let tolerance = (mpp * 14.).max(selection_tolerance).min(4.);
    let mut best = None;
    let mut distance = f64::INFINITY;
    for (i, t) in trenches.iter().enumerate().rev() {
        let (x, y, _, _) = t.project(Vertex {
            x: p[0],
            y: p[1],
            z: None,
        });
        let d = (x - p[0]).hypot(y - p[1]);
        if d <= t.width_meters * 0.5 + tolerance && d < distance {
            best = Some(i);
            distance = d;
        }
    }
    best
}
