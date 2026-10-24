//! Pillow-compatible ellipse spans, without an image-sized mask.
// Adapted from Pillow 11.3.0 src/libImaging/Draw.c quarter_*/ellipse_*.
// Copyright holders and MIT-CMU terms: third_party/Pillow-LICENSE.txt.
use crate::map_canvas::{pixel, Color};
use image::RgbaImage;
use std::sync::atomic::AtomicBool;

struct Quarter {
    a: i64,
    b: i64,
    x: i64,
    y: i64,
    finished: bool,
}
impl Quarter {
    fn new(a: i64, b: i64) -> Self {
        Self {
            a,
            b,
            x: a,
            y: b % 2,
            finished: a < 0 || b < 0,
        }
    }
    fn delta(&self, x: i64, y: i64) -> i128 {
        // Wider arithmetic also keeps large, validated bounds overflow-free.
        let a2 = i128::from(self.a).pow(2);
        let b2 = i128::from(self.b).pow(2);
        (a2 * i128::from(y).pow(2) + b2 * i128::from(x).pow(2) - a2 * b2).abs()
    }
    fn next(&mut self) -> Option<(i64, i64)> {
        if self.finished {
            return None;
        }
        let result = (self.x, self.y);
        if self.x == self.a % 2 && self.y == self.b {
            self.finished = true;
        } else {
            let (mut nx, mut ny) = (self.x, self.y + 2);
            let mut delta = self.delta(nx, ny);
            if nx > 1 {
                let candidate = self.delta(self.x - 2, self.y + 2);
                if delta > candidate {
                    nx = self.x - 2;
                    ny = self.y + 2;
                    delta = candidate;
                }
                if delta > self.delta(self.x - 2, self.y) {
                    nx = self.x - 2;
                    ny = self.y;
                }
            }
            (self.x, self.y) = (nx, ny);
        }
        Some(result)
    }
}

