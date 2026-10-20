//! Full-resolution template TIFF orientation, cropping and background alpha.
use crate::{geo::GeoTransform, input, profile::XY, raster_rotation};
use image::RgbaImage;
use std::{path::Path, sync::atomic::AtomicBool};
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub path: std::path::PathBuf,
    pub transform: GeoTransform,
    #[serde(default)]
    pub reverse_orientation: bool,
    /// Optional measured start/end. An already reversed axis is not reversed twice.
    pub axis: Option<[XY; 2]>,
}
pub struct Options<'a> {
    pub transform: GeoTransform,
    pub roads: &'a [Vec<XY>],
    pub terrain: &'a [Vec<XY>],
    pub axis: Option<[XY; 2]>,
    pub reverse: bool,
}
#[derive(Debug, serde::Serialize)]
pub struct Details {
    pub source_size: [u32; 2],
    pub vector: Option<XY>,
    pub rotation: f64,
    pub rotated_size: [u32; 2],
    pub crop: [u32; 4],
}
pub struct Prepared {
    pub image: RgbaImage,
    pub details: Details,
}
fn nearest(point: XY, paths: &[Vec<XY>], cancel: &AtomicBool) -> Result<Option<(XY, f64)>, String> {
    let mut best = None;
    let mut distance = f64::INFINITY;
    for path in paths {
        input::check(cancel)?;
        for (i, pair) in path.windows(2).enumerate() {
            if i % 1024 == 0 {
                input::check(cancel)?;
            }
            let [a, b] = [pair[0], pair[1]];
            if a.into_iter().chain(b).any(|v| !v.is_finite()) {
                return Err("Ongeldig oriëntatiepad".into());
            }
            let dx = b[0] - a[0];
            let dy = b[1] - a[1];
            let squared = dx * dx + dy * dy;
            if squared <= 1e-12 {
                continue;
            }
            let factor = ((point[0] - a[0]) * dx + (point[1] - a[1]) * dy) / squared;
            if !(0. ..=1.).contains(&factor) {
                continue;
            }
            let projection = [a[0] + factor * dx, a[1] + factor * dy];
            let ds = (projection[0] - point[0]).powi(2) + (projection[1] - point[1]).powi(2);
            if ds < distance {
                distance = ds;
                best = Some(projection);
            }
        }
    }
    Ok(best.map(|p| (p, (p[0] - point[0]).hypot(p[1] - point[1]))))
}
pub fn orientation(
    size: [u32; 2],
    options: &Options<'_>,
    cancel: &AtomicBool,
) -> Result<Option<XY>, String> {
    input::check(cancel)?;
    let t = options.transform;
    if [t.a, t.b, t.c, t.d, t.e, t.f]
        .into_iter()
        .any(|v| !v.is_finite())
        || size.contains(&0)
    {
        return Err("Ongeldige rastertransformatie of afmetingen".into());
    }
    // Validate invertibility even when no usable orientation paths exist.
    t.world_to_pixel(t.c, t.f)?;
    if let Some([start, end]) = options.axis {
        if start.into_iter().chain(end).any(|v| !v.is_finite()) {
            return Err("Ongeldige profieloriëntatie".into());
        }
        let a = t.world_to_pixel(start[0], start[1])?;
        let b = t.world_to_pixel(end[0], end[1])?;
        let vector = [b.0 - a.0, b.1 - a.1];
        if vector[0] * vector[0] + vector[1] * vector[1] > 1e-6 {
            return Ok(Some(vector));
        }
    }
    let x = size[0] as f64 / 2.;
    let y = size[1] as f64 / 2.;
    // Preserve Python's operation order (the shared map transform uses FMA).
    let center = [t.a * x + t.b * y + t.c, t.d * x + t.e * y + t.f];
    let road = nearest(center, options.roads, cancel)?;
    let terrain = nearest(center, options.terrain, cancel)?;
    let chosen = match (road, terrain) {
        (Some(r), Some(t)) => Some(if t.1 * 0.8 < r.1 { t.0 } else { r.0 }),
        (Some(r), None) => Some(r.0),
        (None, Some(t)) => Some(t.0),
        _ => None,
    };
    chosen
        .map(|p| t.world_to_pixel(p[0], p[1]).map(|p| [x - p.0, y - p.1]))
        .transpose()
}
fn candidate(image: &RgbaImage, index: usize) -> bool {
    matches!(image.as_raw()[index * 4 + 3], 1 | 2)
}
fn mark_run(
    image: &mut RgbaImage,
    index: usize,
    stack: &mut Vec<(u32, u32)>,
) -> Result<u32, String> {
    let w = image.width() as usize;
    let row = index / w;
    let mut left = index;
    let mut right = index;
    while left > row * w && candidate(image, left - 1) {
        left -= 1;
    }
    while right + 1 < (row + 1) * w && candidate(image, right + 1) {
        right += 1;
    }
    for p in image.as_mut()[left * 4..(right + 1) * 4]
        .as_chunks_mut::<4>()
        .0
    {
        p[3] = 0;
    }
    stack
        .try_reserve(1)
        .map_err(|_| "Onvoldoende geheugen voor rasterachtergrond")?;
    stack.push((left as u32, right as u32));
    Ok(right as u32)
}
fn normalize(image: &mut RgbaImage, cancel: &AtomicBool) -> Result<(), String> {
    let (w, h) = image.dimensions();
    let w = w as usize;
    let h = h as usize;
    // Four states in the owned alpha channel replace several full-image masks:
    // 1 invisible, 2 visible near-white, 255 opaque, 0 edge-connected/visited.
    for (i, p) in image.as_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
        if i % 16384 == 0 {
            input::check(cancel)?;
        }
        let visible = p[3] > 15;
        let min = *p[..3].iter().min().unwrap();
        let max = *p[..3].iter().max().unwrap();
        p[3] = if !visible {
            1
        } else if min >= 240 && max - min <= 30 {
            2
        } else {
            255
        };
    }
    let mut stack = Vec::new();
    for x in 0..w {
        for i in [x, (h - 1) * w + x] {
            if candidate(image, i) {
                mark_run(image, i, &mut stack)?;
            }
        }
    }
    for y in 1..h.saturating_sub(1) {
        for i in [y * w, y * w + w - 1] {
            if candidate(image, i) {
                mark_run(image, i, &mut stack)?;
            }
        }
    }
    while let Some((left, right)) = stack.pop() {
        input::check(cancel)?;
        let row = left as usize / w;
        for next in [row.checked_sub(1), row.checked_add(1).filter(|&y| y < h)]
            .into_iter()
            .flatten()
        {
            let mut x = left as usize % w;
            let end = right as usize % w;
            while x <= end {
                let i = next * w + x;
                if candidate(image, i) {
                    x = mark_run(image, i, &mut stack)? as usize % w + 1;
                } else {
                    x += 1;
                }
            }
        }
    }
    for (i, p) in image.as_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
        if i % 16384 == 0 {
            input::check(cancel)?;
        }
        if p[3] == 1 {
            p[3] = 0;
        } else if p[3] == 2 {
            p[3] = 255;
        }
    }
    Ok(())
}
pub fn prepare(
    image: RgbaImage,
    options: &Options<'_>,
    cancel: &AtomicBool,
) -> Result<Prepared, String> {
    input::check(cancel)?;
    let (w, h) = image.dimensions();
    let source_size = [w, h];
    if u64::from(w) * u64::from(h) > crate::geotiff::MAX_GEOTIFF_PIXELS {
        return Err("TIFF overschrijdt het bronpixelbudget".into());
    }
    let vector = orientation(source_size, options, cancel)?;
    let rotation = vector
        .filter(|v| v[0] * v[0] + v[1] * v[1] > 1e-6)
        .map_or(0., |v| {
            let bearing = ((-v[0]).atan2(-v[1]).to_degrees() + 360.).rem_euclid(360.);
            let target = if options.axis.is_some() {
                270.
            } else if options.reverse {
                90.
            } else {
                270.
            };
            (target - bearing + 180.).rem_euclid(360.) - 180.
        });
    let mut image = raster_rotation::rotate(image, rotation, cancel)?;
    let (w, h) = image.dimensions();
    let rotated_size = [w, h];
    let mut bounds = [w, h, 0, 0];
    let mut visible = false;
    for y in 0..h {
        input::check(cancel)?;
        for x in 0..w {
            if image.get_pixel(x, y)[3] > 15 {
                visible = true;
                bounds[0] = bounds[0].min(x);
                bounds[1] = bounds[1].min(y);
                bounds[2] = bounds[2].max(x + 1);
                bounds[3] = bounds[3].max(y + 1);
            }
        }
    }
    let crop = if visible {
        [
            bounds[0].saturating_sub(2),
            bounds[1].saturating_sub(2),
            (bounds[2] + 2).min(w),
            (bounds[3] + 2).min(h),
        ]
    } else {
        [0, 0, w, h]
    };
    if crop != [0, 0, w, h] {
        let mut cropped = raster_rotation::allocate(crop[2] - crop[0], crop[3] - crop[1])?;
        let width = cropped.width() as usize * 4;
        let src = image.as_raw();
        for (row, dst) in cropped.as_mut().chunks_exact_mut(width).enumerate() {
            input::check(cancel)?;
            let start = ((crop[1] as usize + row) * w as usize + crop[0] as usize) * 4;
            dst.copy_from_slice(&src[start..start + width]);
        }
        image = cropped;
    }
    normalize(&mut image, cancel)?;
    Ok(Prepared {
        image,
        details: Details {
            source_size,
            vector,
            rotation,
            rotated_size,
            crop,
        },
    })
}
pub fn from_tiff(
    source: &Source,
    roads: &[Vec<XY>],
    terrain: &[Vec<XY>],
    axis: Option<[XY; 2]>,
    cancel: &AtomicBool,
) -> Result<Prepared, String> {
    let raster = crate::raster::decode_full_with_cancel(&source.path, cancel)?;
    let image = RgbaImage::from_raw(raster.width, raster.height, raster.rgba)
        .ok_or("Ongeldige TIFF-pixelbuffer")?;
    prepare(
        image,
        &Options {
            transform: source.transform,
            roads,
            terrain,
            axis: axis.or(source.axis),
            reverse: source.reverse_orientation,
        },
        cancel,
    )
}
struct CheckedPng<'a, W> {
    out: W,
    cancel: &'a AtomicBool,
    bytes: u64,
}
impl<W: std::io::Write> std::io::Write for CheckedPng<'_, W> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        input::check(self.cancel).map_err(std::io::Error::other)?;
        if self.bytes.saturating_add(data.len() as u64) > 256 * 1024 * 1024 {
            return Err(std::io::Error::other(
                "Voorbereide PNG overschrijdt het bestandsbudget",
            ));
        }
        let count = self.out.write(data)?;
        self.bytes += count as u64;
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        input::check(self.cancel).map_err(std::io::Error::other)?;
        self.out.flush()
    }
}
/// Encode a new asset atomically. Errors/cancellation leave no partial PNG and
/// never replace an earlier file. Export assembly supplies an owned bundle path.
pub fn save(prepared: &Prepared, path: &Path, cancel: &AtomicBool) -> Result<(), String> {
    use image::{
        codecs::png::{CompressionType, FilterType, PngEncoder},
        ImageEncoder,
    };
    input::check(cancel)?;
    crate::storage::atomic_write_checked(
        path,
        false,
        |file| {
            let writer = CheckedPng {
                out: file,
                cancel,
                bytes: 0,
            };
            PngEncoder::new_with_quality(writer, CompressionType::Fast, FilterType::Adaptive)
                .write_image(
                    prepared.image.as_raw(),
                    prepared.image.width(),
                    prepared.image.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .map_err(|e| e.to_string())
        },
        || input::check(cancel),
    )
}
