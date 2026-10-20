use serde::Deserialize;
use sleufbase_rust::raster::{decode_preview, decode_preview_with_cancel, PreviewOptions};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

#[derive(Deserialize)]
struct Options {
    max_side: u32,
    max_pixels: u64,
}
#[derive(Deserialize)]
struct Case {
    file: String,
    width: u32,
    height: u32,
    crc32: u32,
    rgba: Option<Vec<u8>>,
    #[serde(default)]
    tolerance: u8,
    options: Option<Options>,
    max_peak_chunk_bytes: Option<usize>,
    expected_chunks: Option<u32>,
}
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters")
}

#[test]
fn tiff_strips_tiles_formats_and_orientation_match_pixel_oracles() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/rasters/manifest.json")).unwrap();
    assert_eq!(cases.len(), 47);
    for case in cases {
        let mut options = PreviewOptions::default();
        if let Some(o) = case.options {
            options.max_side = o.max_side;
            options.max_pixels = o.max_pixels;
        }
        let preview = decode_preview(&fixtures().join(&case.file), options)
            .unwrap_or_else(|e| panic!("{}: {e}", case.file));
        assert_eq!(
            (preview.width, preview.height),
            (case.width, case.height),
            "{} dimensions",
            case.file
        );
        assert_eq!(preview.rgba.len(), (case.width * case.height * 4) as usize);
        if let Some(expected) = case.rgba {
            if let Some(index) = preview
                .rgba
                .iter()
                .zip(&expected)
                .position(|(a, b)| a.abs_diff(*b) > case.tolerance)
            {
                panic!(
                    "{} pixel {} channel {}: actual {:?}, expected {:?}",
                    case.file,
                    index / 4,
                    index % 4,
                    &preview.rgba[(index / 4) * 4..(index / 4) * 4 + 4],
                    &expected[(index / 4) * 4..(index / 4) * 4 + 4]
                );
            }
        }
        if case.tolerance == 0 {
            assert_eq!(
                crc32fast::hash(&preview.rgba),
                case.crc32,
                "{} RGBA pixels",
                case.file
            );
        }
        if let Some(limit) = case.max_peak_chunk_bytes {
            assert!(
                preview.stats.peak_chunk_bytes <= limit,
                "{} decoded too much at once",
                case.file
            );
        }
        if let Some(chunks) = case.expected_chunks {
            assert_eq!(
                preview.stats.decoded_chunks, chunks,
                "{} decoded chunk count",
                case.file
            );
        }
    }
}

