//! Bounded, native TIFF raster decode for the desktop map preview.
//! This is intentionally a limited preview (8-bit RGB/RGBA/Gray), NOT full
//! GeoTIFF rendering parity for all large engineering imagery formats.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use tiff::decoder::{Decoder, DecodingResult};
use tiff::ColorType;

pub const MAX_PREVIEW_PIXELS: u64 = 16_000_000;

fn convert_rgba(
    colors: ColorType,
    result: DecodingResult,
    pixels: usize
) -> Result<Vec<u8>, String> {
    let DecodingResult::U8(buffer) = result else {
        return Err("Preview ondersteunt momenteel alleen 8-bit TIFF-rasters".into());
    };
    let (channels, append_alpha) = match colors {
        ColorType::RGB(8) => (3usize, true),
        ColorType::RGBA(8) => (4usize, false),
        ColorType::Gray(8) => (1usize, true),
        _ => return Err("Dit TIFF-kleurtype wordt nog niet weergegeven".into()),
    };
    let expected = pixels.checked_mul(channels)
        .ok_or("TIFF-preview is te groot")?;
    if buffer.len() != expected {
        return Err("TIFF-pixels hebben niet de verwachte afmeting".into());
    }
    let mut rgba = Vec::with_capacity(pixels * 4);
    for src in buffer.chunks_exact(channels) {
        match channels {
            1 => rgba.extend_from_slice(&[src[0],src[0],src[0],255]),
            3 if append_alpha => rgba.extend_from_slice(&[src[0],src[1],src[2],255]),
            4 => rgba.extend_from_slice(src),
            _ => unreachable!(),
        }
    }
    Ok(rgba)
}

pub fn decode_tiff_preview(path: &Path) -> Result<(u32,u32,Vec<u8>), String> {
    let source = File::open(path).map_err(|e| format!("Rasterbestand openen mislukt: {e}"))?;
    let mut decoder = Decoder::new(BufReader::new(source))
        .map_err(|e| format!("Rasterbestand ongeldig: {e}"))?;
    let (width,height) = decoder.dimensions().map_err(|e| e.to_string())?;
    let pixels = u64::from(width) * u64::from(height);
    if pixels==0 || pixels>MAX_PREVIEW_PIXELS {
        return Err(format!(
            "Raster van {pixels} pixels wordt niet direct getoond (previewlimiet {MAX_PREVIEW_PIXELS})"
        ));
    }
    let color = decoder.colortype().map_err(|e|e.to_string())?;
    let data = decoder.read_image().map_err(|e|format!("TIFF-decodering mislukt: {e}"))?;
    let rgba = convert_rgba(color,data,pixels as usize)?;
    Ok((width,height,rgba))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn convert_rgb_gray_and_rgba() {
        assert_eq!(
            convert_rgba(ColorType::RGB(8),DecodingResult::U8(vec![10,20,30]),1).unwrap(),
            vec![10,20,30,255]
        );
        assert_eq!(
            convert_rgba(ColorType::Gray(8),DecodingResult::U8(vec![17]),1).unwrap(),
            vec![17,17,17,255]
        );
        assert_eq!(
            convert_rgba(ColorType::RGBA(8),DecodingResult::U8(vec![10,20,30,40]),1).unwrap(),
            vec![10,20,30,40]
        );
        assert!(convert_rgba(ColorType::RGB(8),DecodingResult::U8(vec![1,2]),1).is_err());
    }
}