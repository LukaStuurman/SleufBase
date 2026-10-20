use image::{Rgba, RgbaImage};
use sleufbase_rust::{
    geo::GeoTransform,
    template_raster::{self, Options},
};
use std::sync::atomic::{AtomicBool, Ordering};
fn options() -> Options<'static> {
    Options {
        transform: GeoTransform {
            a: 1.,
            b: 0.,
            c: 0.,
            d: 0.,
            e: -1.,
            f: 0.,
        },
        roads: &[],
        terrain: &[],
        axis: None,
        reverse: false,
    }
}
#[test]
fn edge_background_is_four_connected_and_preserves_enclosed_white() {
    let mut image = RgbaImage::from_pixel(11, 11, Rgba([10, 20, 30, 127]));
    for i in 0..11 {
        image.put_pixel(i, 0, Rgba([255, 255, 255, 255]));
        image.put_pixel(0, i, Rgba([255, 255, 255, 255]));
    }
    for (x, y, a) in [(1, 1, 255), (2, 2, 255), (5, 5, 255), (6, 6, 10)] {
        image.put_pixel(x, y, Rgba([255, 255, 255, a]));
    }
    let prepared = template_raster::prepare(image, &options(), &AtomicBool::new(false)).unwrap();
    assert_eq!(prepared.details.crop, [0, 0, 11, 11]);
    for (x, y, a) in [
        (0, 0, 0),
        (1, 1, 0),
        (2, 2, 255),
        (5, 5, 255),
        (6, 6, 0),
        (9, 9, 255),
    ] {
        assert_eq!(prepared.image.get_pixel(x, y)[3], a, "{x},{y}");
    }
    assert!(prepared.image.pixels().all(|p| matches!(p[3], 0 | 255)));
}
#[test]
fn invalid_geometry_huge_expansion_and_cancellation_fail_before_output() {
    let cancel = AtomicBool::new(false);
    let image = RgbaImage::from_pixel(5, 7, Rgba([20, 40, 80, 255]));
    let mut invalid = options();
    invalid.transform.a = 0.;
    assert!(template_raster::prepare(image.clone(), &invalid, &cancel).is_err());
    invalid = options();
    invalid.transform.f = f64::NAN;
    assert!(template_raster::prepare(image.clone(), &invalid, &cancel).is_err());
    let mut oversized = options();
    oversized.axis = Some([[0., 0.], [1., 1.]]);
    assert!(template_raster::prepare(
        RgbaImage::from_pixel(50_000, 1, Rgba([0, 0, 0, 255])),
        &oversized,
        &cancel
    )
    .is_err());
    cancel.store(true, Ordering::Relaxed);
    assert!(template_raster::prepare(image, &options(), &cancel).is_err());
}
#[test]
fn png_encoding_is_atomic_and_never_replaces_existing_assets() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("image.png");
    let cancel = AtomicBool::new(false);
    let prepared = template_raster::prepare(
        RgbaImage::from_pixel(5, 7, Rgba([20, 40, 80, 255])),
        &options(),
        &cancel,
    )
    .unwrap();
    std::fs::write(&path, b"old asset").unwrap();
    assert!(template_raster::save(&prepared, &path, &cancel).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"old asset");
    let new = temp.path().join("new.png");
    cancel.store(true, Ordering::Relaxed);
    assert!(template_raster::save(&prepared, &new, &cancel).is_err());
    assert!(!new.exists());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    cancel.store(false, Ordering::Relaxed);
    template_raster::save(&prepared, &new, &cancel).unwrap();
    assert_eq!(image::open(new).unwrap().into_rgba8(), prepared.image);
}
