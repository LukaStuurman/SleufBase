//! Absolute GeoTIFF rotation about its original affine centre; pixels stay shared.
use crate::{geo::GeoTransform, geotiff::GeoTiffInfo, python_values};
use serde_json::{json, Value};
pub const BASE: &str = "base_transform_matrix";
pub const ANGLE: &str = "rotation_degrees";
pub fn normalize(angle: f64) -> Result<f64, String> {
    if !angle.is_finite() {
        return Err("Geef een eindige rotatiehoek op".into());
    }
    let mut angle = angle % 360.;
    if angle <= -180. {
        angle += 360.;
    } else if angle > 180. {
        angle -= 360.;
    }
    Ok(if angle.abs() < 1e-9 { 0. } else { angle })
}
pub fn parse(text: &str) -> Result<f64, String> {
    normalize(
        python_values::number(Some(&Value::String(text.replace(',', "."))))
            .ok_or("Voer een geldig aantal graden in")?,
    )
}
pub fn angle(raster: &GeoTiffInfo) -> f64 {
    python_values::number(raster.metadata.get(ANGLE)).unwrap_or(0.)
}
pub fn matrix(transform: GeoTransform) -> Value {
    json!([
        transform.a,
        transform.b,
        transform.c,
        transform.d,
        transform.e,
        transform.f,
        0.,
        0.,
        1.
    ])
}
fn base(raster: &GeoTiffInfo) -> Result<(GeoTransform, bool), String> {
    if let Some(values) = raster
        .metadata
        .get(BASE)
        .and_then(Value::as_array)
        .filter(|v| v.len() == 9)
    {
        if let Some(v) = values
            .iter()
            .map(|v| python_values::number(Some(v)))
            .collect::<Option<Vec<_>>>()
        {
            if v.iter().any(|v| !v.is_finite()) || v[6] != 0. || v[7] != 0. || v[8] != 1. {
                return Err("Ongeldige affiene basisrastermatrix".into());
            }
            let base = GeoTransform {
                a: v[0],
                b: v[1],
                c: v[2],
                d: v[3],
                e: v[4],
                f: v[5],
            };
            base.world_to_pixel(base.c, base.f)?;
            base.bounds(raster.width, raster.height)?;
            return Ok((base, false));
        }
    }
    // Earlier native Python imports stored an already rotated transform but no
    // original matrix. Recover that basis once, rather than rotating it twice.
    let old = normalize(angle(raster))?;
    let base = if old == 0. {
        raster.transform
    } else {
        raster
            .transform
            .rotate_degrees(-old, raster.width, raster.height)?
    };
    base.world_to_pixel(base.c, base.f)?;
    base.bounds(raster.width, raster.height)?;
    Ok((base, true))
}
pub fn set(raster: &mut GeoTiffInfo, degrees: f64) -> Result<bool, String> {
    let angle = normalize(degrees)?;
    let bytes = crate::raster_points::validate_metadata(&raster.metadata)?;
    let (base, store_base) = base(raster)?;
    let transform = if angle == 0. {
        base
    } else {
        base.rotate_degrees(angle, raster.width, raster.height)?
    };
    let bounds = transform.bounds(raster.width, raster.height)?;
    let value = json!(angle);
    if !store_base && raster.transform == transform && raster.metadata.get(ANGLE) == Some(&value) {
        return Ok(false);
    }
    if bytes + if store_base { 512 } else { 64 } > 2 * 1024 * 1024 {
        return Err("Geen ruimte voor rasterrotatie in laagmetadata".into());
    }
    if raster.metadata.is_null() {
        raster.metadata = json!({});
    }
    let metadata = raster.metadata.as_object_mut().unwrap();
    if store_base {
        metadata.insert(BASE.into(), matrix(base));
    }
    metadata.insert(ANGLE.into(), value);
    raster.transform = transform;
    raster.bounds = bounds;
    Ok(true)
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Overlay {
    pub center: [f64; 2],
    pub radius: f64,
    pub handle: [f64; 2],
    pub footprint: Vec<[f64; 2]>,
    pub rotation: f64,
}
pub fn overlay(
    raster: &GeoTiffInfo,
    project: impl Fn([f64; 2]) -> [f64; 2],
) -> Result<Overlay, String> {
    let center = project(crate::raster_points::center(raster));
    let mut footprint = Vec::with_capacity(4);
    for (x, y) in [
        (0., 0.),
        (raster.width as f64, 0.),
        (raster.width as f64, raster.height as f64),
        (0., raster.height as f64),
    ] {
        let (x, y) = raster.transform.pixel_to_world(x, y);
        footprint.push(project([x, y]));
    }
    let radius = (footprint
        .iter()
        .map(|p| (p[0] - center[0]).hypot(p[1] - center[1]))
        .fold(0., f64::max)
        + 28.)
        .max(42.);
    let rotation = angle(raster);
    let radians = (rotation + 90.).to_radians();
    let handle = [
        center[0] + radians.cos() * radius,
        center[1] - radians.sin() * radius,
    ];
    if footprint
        .iter()
        .flatten()
        .chain(center.iter())
        .chain(handle.iter())
        .any(|v| !v.is_finite())
        || !radius.is_finite()
        || !rotation.is_finite()
    {
        return Err("Ongeldige rotatiecirkel".into());
    }
    Ok(Overlay {
        center,
        radius,
        handle,
        footprint,
        rotation,
    })
}
pub fn hit(overlay: &Overlay, p: [f64; 2]) -> bool {
    p.iter().all(|v| v.is_finite())
        && (((p[0] - overlay.center[0]).hypot(p[1] - overlay.center[1]) - overlay.radius).abs()
            <= 18.
            || (p[0] - overlay.handle[0]).hypot(p[1] - overlay.handle[1]) <= 16.)
}
pub fn screen_angle(center: [f64; 2], pointer: [f64; 2]) -> f64 {
    (center[1] - pointer[1])
        .atan2(pointer[0] - center[0])
        .to_degrees()
}
pub fn dragged(initial: f64, start: f64, current: f64) -> Result<f64, String> {
    normalize(initial + (current - start + 180.).rem_euclid(360.) - 180.)
}
