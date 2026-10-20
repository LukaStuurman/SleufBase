//! Native PDOK aerial WMS integration (EPSG:28992).
//! Uses the same default PDOK source and WMS 1.1.1 parameters as Python pdok.py.
//! The source URL is fixed; this module never forwards arbitrary endpoints.

use crate::model::Bounds;
use image::ImageDecoder;
use std::{
    io::Read,
    sync::{atomic::AtomicBool, OnceLock},
    time::Duration,
};

const PDOK_WMS_URL: &str = "https://service.pdok.nl/hwh/luchtfotorgb/wms/v1_0";
const MAX_RESPONSE_BYTES: usize = 24 * 1024 * 1024;
const MAX_OUTPUT_PIXELS: u64 = 2_048 * 2_048;

#[derive(Clone, Debug)]
pub struct PdokRaster {
    pub bounds: Bounds,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn validate(bounds: Bounds, width: u32, height: u32) -> Result<(), String> {
    if width == 0
        || height == 0
        || width > 2048
        || height > 2048
        || u64::from(width) * u64::from(height) > MAX_OUTPUT_PIXELS
    {
        return Err("PDOK WMS-tegel moet 1 tot 2048 pixels per zijde zijn".into());
    }
    if [bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y]
        .iter()
        .any(|x| !x.is_finite())
        || bounds.min_x >= bounds.max_x
        || bounds.min_y >= bounds.max_y
    {
        return Err("BBOX-coördinaten zijn ongeldig".into());
    }
    Ok(())
}

pub fn build_wms_url(bounds: Bounds, width: u32, height: u32) -> Result<reqwest::Url, String> {
    validate(bounds, width, height)?;
    let mut url = reqwest::Url::parse(PDOK_WMS_URL).map_err(|e| e.to_string())?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("SERVICE", "WMS");
        query.append_pair("VERSION", "1.1.1");
        query.append_pair("REQUEST", "GetMap");
        query.append_pair("LAYERS", "Actueel_orthoHR");
        query.append_pair("STYLES", "");
        query.append_pair("FORMAT", "image/png");
        query.append_pair("TRANSPARENT", "FALSE");
        query.append_pair("SRS", "EPSG:28992");
        query.append_pair("WIDTH", &width.to_string());
        query.append_pair("HEIGHT", &height.to_string());
        query.append_pair(
            "BBOX",
            &format!(
                "{},{},{},{}",
                bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y
            ),
        );
    }
    Ok(url)
}

pub fn request_map(bounds: Bounds, width: u32, height: u32) -> Result<PdokRaster, String> {
    request_map_with_cancel(bounds, width, height, &AtomicBool::new(false))
}
fn read_response(mut reader: impl Read, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        crate::input::check(cancel)?;
        let n = reader
            .read(&mut buffer)
            .map_err(|e| format!("PDOK afbeelding lezen mislukt: {e}"))?;
        if n == 0 {
            break;
        }
        if bytes.len() + n > MAX_RESPONSE_BYTES {
            return Err("PDOK-antwoord is onverwacht groot".into());
        }
        bytes.extend_from_slice(&buffer[..n]);
    }
    crate::input::check(cancel)?;
    Ok(bytes)
}
pub fn request_map_with_cancel(
    bounds: Bounds,
    width: u32,
    height: u32,
    cancel: &AtomicBool,
) -> Result<PdokRaster, String> {
    crate::input::check(cancel)?;
    let url = build_wms_url(bounds, width, height)?;
    static CLIENT: OnceLock<Result<reqwest::blocking::Client, String>> = OnceLock::new();
    let client = CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(30))
                .connect_timeout(Duration::from_secs(10))
                .user_agent("SleufBase-Rust/0.1")
                .build()
                .map_err(|e| format!("Netwerkclient mislukt: {e}"))
        })
        .as_ref()
        .map_err(Clone::clone)?;
    let response = client
        .get(url)
        .send()
        .map_err(|e| format!("PDOK onbereikbaar: {e}"))?
        .error_for_status()
        .map_err(|e| format!("PDOK HTTP-fout: {e}"))?;
    if response
        .content_length()
        .is_some_and(|len| len > MAX_RESPONSE_BYTES as u64)
    {
        return Err("PDOK-antwoord is onverwacht groot".into());
    }
    let bytes = read_response(response, cancel)?;
    let rgba = decode_response(&bytes, width, height, cancel)?;
    Ok(PdokRaster {
        bounds,
        width,
        height,
        rgba,
    })
}
fn decode_response(
    bytes: &[u8],
    width: u32,
    height: u32,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    crate::input::check(cancel)?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err("PDOK-antwoord is onverwacht groot".into());
    }
    // PDOK can return JPEG even to FORMAT=image/png. Match the Python content
    // decoder, while whitelisting formats and checking dimensions before pixels.
    let format = image::guess_format(bytes).map_err(|_| "PDOK leverde geen geldige PNG/JPEG")?;
    if !matches!(format, image::ImageFormat::Png | image::ImageFormat::Jpeg) {
        return Err("PDOK leverde geen ondersteunde PNG/JPEG".into());
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(width);
    limits.max_image_height = Some(height);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .map_err(|e| format!("PDOK afbeelding ongeldig: {e}"))?;
    if decoder.dimensions() != (width, height) {
        return Err("PDOK leverde onverwachte afbeeldingsafmetingen".into());
    }
    let image = image::DynamicImage::from_decoder(decoder)
        .map_err(|e| format!("PDOK afbeelding ongeldig: {e}"))?;
    crate::input::check(cancel)?;
    Ok(image.to_rgba8().into_raw())
}

