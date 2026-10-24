//! Original object-export direction, job names and collision-aware labels.
use crate::{
    model::{Dataset, Geometry, Vertex},
    Bounds,
};
mod unicode {
    include!("export_unicode.rs");
}

pub(crate) fn decimal(ch: char) -> Option<u32> {
    let n = ch as u32;
    let i = unicode::DECIMAL_ZEROS
        .partition_point(|zero| *zero <= n)
        .checked_sub(1)?;
    let d = n - unicode::DECIMAL_ZEROS[i];
    (d < 10).then_some(d)
}
pub(crate) fn word(ch: char) -> bool {
    let n = ch as u32;
    unicode::WORD_RANGES
        .get(unicode::WORD_RANGES.partition_point(|(_, end)| *end < n))
        .is_some_and(|(start, _)| *start <= n)
}
fn digits(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut value = String::new();
    let mut at = start;
    while let Some(d) = chars.get(at).and_then(|c| decimal(*c)) {
        value.push(char::from(b'0' + d as u8));
        at += 1;
    }
    if at == start {
        return None;
    }
    let trimmed = value.trim_start_matches('0');
    Some((
        if trimmed.is_empty() {
            "0".into()
        } else {
            trimmed.into()
        },
        at,
    ))
}
/// Python's PS pattern takes precedence over its first-number fallback, per
/// source (job title first, then the source filename). No fixed-width int parse.
pub fn job_label(dataset: &Dataset, fallback: usize) -> String {
    for source in [
        Some(dataset.title.as_str()),
        dataset.source_path.file_stem().and_then(|v| v.to_str()),
    ]
    .into_iter()
    .flatten()
    {
        let chars: Vec<_> = source.chars().collect();
        for i in 0..chars.len().saturating_sub(1) {
            if !matches!(chars[i], 'p' | 'P')
                || !matches!(chars[i + 1], 's' | 'S' | 'ſ')
                || (i > 0 && word(chars[i - 1]))
            {
                continue;
            }
            let mut at = i + 2;
            while chars.get(at).is_some_and(|ch| {
                ch.is_whitespace() || matches!(*ch, '\u{1c}'..='\u{1f}' | '.' | '_' | '-')
            }) {
                at += 1;
            }
            if let Some((value, end)) = digits(&chars, at) {
                if chars.get(end).is_none_or(|c| !word(*c)) {
                    return format!("PS{value}");
                }
            }
        }
        for i in 0..chars.len() {
            if let Some((value, _)) = digits(&chars, i) {
                return format!("PS{value}");
            }
        }
    }
    format!("PS{fallback}")
}
pub(crate) fn point_delta(dataset: &Dataset, half: f64) -> [f64; 2] {
    let points = || {
        dataset.features.iter().filter_map(|f| match f.geometry {
            Geometry::Point(v) => Some(v),
            _ => None,
        })
    };
    let Some(first) = points().next() else {
        return [0.; 2];
    };
    let angle = points()
        .rev()
        .find(|p| (p.x - first.x).abs() > 1e-6 || (p.y - first.y).abs() > 1e-6)
        .map_or(std::f64::consts::FRAC_PI_2, |last| {
            (last.y - first.y).atan2(last.x - first.x) + std::f64::consts::FRAC_PI_2
        });
    [angle.cos() * half, angle.sin() * half]
}
pub(crate) fn ends(point: Vertex, delta: [f64; 2]) -> [Vertex; 2] {
    [
        Vertex {
            x: point.x + delta[0],
            y: point.y + delta[1],
            z: point.z,
        },
        Vertex {
            x: point.x - delta[0],
            y: point.y - delta[1],
            z: point.z,
        },
    ]
}
fn intersects(a: Bounds, b: Bounds) -> bool {
    !(a.max_x < b.min_x || a.min_x > b.max_x || a.max_y < b.min_y || a.min_y > b.max_y)
}
pub(crate) fn label_bounds(label: &str, center: [f64; 2]) -> Bounds {
    let half = (label.chars().count() as f64 * 6. * 0.72).max(12.) / 2.;
    Bounds {
        min_x: center[0] - half,
        max_x: center[0] + half,
        min_y: center[1] - 5.,
        max_y: center[1] + 5.,
    }
}
pub(crate) fn label_center(
    index: usize,
    label: &str,
    jobs: &[Bounds],
    placed: &[Bounds],
    gap: f64,
) -> Result<[f64; 2], String> {
    let own = jobs[index];
    // Preserve the reference's rounding order; use a finite midpoint fallback
    // only when the sum itself overflows.
    let midpoint = |a: f64, b: f64| {
        let sum = a + b;
        if sum.is_finite() {
            sum / 2.
        } else {
            a / 2. + b / 2.
        }
    };
    let cx = midpoint(own.min_x, own.max_x);
    let cy = midpoint(own.min_y, own.max_y);
    let half_width = (label.chars().count() as f64 * 6. * 0.72).max(12.) / 2.;
    let mut best = None;
    for ring in 0..5 {
        let g = gap + ring as f64 * 4.5;
        let candidates = [
            [cx, own.max_y + g + 5.],
            [cx, own.min_y - g - 5.],
            [own.max_x + g + half_width, cy],
            [own.min_x - g - half_width, cy],
            [own.max_x + g + half_width, own.max_y + g + 5.],
            [own.min_x - g - half_width, own.max_y + g + 5.],
            [own.max_x + g + half_width, own.min_y - g - 5.],
            [own.min_x - g - half_width, own.min_y - g - 5.],
        ];
        for (priority, center) in candidates.into_iter().enumerate() {
            let bounds = label_bounds(label, center);
            if [bounds.min_x, bounds.max_x, bounds.min_y, bounds.max_y]
                .iter()
                .any(|v| !v.is_finite())
            {
                return Err("Joblabel valt buiten het geldige coördinatenbereik".into());
            }
            let overlaps_labels = placed
                .iter()
                .filter(|other| intersects(bounds, **other))
                .count();
            let overlaps_jobs = jobs
                .iter()
                .enumerate()
                .filter(|(i, other)| *i != index && intersects(bounds, **other))
                .count();
            let distance = (center[0] - center[0].clamp(own.min_x, own.max_x))
                .hypot(center[1] - center[1].clamp(own.min_y, own.max_y));
            let score = (overlaps_labels, overlaps_jobs, distance, priority);
            if best.as_ref().is_none_or(|(previous, _)| score < *previous) {
                best = Some((score, center));
            }
            if overlaps_labels == 0 && overlaps_jobs == 0 {
                return Ok(center);
            }
        }
    }
    Ok(best.expect("forty candidates").1)
}
