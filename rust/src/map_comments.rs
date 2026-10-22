//! Original free-map and per-TIFF comments, independent of the desktop UI.
use crate::{geotiff::GeoTiffInfo, project::Comment, python_values};
use serde_json::{Map, Value};
use std::borrow::Cow;
pub const KEY: &str = "tiff_comment_text";
const MAX_TEXT: usize = 64 * 1024;
const MAX_EXPORT_TEXT: usize = 16 * 1024 * 1024;

pub fn text(metadata: &Value) -> Result<String, String> {
    let value = metadata.get(KEY).filter(|v| python_values::truth(v));
    Ok(python_values::trim(&python_values::string(value)?).to_owned())
}

pub fn set(raster: &mut GeoTiffInfo, text: &str) -> Result<bool, String> {
    let text = python_values::trim(text);
    if text.len() > MAX_TEXT {
        return Err("Opmerking overschrijdt 64 KiB".into());
    }
    crate::raster_points::validate_metadata(&raster.metadata)?;
    let mut next = raster.metadata.as_object().cloned().unwrap_or_default();
    if text.is_empty() {
        next.remove(KEY);
    } else {
        next.insert(KEY.into(), Value::String(text.into()));
    }
    if next == *raster.metadata.as_object().unwrap_or(&Map::new()) {
        return Ok(false);
    }
    let next = Value::Object(next);
    crate::raster_points::validate_metadata(&next)?;
    raster.metadata = next;
    Ok(true)
}

/// Count and validate before changing anything, including Python-like imported values.
pub fn clear(comments: &mut Vec<Comment>, rasters: &mut [GeoTiffInfo]) -> Result<usize, String> {
    let mut indices = Vec::new();
    for (i, raster) in rasters.iter().enumerate() {
        if !text(&raster.metadata)?.is_empty() {
            indices.push(i);
        }
    }
    let count = comments.len() + indices.len();
    if count > 0 {
        for i in indices {
            rasters[i].metadata.as_object_mut().unwrap().remove(KEY);
        }
        comments.clear();
    }
    Ok(count)
}

/// The original app appends each TIFF note at its actual affine image centre.
/// This vector is prepared once per export, never once per slot or canvas frame.
pub fn combined(comments: &[Comment], rasters: &[GeoTiffInfo]) -> Result<Vec<Comment>, String> {
    let mut result = Vec::new();
    let mut bytes = 0usize;
    let mut push = |x: f64, y: f64, text: Cow<'_, str>| -> Result<(), String> {
        if !x.is_finite() || !y.is_finite() {
            return Err("Ongeldige opmerkingcoördinaten".into());
        }
        bytes = bytes.saturating_add(text.len());
        if bytes > MAX_EXPORT_TEXT || result.len() >= 100_000 {
            return Err("Te veel opmerkingen voor één export".into());
        }
        result.push(Comment {
            x,
            y,
            text: text.into_owned(),
        });
        Ok(())
    };
    for comment in comments
        .iter()
        .filter(|c| !python_values::trim(&c.text).is_empty())
    {
        push(comment.x, comment.y, Cow::Borrowed(&comment.text))?;
    }
    for raster in rasters {
        let text = text(&raster.metadata)?;
        if !text.is_empty() {
            if raster.epsg.is_some_and(|epsg| epsg != 28992) {
                return Err("TIFF-opmerkingen in een sjabloon vereisen RD (EPSG:28992)".into());
            }
            let [x, y] = crate::raster_points::center(raster);
            push(x, y, Cow::Owned(text))?;
        }
    }
    Ok(result)
}
