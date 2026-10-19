//! Original BGT matrix addressing and per-tile RD geometries. These raw parts
//! still require merge/make-valid/clip before the profile appearance stage.
use crate::{
    bgt_surface::Geometry,
    input, mvt,
    python_values::{self, casefold, trim},
    Bounds,
};
use std::sync::{atomic::AtomicBool, Arc};
pub const SPAN: f64 = 256. * 0.84;
pub const ORIGIN_X: f64 = -285401.92;
pub const ORIGIN_Y: f64 = 903401.92;
pub const MATRIX_SIZE: u16 = 4096;
pub const MAX_TILE_BYTES: usize = 24 * 1024 * 1024;
pub const MAX_DECODED_BYTES: usize = 64 * 1024 * 1024;
const MAX_ID: usize = 1024 * 1024;
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct TileId {
    pub row: u16,
    pub col: u16,
}
impl TileId {
    pub fn validate(self) -> Result<(), String> {
        if self.row < MATRIX_SIZE && self.col < MATRIX_SIZE {
            Ok(())
        } else {
            Err("BGT-tegel buiten matrix".into())
        }
    }
    pub fn url(self) -> Result<String, String> {
        self.validate()?;
        Ok(format!(
            "https://api.pdok.nl/lv/bgt/ogc/v1/tiles/NetherlandsRDNewQuad/12/{}/{}?f=mvt",
            self.row, self.col
        ))
    }
}
pub fn tiles_for_bounds(bounds: Bounds) -> Result<Vec<TileId>, String> {
    if [bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y]
        .iter()
        .any(|v| !v.is_finite())
        || bounds.min_x > bounds.max_x
        || bounds.min_y > bounds.max_y
    {
        return Err("Ongeldige BGT-uitsnede".into());
    }
    let clamp = |n: f64| n.clamp(0., 4095.) as u16;
    let min_col = clamp(((bounds.min_x - ORIGIN_X) / SPAN).floor());
    let max_col = clamp(((bounds.max_x - ORIGIN_X - 1e-9) / SPAN).floor());
    let min_row = clamp(((ORIGIN_Y - bounds.max_y) / SPAN).floor());
    let max_row = clamp(((ORIGIN_Y - bounds.min_y - 1e-9) / SPAN).floor());
    if min_col > max_col || min_row > max_row {
        return Ok(Vec::new());
    }
    let count =
        (max_col as usize - min_col as usize + 1) * (max_row as usize - min_row as usize + 1);
    if count > 4096 {
        return Err("BGT-uitsnede omvat meer dan 4096 tegels".into());
    }
    Ok((min_row..=max_row)
        .flat_map(|row| (min_col..=max_col).map(move |col| TileId { row, col }))
        .collect())
}
#[derive(Debug, serde::Serialize)]
pub struct TileFeature {
    pub layer_name: Arc<str>,
    pub feature_id: String,
    pub physical_appearance: String,
    pub geometry: Geometry,
}
#[derive(Debug)]
pub struct Tile {
    pub features: Vec<TileFeature>,
    pub memory_bytes: usize,
}
fn text(value: Option<&serde_json::Value>, limit: usize) -> Result<String, String> {
    if value
        .and_then(serde_json::Value::as_str)
        .is_some_and(|s| s.len() > limit)
    {
        return Err("BGT-eigenschap te lang".into());
    }
    let s = python_values::string(value)?;
    if s.len() > limit {
        return Err("BGT-eigenschap te lang".into());
    }
    Ok(s)
}
fn excluded(name: &str) -> bool {
    matches!(
        name,
        "buurt"
            | "gemeente"
            | "landsgrens"
            | "openbareruimtelabel"
            | "pand_nummeraanduiding"
            | "provincie"
            | "wijk"
    )
}
fn geometry(
    raw: mvt::Geometry,
    id: TileId,
    extent: u32,
    cancel: &AtomicBool,
) -> Result<Option<Geometry>, String> {
    let tile_min_x = ORIGIN_X + id.col as f64 * SPAN;
    let tile_max_y = ORIGIN_Y - id.row as f64 * SPAN;
    let tile_min_y = tile_max_y - SPAN;
    let scale = if extent == 0 { 4096. } else { extent as f64 };
    let mut count = 0;
    let mut path = |points: Vec<mvt::XY>, close: bool| -> Result<Vec<[f64; 2]>, String> {
        let mut result = Vec::with_capacity(points.len() + usize::from(close));
        for p in points {
            if count % 1024 == 0 {
                input::check(cancel)?;
            }
            count += 1;
            result.push([
                tile_min_x + (p[0] as f64 / scale * SPAN),
                tile_min_y + (p[1] as f64 / scale * SPAN),
            ]);
        }
        if close && result.first() != result.last() {
            result.push(result[0]);
        }
        Ok(result)
    };
    match raw {
        mvt::Geometry::Point(_) => Ok(None),
        mvt::Geometry::Line(parts) => {
            if parts.is_empty() || parts.iter().any(|p| p.len() < 2) {
                return Ok(None);
            }
            let mut result = parts
                .into_iter()
                .map(|p| path(p, false))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Some(if result.len() == 1 {
                Geometry::LineString(result.remove(0))
            } else {
                Geometry::MultiLineString(result)
            }))
        }
        mvt::Geometry::Polygon(polygons) => {
            if polygons.is_empty()
                || polygons
                    .iter()
                    .any(|p| p.is_empty() || p.iter().any(|r| r.len() < 3))
            {
                return Ok(None);
            }
            let mut result = Vec::with_capacity(polygons.len());
            for polygon in polygons {
                result.push(
                    polygon
                        .into_iter()
                        .map(|r| path(r, true))
                        .collect::<Result<Vec<_>, _>>()?,
                );
            }
            Ok(Some(if result.len() == 1 {
                Geometry::Polygon(result.remove(0))
            } else {
                Geometry::MultiPolygon(result)
            }))
        }
    }
}
fn geometry_bytes(g: &Geometry) -> usize {
    use std::mem::size_of;
    match g {
        Geometry::Point(_) => 0,
        Geometry::MultiPoint(p) | Geometry::LineString(p) => p.capacity() * size_of::<[f64; 2]>(),
        Geometry::MultiLineString(p) | Geometry::Polygon(p) => {
            p.capacity() * size_of::<Vec<[f64; 2]>>()
                + p.iter()
                    .map(|r| r.capacity() * size_of::<[f64; 2]>())
                    .sum::<usize>()
        }
        Geometry::MultiPolygon(p) => {
            p.capacity() * size_of::<Vec<Vec<[f64; 2]>>>()
                + p.iter()
                    .map(|p| {
                        p.capacity() * size_of::<Vec<[f64; 2]>>()
                            + p.iter()
                                .map(|r| r.capacity() * size_of::<[f64; 2]>())
                                .sum::<usize>()
                    })
                    .sum::<usize>()
        }
        Geometry::Collection(p) => {
            p.capacity() * size_of::<Geometry>() + p.iter().map(geometry_bytes).sum::<usize>()
        }
    }
}
pub fn decode(bytes: &[u8], id: TileId, cancel: &AtomicBool) -> Result<Tile, String> {
    id.validate()?;
    input::check(cancel)?;
    if bytes.len() > MAX_TILE_BYTES {
        return Err("BGT-tegel te groot".into());
    }
    let layers = mvt::decode(bytes, cancel)?;
    let mut features = Vec::new();
    let mut memory = 0usize;
    for layer in layers {
        let normalized = casefold(trim(layer.name));
        if excluded(&normalized) {
            continue;
        }
        let name: Arc<str> = normalized.into();
        memory += name.len();
        for (index, f) in layer.features.into_iter().enumerate() {
            input::check(cancel)?;
            if matches!(f.geometry, mvt::Geometry::Point(_)) {
                continue;
            }
            let value = [f.local_id.as_deref(), f.external_id.as_deref()]
                .into_iter()
                .flatten()
                .find(|v| python_values::truth(v));
            let feature_id = if let Some(value) = value {
                text(Some(value), MAX_ID)?
            } else if f.id != 0 {
                f.id.to_string()
            } else {
                if f.geometry.vertices() > MAX_ID / 48 {
                    return Err("BGT-noodidentificatie te groot".into());
                }
                // Payload has only fixed ASCII geometry-type names and integer
                // arrays. Match Python dumps(sort_keys=True)'s separators.
                let payload = serde_json::to_string(&f.geometry.payload())
                    .map_err(|e| e.to_string())?
                    .replace(',', ", ")
                    .replace(':', ": ");
                format!("{}:{}:{index}:{payload}", id.row, id.col)
            };
            let appearance = f
                .appearance
                .as_deref()
                .filter(|v| python_values::truth(v))
                .map(|v| text(Some(v), 4096))
                .transpose()?
                .unwrap_or_default();
            let appearance = trim(&appearance).to_owned();
            let Some(geometry) = geometry(f.geometry, id, layer.extent, cancel)? else {
                continue;
            };
            memory = memory
                .checked_add(
                    std::mem::size_of::<TileFeature>()
                        + feature_id.capacity()
                        + appearance.capacity()
                        + geometry_bytes(&geometry),
                )
                .ok_or("BGT-geheugenberekening loopt over")?;
            if memory > MAX_DECODED_BYTES {
                return Err("Uitgelezen BGT-tegel is te groot".into());
            }
            features.push(TileFeature {
                layer_name: name.clone(),
                feature_id,
                physical_appearance: appearance,
                geometry,
            });
        }
    }
    memory += features.capacity() * std::mem::size_of::<TileFeature>();
    if memory > MAX_DECODED_BYTES {
        return Err("Uitgelezen BGT-tegel is te groot".into());
    }
    input::check(cancel)?;
    Ok(Tile {
        features,
        memory_bytes: memory,
    })
}
