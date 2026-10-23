//! Bounded CPU map canvas. Geometry never allocates a full-size mask per item.
use image::{Rgba, RgbaImage};
use std::sync::atomic::AtomicBool;
pub(crate) type Color = [u8; 4];
pub(crate) fn blend(dst: &mut [u8; 4], src: Color) {
    let a = u32::from(src[3]);
    if a == 0 {
        return;
    }
    if a == 255 {
        *dst = src;
        return;
    }
    // Pillow 11.3 AlphaComposite.c: seven fractional bits, rounded /255.
    // See third_party/Pillow-LICENSE.txt. Keep integer operation order.
    let denominator = a * 255 + u32::from(dst[3]) * (255 - a);
    let coef = a * 255 * 255 * 128 / denominator;
    let div255 = |v: u32| ((v >> 8) + v) >> 8;
    for c in 0..3 {
        dst[c] =
            (div255(u32::from(src[c]) * coef + u32::from(dst[c]) * (255 * 128 - coef) + (128 << 7))
                >> 7) as u8;
    }
    dst[3] = div255(denominator + 128) as u8;
}
pub(crate) fn pixel(image: &mut RgbaImage, x: i32, y: i32, color: Color, composite: bool) {
    if x < 0 || y < 0 || x >= image.width() as i32 || y >= image.height() as i32 {
        return;
    }
    let p = image.get_pixel_mut(x as u32, y as u32);
    if composite {
        blend(&mut p.0, color);
    } else {
        *p = Rgba(color);
    }
}
pub(crate) fn rect(image: &mut RgbaImage, b: [f64; 4], fill: Color) {
    let [l, t, r, b] = b.map(|v| v as i32);
    for y in t.max(0)..=b.min(image.height() as i32 - 1) {
        for x in l.max(0)..=r.min(image.width() as i32 - 1) {
            pixel(image, x, y, fill, false);
        }
    }
}
pub(crate) use crate::map_ellipse::draw as ellipse;
pub(crate) fn line(
    image: &mut RgbaImage,
    points: &[[f64; 2]],
    width: u32,
    color: Color,
    cancel: &AtomicBool,
    budget: &mut u64,
) -> Result<(), String> {
    // Clip first: huge world coordinates must never produce a huge Bresenham loop.
    for pair in points.windows(2) {
        crate::input::check(cancel)?;
        let Some([a, b]) = clip(
            pair[0],
            pair[1],
            [
                -(width as f64),
                -(width as f64),
                image.width() as f64 + width as f64,
                image.height() as f64 + width as f64,
            ],
        ) else {
            continue;
        };
        let (mut x, mut y) = (a[0] as i32, a[1] as i32);
        let (ex, ey) = (b[0] as i32, b[1] as i32);
        let dx = (ex - x).abs();
        let sx = if x < ex { 1 } else { -1 };
        let dy = -(ey - y).abs();
        let sy = if y < ey { 1 } else { -1 };
        let mut err = dx + dy;
        let cost = (u64::from(dx.max(-dy) as u32) + 1) * u64::from(width) * u64::from(width);
        *budget = budget
            .checked_sub(cost)
            .ok_or("Kaartlijnen overschrijden het tekenbudget")?;
        // Pillow's even-width axis-aligned strokes lie on the positive side
        // when drawn forwards, negative side backwards. They have flat ends;
        // a square brush at every Bresenham point extends and shifts the line.
        if dx>0 && dy==0 || dx==0 && dy<0 {
            let forward=if dx>0{x<ex}else{y<ey};
            let low=if forward{-((width as i32-1)/2)}else{-(width as i32/2)};
            let high=low+width as i32-1;
            if dx>0 {
                for px in x.min(ex)..=x.max(ex){
                    if px%256==0{crate::input::check(cancel)?;}
                    for offset in low..=high{pixel(image,px,y+offset,color,false);}
                }
            }else{
                for py in y.min(ey)..=y.max(ey){
                    if py%256==0{crate::input::check(cancel)?;}
                    for offset in low..=high{pixel(image,x+offset,py,color,false);}
                }
            }
            continue;
        }
        let low = -(width as i32 / 2);
        let high = low + width as i32 - 1;
        let mut steps = 0u32;
        loop {
            if steps.is_multiple_of(256) {
                crate::input::check(cancel)?;
            }
            steps += 1;
            for oy in low..=high {
                for ox in low..=high {
                    pixel(image, x + ox, y + oy, color, false);
                }
            }
            if x == ex && y == ey {
                break;
            }
            let twice = 2 * err;
            if twice >= dy {
                err += dy;
                x += sx;
            }
            if twice <= dx {
                err += dx;
                y += sy;
            }
        }
    }
    Ok(())
}
pub(crate) fn polygon(image: &mut RgbaImage, points: &[[f64; 2]], color: Color) {
    if points.len() < 3 {
        return;
    }
    let miny = points
        .iter()
        .map(|p| p[1])
        .fold(f64::INFINITY, f64::min)
        .floor()
        .max(0.) as i32;
    let maxy = points
        .iter()
        .map(|p| p[1])
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .min(image.height() as f64 - 1.) as i32;
    let mut crossings = Vec::with_capacity(points.len());
    for y in miny..=maxy {
        crossings.clear();
        let scan = y as f64 + 0.5;
        for i in 0..points.len() {
            let a = points[i];
            let b = points[(i + 1) % points.len()];
            if (a[1] <= scan && b[1] > scan) || (b[1] <= scan && a[1] > scan) {
                crossings.push(a[0] + (scan - a[1]) * (b[0] - a[0]) / (b[1] - a[1]));
            }
        }
        crossings.sort_by(f64::total_cmp);
        for pair in crossings.as_chunks::<2>().0 {
            for x in pair[0].ceil().max(0.) as i32
                ..=pair[1].floor().min(image.width() as f64 - 1.) as i32
            {
                pixel(image, x, y, color, false);
            }
        }
    }
}
fn clip(a: [f64; 2], b: [f64; 2], bounds: [f64; 4]) -> Option<[[f64; 2]; 2]> {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let mut lo: f64 = 0.;
    let mut hi: f64 = 1.;
    for (p, q) in [
        (-dx, a[0] - bounds[0]),
        (dx, bounds[2] - a[0]),
        (-dy, a[1] - bounds[1]),
        (dy, bounds[3] - a[1]),
    ] {
        if p == 0. {
            if q < 0. {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0. {
                lo = lo.max(r);
            } else {
                hi = hi.min(r);
            }
            if lo > hi {
                return None;
            }
        }
    }
    Some([
        [a[0] + lo * dx, a[1] + lo * dy],
        [a[0] + hi * dx, a[1] + hi * dy],
    ])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_pillow_even_odd_and_reversed_axis_strokes_have_exact_pixels(){
        let cases:serde_json::Value=serde_json::from_str(include_str!("../tests/fixtures/map-batch/axis-strokes.json")).unwrap();
        for case in cases.as_array().unwrap(){
            let mut image=RgbaImage::new(20,20);
            let points:Vec<[f64;2]>=serde_json::from_value(case["points"].clone()).unwrap();
            line(&mut image,&points,case["width"].as_u64().unwrap() as u32,[19,70,130,190],&AtomicBool::new(false),&mut 100_000).unwrap();
            let actual=(0..20).flat_map(|y|(0..20).map(move |x|[x,y])).filter(|p|image.get_pixel(p[0],p[1])[3]>0).collect::<Vec<_>>();
            assert_eq!(serde_json::to_value(actual).unwrap(),case["pixels"],"{case}");
        }
    }
    #[test]
    fn huge_lines_are_clipped_and_work_or_cancellation_limits_precede_drawing() {
        let mut image = RgbaImage::new(64, 32);
        let mut budget = 1000;
        line(
            &mut image,
            &[[-1e12, 16.], [1e12, 16.]],
            1,
            [12, 34, 56, 255],
            &AtomicBool::new(false),
            &mut budget,
        )
        .unwrap();
        assert!((0..64).all(|x| image.get_pixel(x, 16).0 == [12, 34, 56, 255]));
        let previous = image.clone();
        assert!(line(
            &mut image,
            &[[0., 0.], [63., 31.]],
            10,
            [255; 4],
            &AtomicBool::new(false),
            &mut budget
        )
        .is_err());
        assert_eq!(image, previous);
        assert!(line(
            &mut image,
            &[[0., 0.], [63., 31.]],
            1,
            [255; 4],
            &AtomicBool::new(true),
            &mut budget
        )
        .is_err());
        assert_eq!(image, previous);
    }
}
