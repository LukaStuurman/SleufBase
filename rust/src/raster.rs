//! Bounded strip/tile TIFF decoding and aspect-preserving map previews.
//! Source samples remain in the TIFF; only requested preview pixels become RGBA.
use crate::{
    raster_color::{Chunk, Colors},
    tiff_view::TiffView,
};
use std::{
    fs::File,
    io::{BufReader, Read, Seek},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use tiff::{
    decoder::{ifd::Value, Decoder, Limits},
    tags::Tag,
};

pub const MAX_PREVIEW_PIXELS: u64 = 16_000_000;
pub const MAX_CHUNK_BYTES: usize = 128 * 1024 * 1024;
#[derive(Clone, Copy, Debug)]
pub struct PreviewOptions {
    pub max_side: u32,
    pub max_pixels: u64,
    pub max_chunk_bytes: usize,
}
impl Default for PreviewOptions {
    fn default() -> Self {
        Self {
            max_side: 2048,
            max_pixels: 2048 * 2048,
            max_chunk_bytes: 32 * 1024 * 1024,
        }
    }
}
impl PreviewOptions {
    fn validate(self) -> Result<(), String> {
        if self.max_side == 0
            || self.max_side > 8192
            || self.max_pixels == 0
            || self.max_pixels > MAX_PREVIEW_PIXELS
            || self.max_chunk_bytes == 0
            || self.max_chunk_bytes > MAX_CHUNK_BYTES
        {
            return Err("Ongeldige rasterpreviewlimieten".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default)]
pub struct DecodeStats {
    pub source_pixels: u64,
    pub decoded_chunks: u32,
    pub skipped_chunks: u32,
    pub peak_chunk_bytes: usize,
}
#[derive(Debug)]
pub struct RasterPreview {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub stats: DecodeStats,
}

pub fn oriented_dimensions(
    width: u32,
    height: u32,
    orientation: u32,
) -> Result<(u32, u32), String> {
    if !(1..=8).contains(&orientation) {
        return Err("Ongeldige TIFF-oriëntatie".into());
    }
    Ok(if orientation >= 5 {
        (height, width)
    } else {
        (width, height)
    })
}
fn oriented_pixel(x: u32, y: u32, w: u32, h: u32, orientation: u32) -> (u32, u32) {
    match orientation {
        2 => (w - 1 - x, y),
        3 => (w - 1 - x, h - 1 - y),
        4 => (x, h - 1 - y),
        5 => (y, x),
        6 => (h - 1 - y, x),
        7 => (h - 1 - y, w - 1 - x),
        8 => (y, w - 1 - x),
        _ => (x, y),
    }
}
fn preview_dimensions(w: u32, h: u32, options: PreviewOptions) -> (u32, u32) {
    let scale = (f64::from(options.max_side) / f64::from(w))
        .min(f64::from(options.max_side) / f64::from(h))
        .min((options.max_pixels as f64 / (u64::from(w) * u64::from(h)) as f64).sqrt())
        .min(1.0);
    let mut width = (f64::from(w) * scale).floor().max(1.0) as u32;
    let mut height = (f64::from(h) * scale).floor().max(1.0) as u32;
    while u64::from(width) * u64::from(height) > options.max_pixels {
        if width >= height {
            width -= 1;
        } else {
            height -= 1;
        }
    }
    (width, height)
}
fn sample_positions(source: u32, target: u32) -> Vec<u32> {
    (0..target)
        .map(|p| (((2 * u64::from(p) + 1) * u64::from(source)) / (2 * u64::from(target))) as u32)
        .collect()
}
fn numbers(value: Value) -> Result<Vec<f64>, String> {
    let v = match value {
        Value::List(values) => {
            return values
                .into_iter()
                .map(numbers)
                .collect::<Result<Vec<_>, _>>()
                .map(|v| v.into_iter().flatten().collect())
        }
        Value::Rational(a, b) if b != 0 => f64::from(a) / f64::from(b),
        Value::SRational(a, b) if b != 0 => f64::from(a) / f64::from(b),
        Value::Double(v) => v,
        Value::Float(v) => f64::from(v),
        Value::Short(v) => f64::from(v),
        Value::Unsigned(v) => f64::from(v),
        _ => return Err("Ongeldige numerieke TIFF-kleurtag".into()),
    };
    if !v.is_finite() {
        return Err("Niet-eindige TIFF-kleurtag".into());
    }
    Ok(vec![v])
}
fn numeric_tag<R: Read + Seek>(
    decoder: &mut Decoder<R>,
    tag: Tag,
    default: &[f64],
) -> Result<Vec<f64>, String> {
    decoder
        .find_tag(tag)
        .map_err(|e| e.to_string())?
        .map(numbers)
        .unwrap_or_else(|| Ok(default.to_vec()))
}

pub fn decode_tiff_preview(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let preview = decode_preview(path, PreviewOptions::default())?;
    Ok((preview.width, preview.height, preview.rgba))
}
pub fn decode_preview(path: &Path, options: PreviewOptions) -> Result<RasterPreview, String> {
    decode_preview_with_cancel(path, options, &AtomicBool::new(false))
}
pub fn decode_preview_with_cancel(
    path: &Path,
    options: PreviewOptions,
    cancel: &AtomicBool,
) -> Result<RasterPreview, String> {
    options.validate()?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Rasterverwerking geannuleerd".into());
    }
    let file = File::open(path).map_err(|e| format!("Rasterbestand openen mislukt: {e}"))?;
    let reader = TiffView::new(BufReader::new(file)).map_err(|e| e.to_string())?;
    let mut limits = Limits::default();
    limits.decoding_buffer_size = options.max_chunk_bytes;
    limits.intermediate_buffer_size = options.max_chunk_bytes;
    let mut decoder = Decoder::new(reader)
        .map_err(|e| format!("Rasterbestand ongeldig: {e}"))?
        .with_limits(limits);
    let (source_w, source_h) = decoder.dimensions().map_err(|e| e.to_string())?;
    let pixels = u64::from(source_w) * u64::from(source_h);
    if pixels == 0 || pixels > crate::geotiff::MAX_GEOTIFF_PIXELS {
        return Err("TIFF-afmetingen zijn leeg of overschrijden de bronlimiet".into());
    }
    let orientation = decoder
        .find_tag_unsigned::<u32>(Tag::Orientation)
        .map_err(|e| e.to_string())?
        .unwrap_or(1);
    oriented_dimensions(source_w, source_h, orientation)?;
    let photometric = decoder
        .get_tag_u32(Tag::PhotometricInterpretation)
        .map_err(|e| e.to_string())?;
    if ![0, 1, 2, 3, 5, 6].contains(&photometric) {
        return Err(format!(
            "TIFF-kleurruimte {photometric} wordt nog niet weergegeven"
        ));
    }
    let source_channels = decoder
        .find_tag_unsigned::<u32>(Tag::SamplesPerPixel)
        .map_err(|e| e.to_string())?
        .unwrap_or(1);
    let planar = decoder
        .find_tag_unsigned::<u32>(Tag::PlanarConfiguration)
        .map_err(|e| e.to_string())?
        .unwrap_or(1)
        == 2;
    let extras = decoder
        .find_tag_unsigned_vec::<u16>(Tag::ExtraSamples)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let nodata = decoder
        .find_tag(Tag::Unknown(42113))
        .map_err(|e| e.to_string())?
        .and_then(|v| v.into_string().ok())
        .and_then(|s| s.trim_end_matches('\0').trim().parse::<f64>().ok());
    // Re-read only the IFD through an in-memory tag view, keeping the same file
    // handle. Palette indices and gray+alpha are otherwise unsupported by tiff.
    if photometric == 0 || photometric == 3 {
        decoder.inner().black_is_zero().map_err(|e| e.to_string())?;
        decoder.seek_to_image(0).map_err(|e| e.to_string())?;
    }
    let color = decoder.colortype().map_err(|e| e.to_string())?;
    let bits = color.bit_depth();
    if ![1, 2, 4, 8, 16, 32, 64].contains(&bits) {
        return Err(format!("TIFF bitdiepte {bits} wordt nog niet weergegeven"));
    }
    let channels = usize::from(color.num_samples());
    let base = match photometric {
        0 | 1 | 3 => 1,
        2 | 6 => 3,
        5 => 4,
        _ => unreachable!(),
    };
    if channels < base || channels > 1024 {
        return Err("Ongeldig TIFF-kanaalaantal".into());
    }
    let alpha = (channels > base && matches!(extras.first(), Some(1 | 2))).then_some(base);
    let palette = if photometric == 3 {
        if bits > 16 {
            return Err("TIFF-kleurenkaart heeft meer dan 16 indexbits".into());
        }
        let palette = decoder
            .get_tag_u16_vec(Tag::ColorMap)
            .map_err(|e| e.to_string())?;
        if palette.len() != 3 * (1usize << bits) {
            return Err("TIFF-kleurenkaart heeft ongeldige afmetingen".into());
        }
        palette
    } else {
        Vec::new()
    };
    let (coefficients, reference) = if photometric == 6 {
        let maximum = if bits == 64 {
            u64::MAX as f64
        } else {
            ((1u64 << bits) - 1) as f64
        };
        let middle = (1u64 << (bits - 1)) as f64;
        (
            numeric_tag(&mut decoder, Tag::Unknown(529), &[0.299, 0.587, 0.114])?,
            numeric_tag(
                &mut decoder,
                Tag::Unknown(532),
                &[0.0, maximum, middle, maximum, middle, maximum],
            )?,
        )
    } else {
        (
            vec![0.299, 0.587, 0.114],
            vec![0.0, 255.0, 128.0, 255.0, 128.0, 255.0],
        )
    };
    if coefficients.len() != 3
        || reference.len() != 6
        || coefficients[1] <= 0.0
        || reference.as_chunks::<2>().0.iter().any(|p| p[1] <= p[0])
    {
        return Err("Ongeldige TIFF YCbCr-coëfficiënten/referentie".into());
    }
    let colors = Colors {
        photometric,
        bits,
        channels,
        planar,
        alpha,
        associated: extras.first() == Some(&1),
        palette,
        nodata,
        ycbcr_coefficients: coefficients.try_into().unwrap(),
        ycbcr_reference: reference.try_into().unwrap(),
    };
    let (chunk_w, chunk_h) = decoder.chunk_dimensions();
    if chunk_w == 0 || chunk_h == 0 {
        return Err("TIFF heeft lege stroken/tegels".into());
    }
    let columns = source_w.div_ceil(chunk_w);
    let rows = source_h.div_ceil(chunk_h);
    let per_plane = columns.checked_mul(rows).ok_or("Te veel TIFF-tegels")?;
    let source_planes = if planar { source_channels } else { 1 };
    let tag = match decoder.get_chunk_type() {
        tiff::decoder::ChunkType::Strip => Tag::StripOffsets,
        tiff::decoder::ChunkType::Tile => Tag::TileOffsets,
    };
    // strip_count() in tiff 0.11 can overflow on valid RowsPerStrip=UINT32_MAX.
    let total_chunks = decoder
        .get_tag_u64_vec(tag)
        .map_err(|e| e.to_string())?
        .len();
    if per_plane.checked_mul(source_planes).map(|v| v as usize) != Some(total_chunks) {
        return Err("TIFF strook-/tegelaantal klopt niet".into());
    }
    let encoded_h = if tag == Tag::StripOffsets {
        chunk_h.min(source_h)
    } else {
        chunk_h
    };
    let encoded_bytes = (u128::from(chunk_w)
        * u128::from(encoded_h)
        * u128::from(if planar { 1 } else { source_channels })
        * u128::from(bits))
    .div_ceil(8);
    if encoded_bytes > options.max_chunk_bytes as u128 {
        return Err("TIFF-strook/tegel overschrijdt het decodergeheugenbudget".into());
    }
    let (preview_w, preview_h) = preview_dimensions(source_w, source_h, options);
    let (width, height) = oriented_dimensions(preview_w, preview_h, orientation)?;
    let mut rgba = vec![0; (u64::from(width) * u64::from(height) * 4) as usize];
    let xs = sample_positions(source_w, preview_w);
    let ys = sample_positions(source_h, preview_h);
    let mut stats = DecodeStats {
        source_pixels: pixels,
        ..Default::default()
    };
    for row in 0..rows {
        let top = row * chunk_h;
        let bottom = top.saturating_add(chunk_h).min(source_h);
        let ya = ys.partition_point(|y| *y < top);
        let yb = ys.partition_point(|y| *y < bottom);
        if ya == yb {
            continue;
        }
        for column in 0..columns {
            if cancel.load(Ordering::Relaxed) {
                return Err("Rasterverwerking geannuleerd".into());
            }
            let left = column * chunk_w;
            let right = left.saturating_add(chunk_w).min(source_w);
            let xa = xs.partition_point(|x| *x < left);
            let xb = xs.partition_point(|x| *x < right);
            if xa == xb {
                continue;
            }
            let mut chunks = Vec::with_capacity(colors.required_planes());
            let mut used = 0usize;
            for plane in 0..colors.required_planes() {
                let index = row * columns + column + plane as u32 * per_plane;
                let layout = decoder
                    .image_chunk_buffer_layout(index)
                    .map_err(|e| e.to_string())?;
                used = used
                    .checked_add(layout.len)
                    .ok_or("TIFF-tegel is te groot")?;
                if used > options.max_chunk_bytes {
                    return Err("TIFF-tegelvlakken overschrijden het geheugenbudget".into());
                }
                let data = decoder
                    .read_chunk(index)
                    .map_err(|e| format!("TIFF-strook/tegel: {e}"))?;
                chunks.push(Chunk {
                    data,
                    row_stride: layout.row_stride.ok_or("TIFF mist rij-indeling")?.get(),
                });
                stats.decoded_chunks += 1;
            }
            stats.peak_chunk_bytes = stats.peak_chunk_bytes.max(used);
            // The dominant aerial-imagery path needs no numeric sample dispatch,
            // floating-point NoData checks, or general color-space conversion.
            let rgb8 = if colors.photometric == 2
                && colors.bits == 8
                && !colors.planar
                && !colors.associated
                && colors.nodata.is_none()
            {
                if let tiff::decoder::DecodingResult::U8(bytes) = &chunks[0].data {
                    Some(bytes.as_slice())
                } else {
                    None
                }
            } else {
                None
            };
            for (y, sy) in ys.iter().enumerate().take(yb).skip(ya) {
                for (x, sx) in xs.iter().enumerate().take(xb).skip(xa) {
                    let pixel = if let Some(bytes) = rgb8 {
                        let offset = (sy - top) as usize * chunks[0].row_stride
                            + (sx - left) as usize * colors.channels;
                        let source = bytes
                            .get(offset..offset + colors.channels)
                            .ok_or("TIFF RGB-pixels zijn onvolledig")?;
                        [
                            source[0],
                            source[1],
                            source[2],
                            colors.alpha.map_or(255, |a| source[a]),
                        ]
                    } else {
                        colors.rgba(&chunks, (sx - left) as usize, (sy - top) as usize)?
                    };
                    let (dx, dy) =
                        oriented_pixel(x as u32, y as u32, preview_w, preview_h, orientation);
                    let dest = (dy as usize * width as usize + dx as usize) * 4;
                    rgba[dest..dest + 4].copy_from_slice(&pixel);
                }
            }
        }
    }
    stats.skipped_chunks = total_chunks as u32 - stats.decoded_chunks;
    Ok(RasterPreview {
        width,
        height,
        rgba,
        stats,
    })
}