#[test]
fn full_export_decoding_matches_existing_color_and_orientation_oracles_without_resizing() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/rasters/manifest.json")).unwrap();
    let mut checked = 0;
    for case in cases
        .into_iter()
        .filter(|c| c.options.is_none() && c.rgba.is_some())
    {
        let image = sleufbase_rust::raster::decode_full_with_cancel(
            &fixtures().join(&case.file),
            &AtomicBool::new(false),
        )
        .unwrap_or_else(|e| panic!("{}: {e}", case.file));
        assert_eq!(
            (image.width, image.height),
            (case.width, case.height),
            "{} full size",
            case.file
        );
        assert_eq!(
            image.stats.source_pixels,
            u64::from(case.width) * u64::from(case.height)
        );
        let expected = case.rgba.unwrap();
        assert_eq!(image.rgba.len(), expected.len());
        assert!(
            image
                .rgba
                .iter()
                .zip(expected)
                .all(|(a, b)| a.abs_diff(b) <= case.tolerance),
            "{} full pixels",
            case.file
        );
        checked += 1;
    }
    assert!(checked >= 35);
}
#[test]
fn full_export_of_a_large_strip_tiff_keeps_every_source_pixel() {
    let image = sleufbase_rust::raster::decode_full_with_cancel(
        &fixtures().join("large-striped.tiff"),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!((image.width, image.height), (4097, 4097));
    assert_eq!(image.stats.source_pixels, 4097 * 4097);
    assert_eq!(image.stats.decoded_chunks, 257);
    assert_eq!(image.stats.skipped_chunks, 0);
    assert!(image.stats.peak_chunk_bytes <= 4097 * 16 * 3);
    // The independent TIFF generator writes x, y, 17. Check every original
    // pixel, including the final row/column omitted by the 2048px preview.
    for (i, p) in image.rgba.as_chunks::<4>().0.iter().enumerate() {
        assert_eq!(
            *p,
            [(i % 4097 % 256) as u8, (i / 4097 % 256) as u8, 17, 255],
            "source pixel {i}"
        );
    }
}
#[test]
fn invalid_limits_and_cancellation_do_not_return_partial_images() {
    let path = fixtures().join("rgb8-strips.tiff");
    for options in [
        PreviewOptions {
            max_side: 0,
            ..Default::default()
        },
        PreviewOptions {
            max_pixels: 0,
            ..Default::default()
        },
        PreviewOptions {
            max_chunk_bytes: 1,
            ..Default::default()
        },
    ] {
        assert!(decode_preview(&path, options).is_err());
    }
    assert!(
        decode_preview_with_cancel(&path, PreviewOptions::default(), &AtomicBool::new(true))
            .unwrap_err()
            .contains("geannuleerd")
    );
}

#[test]
fn palette_tag_view_never_modifies_original_bytes_and_small_previews_skip_strips() {
    let path = fixtures().join("palette4.tiff");
    let original = std::fs::read(&path).unwrap();
    decode_preview(&path, PreviewOptions::default()).unwrap();
    assert_eq!(std::fs::read(path).unwrap(), original);
    let image = decode_preview(
        &fixtures().join("thin-skipped-strips.tiff"),
        PreviewOptions {
            max_side: 8,
            max_pixels: 64,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(image.stats.decoded_chunks, 8);
    assert_eq!(image.stats.skipped_chunks, 4089);
    assert_eq!(image.stats.peak_chunk_bytes, 1);
}

#[test]
fn georeference_dimensions_follow_oriented_raster_like_python() {
    for orientation in 1..=8 {
        let path = fixtures().join(format!("orientation{orientation}.tiff"));
        let info = sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap();
        let preview = decode_preview(&path, PreviewOptions::default()).unwrap();
        assert_eq!((info.width, info.height), (preview.width, preview.height));
        assert!(
            (info.bounds.max_x - info.bounds.min_x - f64::from(info.width) * 0.25).abs() < 1e-7
        );
        assert!(
            (info.bounds.max_y - info.bounds.min_y - f64::from(info.height) * 0.5).abs() < 1e-7
        );
    }
}

#[test]
fn truncated_and_invalid_dimensions_fail_without_partial_images() {
    let dir = tempfile::tempdir().unwrap();
    let original = std::fs::read(fixtures().join("rgb8-strips.tiff")).unwrap();
    let path = dir.path().join("bad.tiff");
    for length in [0, 2, 7, 16, 63] {
        std::fs::write(&path, &original[..length]).unwrap();
        assert!(decode_preview(&path, PreviewOptions::default()).is_err());
    }
    let ifd = u32::from_le_bytes(original[4..8].try_into().unwrap()) as usize;
    let count = u16::from_le_bytes(original[ifd..ifd + 2].try_into().unwrap()) as usize;
    for (tag, value) in [(256u16, 0u32), (256, u32::MAX), (278, 0)] {
        let mut bytes = original.clone();
        let mut changed = false;
        for i in 0..count {
            let entry = ifd + 2 + i * 12;
            if u16::from_le_bytes(bytes[entry..entry + 2].try_into().unwrap()) == tag {
                bytes[entry + 2..entry + 4].copy_from_slice(&4u16.to_le_bytes());
                bytes[entry + 4..entry + 8].copy_from_slice(&1u32.to_le_bytes());
                bytes[entry + 8..entry + 12].copy_from_slice(&value.to_le_bytes());
                changed = true;
                break;
            }
        }
        assert!(changed);
        std::fs::write(&path, bytes).unwrap();
        assert!(
            decode_preview(&path, PreviewOptions::default()).is_err(),
            "tag {tag}={value}"
        );
    }
}
