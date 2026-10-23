//! Resolution-aware reference legends; map pixels and world bounds stay intact.
use crate::map_page::{self, Layout, Op, XY};
use std::sync::atomic::AtomicBool;

/// Keep the original 300-DPI layout contract while making physical text and
/// symbol sizes consistent at other resolutions. The old fixed-pixel legend
/// covered most of a 96-DPI page and could overlap its north arrow.
pub fn plan(
    layout: &Layout,
    attribution: Option<&str>,
    reference: Option<XY>,
    measure: &mut impl FnMut(&str, f32, bool) -> Result<[f64; 4], String>,
    cancel: &AtomicBool,
) -> Result<Vec<Op>, String> {
    let scale = f64::from(layout.page_size[0]) / 3508.;
    if !(0.09..=2.01).contains(&scale) || layout.map_size.contains(&0) {
        return Err("Ongeldige kaartgrootte voor annotaties".into());
    }
    // Pages without a reference legend retain their existing layout contract.
    if scale == 1. || reference.is_none() {
        return map_page::plan(layout, attribution, reference, measure, cancel);
    }
    let mut base = layout.clone();
    base.page_size = layout
        .page_size
        .map(|v| (f64::from(v) / scale).round_ties_even() as u32);
    base.map_size = layout
        .map_size
        .map(|v| (f64::from(v) / scale).round_ties_even().max(1.) as u32);
    // Separate ratios remove rounding drift from RD positions and the frame.
    let sx = f64::from(layout.map_size[0]) / f64::from(base.map_size[0]);
    let sy = f64::from(layout.map_size[1]) / f64::from(base.map_size[1]);
    base.origin = [layout.origin[0] / sx, layout.origin[1] / sy];
    base.meters_per_pixel = layout.meters_per_pixel * sx;
    let mut ops = map_page::plan(
        &base,
        attribution,
        reference,
        &mut |text, size, bold| {
            // Measure the actual output font, rather than measuring a larger
            // font and assuming its hinted bounds scale exactly.
            let b = measure(text, size * scale as f32, bold)?;
            Ok([b[0] / sx, b[1] / sy, b[2] / sx, b[3] / sy])
        },
        cancel,
    )?;
    let point = |p: &mut XY| {
        p[0] *= sx;
        p[1] *= sy;
    };
    let width = |v: &mut u32| {
        *v = (f64::from(*v) * scale).round_ties_even().max(1.) as u32;
    };
    for op in &mut ops {
        crate::input::check(cancel)?;
        match op {
            Op::Line {
                points, width: w, ..
            } => {
                for p in points {
                    point(p);
                }
                width(w);
            }
            Op::Polygon { points, .. } => {
                for p in points {
                    point(p);
                }
            }
            Op::Rect {
                bounds,
                width: w,
                radius,
                ..
            } => {
                bounds[0] *= sx;
                bounds[2] *= sx;
                bounds[1] *= sy;
                bounds[3] *= sy;
                width(w);
                *radius *= scale;
            }
            Op::Ellipse {
                bounds, width: w, ..
            } => {
                bounds[0] *= sx;
                bounds[2] *= sx;
                bounds[1] *= sy;
                bounds[3] *= sy;
                width(w);
            }
            Op::Text {
                position,
                size,
                stroke,
                ..
            } => {
                point(position);
                *size *= scale as f32;
                if let Some((_, w)) = stroke {
                    width(w);
                }
            }
        }
    }
    Ok(ops)
}