/// `width=None` fills; `Some(width)` draws the original inside outline.
pub(crate) fn draw(
    image: &mut RgbaImage,
    bounds: [f64; 4],
    color: Color,
    width: Option<u32>,
    cancel: &AtomicBool,
    budget: &mut u64,
) -> Result<(), String> {
    if bounds.iter().any(|v| !v.is_finite()) {
        return Err("Ellips ligt buiten het tekenbereik".into());
    }
    // Valid CAD/paired maps can contain points very far outside this page.
    // Cull before imposing traversal limits, using Pillow's integer truncation.
    if bounds[2].trunc() < 0.
        || bounds[3].trunc() < 0.
        || bounds[0].trunc() >= f64::from(image.width())
        || bounds[1].trunc() >= f64::from(image.height())
    {
        return Ok(());
    }
    if bounds.iter().any(|v| v.abs() > 1_000_000.) {
        return Err("Ellips ligt buiten het tekenbereik".into());
    }
    let [x0, y0, x1, y1] = bounds.map(|v| v as i64);
    let (a, b) = (x1 - x0, y1 - y0);
    if a < 0
        || b < 0
        || x1 < 0
        || y1 < 0
        || x0 >= i64::from(image.width())
        || y0 >= i64::from(image.height())
    {
        return Ok(());
    }
    let width = width.map(i64::from).unwrap_or(a + b);
    if width < 1 {
        return Ok(());
    }
    // Reserve traversal work before entering either quarter loop. Visible
    // spans are charged separately; clipping never creates an enormous mask.
    *budget = budget
        .checked_sub((a + b + 2) as u64)
        .ok_or("Ellipsen overschrijden het tekenbudget")?;
    let mut outer = Quarter::new(a, b);
    let mut inner = Quarter::new(a - 2 * (width - 1), b - 2 * (width - 1));
    let (mut right, mut y) = outer.next().unwrap();
    let leftmost = a % 2;
    let mut previous_left = leftmost;
    loop {
        crate::input::check(cancel)?;
        let (row, mut left, right_here) = (y, previous_left, right);
        let mut next_outer = outer.next();
        while next_outer.is_some_and(|(_, cy)| cy <= row) {
            next_outer = outer.next();
        }
        let mut next_inner = inner.next();
        while let Some((cx, cy)) = next_inner {
            if cy > row {
                break;
            }
            left = cx;
            next_inner = inner.next();
        }
        previous_left = next_inner.map_or(leftmost, |(cx, _)| cx);
        let positive_left = if left == 0 { 2 } else { left };
        // Original four spans; overlapping center pixels intentionally receive
        // the same copied RGBA ink, rather than alpha-compositing repeatedly.
        let spans = [
            (
                positive_left,
                row,
                right_here,
                (left > 0 || left < right_here) && row > 0,
            ),
            (-right_here, row, -left, row > 0),
            (
                positive_left,
                -row,
                right_here,
                left > 0 || left < right_here,
            ),
            (-right_here, -row, -left, true),
        ];
        for (l, cy, r, enabled) in spans {
            if !enabled {
                continue;
            }
            let py = y0 + (cy + b) / 2;
            if py < 0 || py >= i64::from(image.height()) {
                continue;
            }
            let start = (x0 + (l + a) / 2).max(0);
            let end = (x0 + (r + a) / 2).min(i64::from(image.width()) - 1);
            if end < start {
                continue;
            }
            *budget = budget
                .checked_sub((end - start + 1) as u64)
                .ok_or("Ellipsen overschrijden het tekenbudget")?;
            for px in start..=end {
                pixel(image, px as i32, py as i32, color, false);
            }
        }
        let Some((nx, ny)) = next_outer else {
            break;
        };
        (right, y) = (nx, ny);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_pillow_fill_and_outline_pixels() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/virtual-map/ellipses.json"))
                .unwrap();
        let cancel = AtomicBool::new(false);
        for case in fixture["cases"].as_array().unwrap() {
            let mut image = RgbaImage::from_pixel(64, 48, image::Rgba([9, 17, 33, 255]));
            let bounds = std::array::from_fn(|i| case["bounds"][i].as_f64().unwrap());
            let mut budget = 1_000_000;
            if case["fill"].as_bool().unwrap() {
                draw(
                    &mut image,
                    bounds,
                    [231, 211, 191, 240],
                    None,
                    &cancel,
                    &mut budget,
                )
                .unwrap();
            }
            if let Some(width) = case["width"].as_u64() {
                draw(
                    &mut image,
                    bounds,
                    [0, 0, 0, 255],
                    Some(width as u32),
                    &cancel,
                    &mut budget,
                )
                .unwrap();
            }
            let actual: Vec<u8> = image.into_raw();
            let expected: Vec<u8> = case["pixels"]
                .as_str()
                .unwrap()
                .bytes()
                .flat_map(|v| match v {
                    b'.' => [9, 17, 33, 255],
                    b'f' => [231, 211, 191, 240],
                    b'o' => [0, 0, 0, 255],
                    _ => panic!("invalid pixel"),
                })
                .collect();
            assert_eq!(actual, expected, "ellipse {}", case["bounds"]);
        }
    }
    #[test]
    fn bounded_cancelled_and_invalid_ellipses() {
        let mut image = RgbaImage::new(8, 8);
        let mut budget = 100;
        let cancel = AtomicBool::new(false);
        for coordinate in [-1e12, 1e12] {
            draw(
                &mut image,
                [coordinate; 4],
                [0; 4],
                None,
                &cancel,
                &mut budget,
            )
            .unwrap();
        }
        assert_eq!(budget, 100);
        assert!(draw(
            &mut image,
            [0., 0., 1e20, 1.],
            [0; 4],
            None,
            &cancel,
            &mut budget
        )
        .is_err());
        assert!(draw(
            &mut image,
            [0., 0., f64::NAN, 1.],
            [0; 4],
            None,
            &cancel,
            &mut budget
        )
        .is_err());
        assert!(draw(
            &mut image,
            [0., 0., 999_999., 999_999.],
            [0; 4],
            None,
            &cancel,
            &mut budget
        )
        .is_err());
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(draw(
            &mut image,
            [0., 0., 4., 4.],
            [0; 4],
            None,
            &cancel,
            &mut budget
        )
        .is_err());
    }
}
