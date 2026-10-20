//! Expanded RGBA bicubic rotation compatible with Pillow 11.3.0.
//! Affine/interpolation and alpha conversion adapted from Pillow Image.py,
//! libImaging/Geometry.c and Convert.c; see third_party/Pillow-LICENSE.txt.
use crate::input;
use image::{Rgba, RgbaImage};
use std::sync::atomic::AtomicBool;
pub(crate) const MAX_PIXELS: u64 = 300_000_000;
pub(crate) fn allocate(w: u32, h: u32) -> Result<RgbaImage, String> {
    let size = u64::from(w) * u64::from(h);
    if size == 0 || size > MAX_PIXELS {
        return Err("Afbeeldingrotatie overschrijdt het pixelbudget".into());
    }
    let bytes = usize::try_from(size * 4).map_err(|_| "Afbeelding is te groot")?;
    let mut data = Vec::new();
    data.try_reserve_exact(bytes)
        .map_err(|_| "Onvoldoende geheugen voor afbeeldingrotatie")?;
    data.resize(bytes, 0);
    RgbaImage::from_raw(w, h, data).ok_or_else(|| "Ongeldige rasterafmetingen".into())
}
fn cubic(v: [f64; 4], d: f64) -> f64 {
    let p1 = v[1];
    let p2 = -v[0] + v[2];
    let p3 = 2. * (v[0] - v[1]) + v[2] - v[3];
    let p4 = -v[0] + v[1] - v[2] + v[3];
    p1 + d * (p2 + d * (p3 + d * p4))
}
fn rounded(v: f64) -> f64 {
    format!("{v:.15}").parse().unwrap()
}
fn mapped(m: [f64; 6], x: f64, y: f64) -> [f64; 2] {
    [m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5]]
}
fn sample(image: &RgbaImage, x: f64, y: f64) -> [u8; 4] {
    let w = image.width() as i64;
    let h = image.height() as i64;
    if x < 0. || x >= w as f64 || y < 0. || y >= h as f64 {
        return [255, 255, 255, 0];
    }
    let x = x - 0.5;
    let y = y - 0.5;
    let floorx = x.floor();
    let floory = y.floor();
    let dx = x - floorx;
    let dy = y - floory;
    let ix = floorx as i64 - 1;
    let iy = floory as i64 - 1;
    let xs = std::array::from_fn::<_, 4, _>(|i| (ix + i as i64).clamp(0, w - 1) as u32);
    let mut rows = [[0.; 4]; 4];
    for row in 0..4 {
        let sy = iy + row as i64;
        if row > 0 && (sy < 0 || sy >= h) {
            rows[row] = rows[row - 1];
            continue;
        }
        let sy = sy.clamp(0, h - 1) as u32;
        let pixels = xs.map(|x| image.get_pixel(x, sy).0);
        for c in 0..4 {
            rows[row][c] = cubic(pixels.map(|p| p[c] as f64), dx);
        }
    }
    let mut p =
        std::array::from_fn::<_, 4, _>(|c| cubic(rows.map(|r| r[c]), dy).clamp(0., 255.) as u8);
    let a = u32::from(p[3]);
    if a != 0 && a != 255 {
        for c in &mut p[..3] {
            *c = (255 * u32::from(*c) / a).min(255) as u8;
        }
    }
    p
}
pub(crate) fn rotate(
    mut image: RgbaImage,
    degrees: f64,
    cancel: &AtomicBool,
) -> Result<RgbaImage, String> {
    input::check(cancel)?;
    if !degrees.is_finite() {
        return Err("Ongeldige rasterrotatie".into());
    }
    let angle = degrees.rem_euclid(360.);
    let (w, h) = image.dimensions();
    if angle == 0. {
        return Ok(image);
    }
    if angle == 180. {
        let pixels = w as usize * h as usize;
        let data = image.as_mut();
        for i in 0..pixels / 2 {
            if i % 16384 == 0 {
                input::check(cancel)?;
            }
            for c in 0..4 {
                data.swap(i * 4 + c, (pixels - 1 - i) * 4 + c);
            }
        }
        return Ok(image);
    }
    if angle == 90. || angle == 270. {
        let mut output = allocate(h, w)?;
        for y in 0..h {
            input::check(cancel)?;
            for x in 0..w {
                let (dx, dy) = if angle == 90. {
                    (y, w - 1 - x)
                } else {
                    (h - 1 - y, x)
                };
                output.put_pixel(dx, dy, *image.get_pixel(x, y));
            }
        }
        return Ok(output);
    }
    let a = -angle.to_radians();
    let cosine = rounded(a.cos());
    let sine = rounded(a.sin());
    let mut matrix = [cosine, sine, 0., -sine, cosine, 0.];
    let offset = mapped(matrix, -(w as f64) / 2., -(h as f64) / 2.);
    matrix[2] = offset[0] + w as f64 / 2.;
    matrix[5] = offset[1] + h as f64 / 2.;
    let corners = [
        [0., 0.],
        [w as f64, 0.],
        [w as f64, h as f64],
        [0., h as f64],
    ]
    .map(|[x, y]| mapped(matrix, x, y));
    let minx = corners
        .iter()
        .map(|p| p[0])
        .fold(f64::INFINITY, f64::min)
        .floor();
    let miny = corners
        .iter()
        .map(|p| p[1])
        .fold(f64::INFINITY, f64::min)
        .floor();
    let maxx = corners
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil();
    let maxy = corners
        .iter()
        .map(|p| p[1])
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil();
    if maxx - minx > u32::MAX as f64 || maxy - miny > u32::MAX as f64 {
        return Err("Rotatieafmetingen buiten bereik".into());
    }
    let (nw, nh) = ((maxx - minx) as u32, (maxy - miny) as u32);
    let mut output = allocate(nw, nh)?;
    let offset = mapped(
        matrix,
        -(nw as f64 - w as f64) / 2.,
        -(nh as f64 - h as f64) / 2.,
    );
    matrix[2] = offset[0];
    matrix[5] = offset[1];
    // Consume the source buffer in place rather than allocate a second full
    // premultiplied copy. The destination is unpremultiplied while sampling.
    for (i, p) in image.as_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
        if i % 16384 == 0 {
            input::check(cancel)?;
        }
        let a = u32::from(p[3]);
        for c in &mut p[..3] {
            let tmp = u32::from(*c) * a + 128;
            *c = ((tmp + (tmp >> 8)) >> 8) as u8;
        }
    }
    for y in 0..nh {
        input::check(cancel)?;
        for x in 0..nw {
            let p = mapped(matrix, x as f64 + 0.5, y as f64 + 0.5);
            output.put_pixel(x, y, Rgba(sample(&image, p[0], p[1])));
        }
    }
    Ok(output)
}
