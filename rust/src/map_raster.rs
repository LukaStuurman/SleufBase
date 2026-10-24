//! Full-resolution TIFF painting for export maps. Bilinear resizing, affine
//! sampling and alpha arithmetic follow Pillow 11.3 libImaging (Resample.c,
//! Geometry.c, Convert.c, AlphaComposite.c); see third_party/Pillow-LICENSE.txt.
use crate::{geo::GeoTransform, input, map_canvas, map_page::Layout};
use image::{Rgba, RgbaImage};
use std::{collections::BTreeMap, sync::atomic::AtomicBool};

fn premultiply(mut p: [u8; 4]) -> [u8; 4] {
    for c in 0..3 {
        let t = u32::from(p[c]) * u32::from(p[3]) + 128;
        p[c] = (((t >> 8) + t) >> 8) as u8;
    }
    p
}
fn straight(mut p: [u8; 4]) -> [u8; 4] {
    if p[3] != 0 && p[3] != 255 {
        for c in 0..3 {
            p[c] = (255 * u32::from(p[c]) / u32::from(p[3])).min(255) as u8;
        }
    }
    p
}
struct Kernel {
    start: u32,
    weights: Vec<i32>,
}
fn kernels(source: u32, output: u32) -> Result<Vec<Kernel>, String> {
    let scale = f64::from(source) / f64::from(output);
    let support = scale.max(1.);
    if (support.ceil() * 2. + 1.) * f64::from(output) > 8_000_000. {
        return Err("Kaartverkleining overschrijdt het filterbudget".into());
    }
    Ok((0..output)
        .map(|x| {
            if source == output {
                return Kernel {
                    start: x,
                    weights: vec![1 << 22],
                };
            }
            let center = (f64::from(x) + 0.5) * scale;
            let start = ((center - support + 0.5) as i64).max(0) as u32;
            let end = ((center + support + 0.5) as i64).min(i64::from(source)) as u32;
            let weights = (start..end)
                .map(|i| (1. - ((f64::from(i) - center + 0.5) / support).abs()).max(0.))
                .collect::<Vec<_>>();
            let sum: f64 = weights.iter().sum();
            Kernel {
                start,
                weights: weights
                    .into_iter()
                    .map(|v| (v / sum * f64::from(1 << 22) + 0.5) as i32)
                    .collect(),
            }
        })
        .collect())
}
/// Separable anti-aliased bilinear resize of a pixel crop. An 8 MiB row cache
/// replaces Pillow's complete horizontal intermediate. No full source copy.
pub fn resize(
    source: &RgbaImage,
    crop: [u32; 4],
    size: [u32; 2],
    cancel: &AtomicBool,
) -> Result<RgbaImage, String> {
    input::check(cancel)?;
    let [left, top, right, bottom] = crop;
    if right <= left
        || bottom <= top
        || right > source.width()
        || bottom > source.height()
        || size.contains(&0)
        || size.iter().any(|&n| n > 10_000)
        || u64::from(size[0]) * u64::from(size[1]) > 50_000_000
    {
        return Err("Ongeldige kaart-rasteruitsnede of pixelbudget".into());
    }
    let [sw, sh] = [right - left, bottom - top];
    let same = [sw, sh] == size;
    if same {
        let mut output = crate::raster_rotation::allocate(sw, sh)?;
        for y in 0..sh {
            input::check(cancel)?;
            let start = ((top + y) as usize * source.width() as usize + left as usize) * 4;
            let target = y as usize * sw as usize * 4;
            output.as_mut()[target..target + sw as usize * 4]
                .copy_from_slice(&source.as_raw()[start..start + sw as usize * 4]);
        }
        return Ok(output);
    }
    let horizontal = kernels(sw, size[0])?;
    let vertical = kernels(sh, size[1])?;
    let mut output = crate::raster_rotation::allocate(size[0], size[1])?;
    let mut cache: BTreeMap<u32, Vec<[u8; 4]>> = BTreeMap::new();
    let max_rows = (8 * 1024 * 1024 / (size[0] as usize * 4)).max(1);
    for (y, k) in vertical.iter().enumerate() {
        input::check(cancel)?;
        cache.retain(|&row, _| row >= k.start);
        let mut sums = vec![[1i32 << 21; 4]; size[0] as usize];
        for (dy, &weight) in k.weights.iter().enumerate() {
            input::check(cancel)?;
            let row = k.start + dy as u32;
            if !cache.contains_key(&row) {
                if cache.len() >= max_rows {
                    cache.pop_first();
                }
                let mut pixels = Vec::with_capacity(size[0] as usize);
                for hk in &horizontal {
                    let mut sum = [1i32 << 21; 4];
                    for (dx, &w) in hk.weights.iter().enumerate() {
                        let mut p = source.get_pixel(left + hk.start + dx as u32, top + row).0;
                        if !same {
                            p = premultiply(p);
                        }
                        for c in 0..4 {
                            sum[c] += i32::from(p[c]) * w;
                        }
                    }
                    pixels.push(sum.map(|n| (n >> 22).clamp(0, 255) as u8));
                }
                cache.insert(row, pixels);
            }
            for (sum, p) in sums.iter_mut().zip(&cache[&row]) {
                for c in 0..4 {
                    sum[c] += i32::from(p[c]) * weight;
                }
            }
        }
        for (x, sum) in sums.into_iter().enumerate() {
            let mut p = sum.map(|n| (n >> 22).clamp(0, 255) as u8);
            if !same {
                p = straight(p);
            }
            output.put_pixel(x as u32, y as u32, Rgba(p));
        }
    }
    Ok(output)
}
fn bilinear(source: &RgbaImage, x: f64, y: f64) -> [u8; 4] {
    if x < 0. || y < 0. || x >= f64::from(source.width()) || y >= f64::from(source.height()) {
        return [0; 4];
    }
    let [x, y] = [x - 0.5, y - 0.5];
    let [ix, iy] = [x.floor() as i64, y.floor() as i64];
    let [dx, dy] = [x - x.floor(), y - y.floor()];
    let pixel = |x: i64, y: i64| {
        premultiply(
            source
                .get_pixel(
                    x.clamp(0, i64::from(source.width()) - 1) as u32,
                    y.clamp(0, i64::from(source.height()) - 1) as u32,
                )
                .0,
        )
    };
    let [a, b, c, d] = [
        pixel(ix, iy),
        pixel(ix + 1, iy),
        pixel(ix, iy + 1),
        pixel(ix + 1, iy + 1),
    ];
    straight(std::array::from_fn(|i| {
        let top = f64::from(a[i]) + (f64::from(b[i]) - f64::from(a[i])) * dx;
        let bottom = f64::from(c[i]) + (f64::from(d[i]) - f64::from(c[i])) * dx;
        (top + (bottom - top) * dy) as u8
    }))
}
pub fn composite(
    destination: &mut RgbaImage,
    source: &RgbaImage,
    origin: [i32; 2],
    cancel: &AtomicBool,
) -> Result<(), String> {
    for y in 0..source.height() {
        input::check(cancel)?;
        for x in 0..source.width() {
            map_canvas::pixel(
                destination,
                origin[0] + x as i32,
                origin[1] + y as i32,
                source.get_pixel(x, y).0,
                true,
            );
        }
    }
    Ok(())
}
/// White-mask and force-opacity contract of DXF template MapExporter.
pub fn paint(
    destination: &mut RgbaImage,
    source: RgbaImage,
    transform: GeoTransform,
    layout: &Layout,
    cancel: &AtomicBool,
) -> Result<(), String> {
    paint_with_opacity(destination,source,transform,layout,1.,cancel)
}
pub fn paint_with_opacity(
    destination: &mut RgbaImage,
    source: RgbaImage,
    transform: GeoTransform,
    layout: &Layout,
    opacity:f64,
    cancel: &AtomicBool,
) -> Result<(), String> {
    paint_inner(destination,source,transform,layout,opacity,true,cancel)
}
/// Original interactive MapRenderer semantics used for Gemma's input scene:
/// white TIFF pixels retain their actual alpha instead of becoming a DXF mask.
pub fn paint_scene(destination:&mut RgbaImage,source:RgbaImage,transform:GeoTransform,layout:&Layout,opacity:f64,cancel:&AtomicBool)->Result<(),String>{
    paint_inner(destination,source,transform,layout,opacity,false,cancel)
}
fn paint_inner(
    destination:&mut RgbaImage,mut source:RgbaImage,transform:GeoTransform,
    layout:&Layout,opacity:f64,remove_white:bool,cancel:&AtomicBool,
)->Result<(),String>{
    input::check(cancel)?;
    if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
        return Err("Ongeldige TIFF-dekking".into());
    }
    if destination.dimensions() != (layout.map_size[0], layout.map_size[1])
        || layout.map_size.contains(&0)
        || [
            layout.bounds.min_x,
            layout.bounds.min_y,
            layout.bounds.max_x,
            layout.bounds.max_y,
        ]
        .iter()
        .any(|n| !n.is_finite())
        || layout.bounds.min_x >= layout.bounds.max_x
        || layout.bounds.min_y >= layout.bounds.max_y
    {
        return Err("Kaartcanvas past niet bij de uitsnede".into());
    }
    transform.world_to_pixel(layout.bounds.min_x, layout.bounds.min_y)?;
    // Preserve original non-FMA corner arithmetic for floor/ceil crop decisions.
    transform.bounds(source.width(), source.height())?;
    let corners = [
        [0., 0.],
        [f64::from(source.width()), 0.],
        [f64::from(source.width()), f64::from(source.height())],
        [0., f64::from(source.height())],
    ]
    .map(|[x, y]| {
        [
            transform.a * x + transform.b * y + transform.c,
            transform.d * x + transform.e * y + transform.f,
        ]
    });
    let bounds = crate::Bounds {
        min_x: corners.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min),
        min_y: corners.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min),
        max_x: corners
            .iter()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max),
        max_y: corners
            .iter()
            .map(|p| p[1])
            .fold(f64::NEG_INFINITY, f64::max),
    };
    if [
        transform.a,
        transform.b,
        transform.c,
        transform.d,
        transform.e,
        transform.f,
    ]
    .iter()
    .any(|v| !v.is_finite())
    {
        return Err("Ongeldige kaart-TIFF-transformatie".into());
    }
    let view = layout.bounds;
    let visible = crate::Bounds {
        min_x: bounds.min_x.max(view.min_x),
        min_y: bounds.min_y.max(view.min_y),
        max_x: bounds.max_x.min(view.max_x),
        max_y: bounds.max_y.min(view.max_y),
    };
    if visible.min_x >= visible.max_x || visible.min_y >= visible.max_y {
        return Ok(());
    }
    for (i, p) in source.pixels_mut().enumerate() {
        if i % 16384 == 0 {
            input::check(cancel)?;
        }
        if remove_white && p[0].min(p[1]).min(p[2]) >= 245 {
            p[3] = 0;
        } else if opacity<1. {
            p[3]=(f64::from(p[3])*opacity) as u8;
        }
    }
    if transform.b.abs() <= 1e-9 && transform.d.abs() <= 1e-9 {
        let (x0, y0) = transform.world_to_pixel(visible.min_x, visible.max_y)?;
        let (x1, y1) = transform.world_to_pixel(visible.max_x, visible.min_y)?;
        let crop = [
            x0.min(x1).floor().max(0.) as u32,
            y0.min(y1).floor().max(0.) as u32,
            x0.max(x1).ceil().min(f64::from(source.width())) as u32,
            y0.max(y1).ceil().min(f64::from(source.height())) as u32,
        ];
        let a = layout.screen([visible.min_x, visible.max_y]);
        let b = layout.screen([visible.max_x, visible.min_y]);
        let origin = [a[0].min(b[0]).floor() as i32, a[1].min(b[1]).floor() as i32];
        let size = [
            a[0].max(b[0]).ceil() as i32 - origin[0],
            a[1].max(b[1]).ceil() as i32 - origin[1],
        ];
        if size.iter().any(|&v| v <= 0) {
            return Ok(());
        }
        let resized = resize(&source, crop, size.map(|v| v as u32), cancel)?;
        composite(destination, &resized, origin, cancel)
    } else {
        // Invert the combined pixel->screen matrix in local coordinates, avoiding
        // cancellation of RD origins during the affine inverse.
        let sx = f64::from(layout.map_size[0]) / (view.max_x - view.min_x);
        let sy = -f64::from(layout.map_size[1]) / (view.max_y - view.min_y);
        let [a, b, c, d, e, f] = [
            sx * transform.a,
            sx * transform.b,
            sx * (transform.c - view.min_x),
            sy * transform.d,
            sy * transform.e,
            sy * (transform.f - view.max_y),
        ];
        let determinant = a * e - b * d;
        let m = [
            e / determinant,
            -b / determinant,
            (b * f - e * c) / determinant,
            -d / determinant,
            a / determinant,
            (d * c - a * f) / determinant,
        ];
        if m.iter().any(|v| !v.is_finite()) {
            return Err("Singuliere kaarttransformatie".into());
        }
        for y in 0..destination.height() {
            input::check(cancel)?;
            for x in 0..destination.width() {
                let [x, yf] = [f64::from(x) + 0.5, f64::from(y) + 0.5];
                let p = bilinear(
                    &source,
                    m[0] * x + m[1] * yf + m[2],
                    m[3] * x + m[4] * yf + m[5],
                );
                map_canvas::blend(&mut destination.get_pixel_mut((x - 0.5) as u32, y).0, p);
            }
        }
        Ok(())
    }
}
