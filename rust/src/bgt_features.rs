//! Source-feature grouping after tile retrieval; never expose raw tile parts as
//! prepared surfaces. Geometry remains owned here and borrowed by the index.
use crate::{
    bgt_geometry,
    bgt_surface::{Feature, Geometry},
    bgt_tiles::Tile,
    input,
    profile::XY,
    Bounds,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{atomic::AtomicBool, Arc},
};

#[derive(Debug, serde::Serialize)]
pub struct SurfaceFeature {
    pub layer_name: Arc<str>,
    pub feature_id: String,
    #[serde(flatten)]
    pub surface: Feature,
}
pub fn surface_index<'a>(
    features: &'a [SurfaceFeature],
    cancel: &AtomicBool,
) -> Result<crate::bgt_surface::Index<'a>, String> {
    crate::bgt_surface::Index::from_normalized_features(features.iter().map(|f| &f.surface), cancel)
}

pub fn surfaces<'a>(
    tiles: impl IntoIterator<Item = &'a Tile>,
    bounds: Bounds,
    cancel: &AtomicBool,
) -> Result<Vec<SurfaceFeature>, String> {
    let mut groups = BTreeMap::<(&str, &str, &str), Vec<&Geometry>>::new();
    let mut count = 0;
    for tile in tiles {
        for f in &tile.features {
            input::check(cancel)?;
            count += 1;
            if count > 100_000 {
                return Err("Te veel BGT-tegeldelen".into());
            }
            if !f.physical_appearance.is_empty()
                && matches!(f.geometry, Geometry::Polygon(_) | Geometry::MultiPolygon(_))
            {
                groups
                    .entry((&f.layer_name, &f.feature_id, &f.physical_appearance))
                    .or_default()
                    .push(&f.geometry);
            }
        }
    }
    let mut result = Vec::new();
    let mut vertices = 0;
    for ((layer, id, appearance), parts) in groups {
        if let Some(geometry) = bgt_geometry::normalize(&parts, bounds, cancel)? {
            vertices += bgt_geometry::vertex_count(&geometry, cancel)?;
            if vertices > 2_000_000 {
                return Err("BGT-oppervlakken na samenvoegen te groot".into());
            }
            result.push(SurfaceFeature {
                layer_name: Arc::from(layer),
                feature_id: id.into(),
                surface: Feature {
                    physical_appearance: appearance.into(),
                    geometry,
                },
            });
        }
    }
    input::check(cancel)?;
    Ok(result)
}

fn extract(g: Geometry, result: &mut Vec<Vec<XY>>) {
    match g {
        Geometry::LineString(p) => result.push(p),
        Geometry::MultiLineString(p) | Geometry::Polygon(p) => result.extend(p),
        Geometry::MultiPolygon(p) => result.extend(p.into_iter().flatten()),
        Geometry::Collection(p) => {
            for g in p {
                extract(g, result);
            }
        }
        Geometry::Point(_) | Geometry::MultiPoint(_) => (),
    }
}
/// Same three-decimal, reversal-only deduplication as Python. Decimal precision
/// formatting rounds the binary float directly, avoiding multiply-then-round
/// double rounding. Keep the unrounded first path for drawing.
pub fn dedupe(paths: Vec<Vec<XY>>, cancel: &AtomicBool) -> Result<Vec<Vec<XY>>, String> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for path in paths {
        input::check(cancel)?;
        if path.len() < 2 {
            continue;
        }
        let mut key = Vec::with_capacity(path.len());
        for p in &path {
            let mut pair = [0; 2];
            for i in 0..2 {
                if !p[i].is_finite() || p[i].abs() > 1e12 {
                    return Err("Ongeldige BGT-padcoördinaat".into());
                }
                let value = format!("{:.3}", p[i])
                    .parse::<f64>()
                    .map_err(|_| "Ongeldige BGT-afronding")?;
                pair[i] = if value == 0. { 0 } else { value.to_bits() };
            }
            key.push(pair);
        }
        let reverse = key.iter().rev().copied().collect::<Vec<_>>();
        if reverse < key {
            key = reverse;
        }
        if seen.insert(key) {
            result.push(path);
        }
    }
    Ok(result)
}
pub fn paths<'a>(
    tiles: impl IntoIterator<Item = &'a Tile>,
    bounds: Bounds,
    cancel: &AtomicBool,
) -> Result<Vec<Vec<XY>>, String> {
    let mut groups = BTreeMap::<(&str, &str), Vec<&Geometry>>::new();
    let mut count = 0;
    for tile in tiles {
        for f in &tile.features {
            input::check(cancel)?;
            count += 1;
            if count > 100_000 {
                return Err("Te veel BGT-tegeldelen".into());
            }
            groups
                .entry((&f.layer_name, &f.feature_id))
                .or_default()
                .push(&f.geometry);
        }
    }
    let mut result = Vec::new();
    let mut vertices = 0;
    for parts in groups.into_values() {
        if let Some(g) = bgt_geometry::normalize(&parts, bounds, cancel)? {
            vertices += bgt_geometry::vertex_count(&g, cancel)?;
            if vertices > 2_000_000 {
                return Err("BGT-paden na samenvoegen te groot".into());
            }
            extract(g, &mut result);
        }
    }
    dedupe(result, cancel)
}