/// Full export resolution, assembled on one shared pixel grid. Tiles preserve
/// pixel scale and meet WMS' 2048-pixel limit without rescaling or seams.
pub fn request_tiled(
    bounds: Bounds,
    width: u32,
    height: u32,
    cancel: &AtomicBool,
) -> Result<image::RgbaImage, String> {
    tiled(bounds, width, height, cancel, |b, w, h| {
        request_map_with_cancel(b, w, h, cancel)
    })
}
fn tiled(
    bounds: Bounds,
    width: u32,
    height: u32,
    cancel: &AtomicBool,
    mut fetch: impl FnMut(Bounds, u32, u32) -> Result<PdokRaster, String>,
) -> Result<image::RgbaImage, String> {
    crate::input::check(cancel)?;
    if width == 0
        || height == 0
        || width > 10_000
        || height > 10_000
        || u64::from(width) * u64::from(height) > 50_000_000
    {
        return Err("PDOK-exportkaart overschrijdt het pixelbudget".into());
    }
    validate(bounds, 1, 1)?;
    let mut output = crate::raster_rotation::allocate(width, height)?;
    let x = |n: u32| bounds.min_x + f64::from(n) / f64::from(width) * (bounds.max_x - bounds.min_x);
    let y =
        |n: u32| bounds.max_y - f64::from(n) / f64::from(height) * (bounds.max_y - bounds.min_y);
    for top in (0..height).step_by(2048) {
        for left in (0..width).step_by(2048) {
            crate::input::check(cancel)?;
            let w = (width - left).min(2048);
            let h = (height - top).min(2048);
            let tile = fetch(
                Bounds {
                    min_x: x(left),
                    min_y: y(top + h),
                    max_x: x(left + w),
                    max_y: y(top),
                },
                w,
                h,
            )?;
            if tile.width != w || tile.height != h || tile.rgba.len() != w as usize * h as usize * 4
            {
                return Err("PDOK-tegel heeft ongeldige pixels".into());
            }
            for row in 0..h {
                crate::input::check(cancel)?;
                let target = ((top + row) as usize * width as usize + left as usize) * 4;
                let source = row as usize * w as usize * 4;
                output.as_mut()[target..target + w as usize * 4]
                    .copy_from_slice(&tile.rgba[source..source + w as usize * 4]);
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pdok_url_matches_legacy_parameters() {
        let b = Bounds {
            min_x: 160000.,
            min_y: 400000.,
            max_x: 161000.,
            max_y: 401000.,
        };
        let url = build_wms_url(b, 1024, 768).unwrap();
        assert_eq!(url.host_str(), Some("service.pdok.nl"));
        let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(query["LAYERS"], "Actueel_orthoHR");
        assert_eq!(query["SRS"], "EPSG:28992");
        assert_eq!(query["BBOX"], "160000,400000,161000,401000");
        assert_eq!(query["WIDTH"], "1024");
    }
    #[test]
    fn rejects_invalid_dimensions() {
        let b = Bounds {
            min_x: 1.,
            min_y: 2.,
            max_x: 3.,
            max_y: 4.,
        };
        assert!(build_wms_url(b, 0, 10).is_err());
        assert!(build_wms_url(b, 4096, 4096).is_err());
    }
    #[test]
    fn export_tiles_share_pixel_boundaries_and_preserve_every_pixel() {
        let b = Bounds {
            min_x: 100.,
            min_y: 200.,
            max_x: 4201.,
            max_y: 2251.,
        };
        let mut calls = Vec::new();
        let image = tiled(b, 4101, 2051, &AtomicBool::new(false), |bounds, w, h| {
            calls.push((bounds, w, h));
            let ox = (bounds.min_x - 100.).round() as u32;
            let oy = (2251. - bounds.max_y).round() as u32;
            Ok(PdokRaster {
                bounds,
                width: w,
                height: h,
                rgba: (0..h)
                    .flat_map(|y| {
                        (0..w).flat_map(move |x| {
                            [((x + ox) % 251) as u8, ((y + oy) % 239) as u8, 42, 255]
                        })
                    })
                    .collect(),
            })
        })
        .unwrap();
        assert_eq!(calls.len(), 6);
        assert_eq!(calls[0].0.max_x, calls[1].0.min_x);
        assert_eq!(calls[0].0.min_y, calls[3].0.max_y);
        for (x, y, p) in image.enumerate_pixels() {
            assert_eq!(p.0, [(x % 251) as u8, (y % 239) as u8, 42, 255]);
        }
        assert!(
            tiled(b, 4101, 2051, &AtomicBool::new(true), |_, _, _| panic!(
                "cancel before network"
            ))
            .is_err()
        );
    }
    #[test]
    fn response_stream_is_bounded_without_content_length() {
        struct Infinite(usize);
        impl Read for Infinite {
            fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
                self.0 += b.len();
                b.fill(0);
                Ok(b.len())
            }
        }
        assert!(read_response(Infinite(0), &AtomicBool::new(false)).is_err());
    }
    #[test]
    fn real_pdok_jpeg_returned_to_a_png_request_is_supported_with_bounded_dimensions() {
        let jpeg = include_bytes!("../tests/fixtures/map-export/pdok-getmap-jpeg.jpg");
        let cancel = AtomicBool::new(false);
        let rgba = decode_response(jpeg, 64, 64, &cancel).unwrap();
        assert_eq!(rgba.len(), 64 * 64 * 4);
        assert!(rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        let expected = image::load_from_memory(include_bytes!(
            "../tests/fixtures/map-export/pdok-expected.png"
        ))
        .unwrap()
        .into_rgba8();
        let max = rgba
            .iter()
            .zip(expected.as_raw())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        // Independent JPEG IDCT/color rounding differs from libjpeg: this real
        // subsampled image has up to three byte values difference. PNG remains
        // exact. Also constrain mean error so a broad color shift cannot pass.
        let sum: usize = rgba
            .iter()
            .zip(expected.as_raw())
            .map(|(a, b)| usize::from(a.abs_diff(*b)))
            .sum();
        assert!(max <= 3, "maximum JPEG channel difference {max}");
        assert!(sum <= rgba.len() / 2, "mean JPEG channel error exceeds 0.5");
        assert!(decode_response(jpeg, 63, 64, &cancel).is_err());
        assert!(decode_response(jpeg, 2048, 2048, &cancel).is_err());
        assert!(decode_response(
            b"<ServiceExceptionReport>not an image</ServiceExceptionReport>",
            64,
            64,
            &cancel
        )
        .is_err());
        assert!(decode_response(jpeg, 64, 64, &AtomicBool::new(true)).is_err());
    }
}
