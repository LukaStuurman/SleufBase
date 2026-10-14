use crate::model::{Dataset, Feature, Geometry, Vertex};
use serde_json::Value;
use std::fs;
use std::path::Path;

const MAX_JSON_SIZE: u64 = 256 * 1024 * 1024;

fn property_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn coordinate(value: Option<&Value>) -> Option<f64> {
    let val = match value? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None
    }?;
    val.is_finite().then_some(val)
}

fn vertex(value: &Value) -> Option<Vertex> {
    let coordinates = value.as_array()?;
    Some(Vertex {
        x: coordinate(coordinates.first())?,
        y: coordinate(coordinates.get(1))?,
        z: coordinate(coordinates.get(2)),
    })
}

/// Parse a KickTheMap jobFeatures.json using the same filtering rules as
/// kickthemap_dxf_export.parse_job_features_dataset_text.
pub fn parse_job_features(content: &str, job_id: u64, title: &str) -> Result<Dataset, String> {
    let root: Value = serde_json::from_str(content)
        .map_err(|e| format!("Ongeldige jobFeatures.json: {e}"))?;
    let raw = root.get("features").and_then(Value::as_array)
        .ok_or("jobFeatures.json bevat geen features-lijst")?;
    let mut features = Vec::new();

    for (index, item) in raw.iter().enumerate() {
        let Some(geometry) = item.get("geometry") else { continue };
        let kind = geometry.get("type").and_then(Value::as_str)
            .unwrap_or("").trim().to_ascii_lowercase();
        if !matches!(kind.as_str(), "point" | "polyline" | "linestring") { continue; }
        let properties = item.get("properties").and_then(Value::as_object);
        let group_type = properties
            .and_then(|p| p.get("groupType")).and_then(Value::as_str)
            .unwrap_or("").trim().to_ascii_lowercase();
        if !matches!(group_type.as_str(), "" | "objects") { continue; }

        let source_name = properties.map(|p| {
            let first = property_text(p.get("idName"));
            if first.is_empty() { property_text(p.get("name")) } else { first }
        }).unwrap_or_default();
        let object_name = if source_name.is_empty() {
            format!("Object {}", index + 1)
        } else { source_name.clone() };
        let attribute = |key: &str| properties.map(|p| property_text(p.get(key))).unwrap_or_default();
        let attributes = [
            attribute("properties1"), attribute("properties2"), attribute("properties3")
        ];

        let coordinates = geometry.get("coordinates");
        let parsed_geometry = if kind == "point" {
            coordinates.and_then(vertex).map(Geometry::Point)
        } else {
            coordinates.and_then(Value::as_array).and_then(|array| {
                let points: Vec<Vertex> = array.iter().filter_map(vertex).collect();
                (points.len() >= 2).then_some(Geometry::Polyline(points))
            })
        };
        if let Some(geometry) = parsed_geometry {
            features.push(Feature {
                object_name, source_name, geometry, attributes
            });
        }
    }
    if features.is_empty() {
        return Err("Geen bruikbare objectpunten of objectpolylijnen gevonden".to_owned());
    }
    Ok(Dataset {
        job_id, title: title.to_owned(), source_path: Default::default(), features
    })
}

pub fn load_job_features(path: &Path) -> Result<Dataset, String> {
    let size = fs::metadata(path).map_err(|e| format!("Bestand niet toegankelijk: {e}"))?.len();
    if size > MAX_JSON_SIZE {
        return Err("JSON-bestand is groter dan 256 MiB".into());
    }
    let content = fs::read_to_string(path).map_err(|e| format!("JSON lezen mislukt: {e}"))?;
    let title = path.parent().and_then(Path::file_name).and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .or_else(|| path.file_stem().and_then(|s| s.to_str()))
        .unwrap_or("Onbekende job");
    let sidecar_id = path.parent().and_then(Path::file_name).and_then(|s| s.to_str())
        .and_then(|s| s.split('_').next_back()).and_then(|s| s.parse::<u64>().ok());
    let mut dataset = parse_job_features(&content, sidecar_id.unwrap_or(0), title)?;
    dataset.source_path = path.to_owned();
    Ok(dataset)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn point_and_linestring_and_ignored_features() {
        let json = r#"{"features":[
            {"geometry":{"type":"Point","coordinates":[1,2,3]},"properties":{"groupType":"objects","idName":"water"}},
            {"geometry":{"type":"LineString","coordinates":[[3,4],[5,6]]},"properties":{"name":"ls"}},
            {"geometry":{"type":"Point","coordinates":[0,0]},"properties":{"groupType":"annotations"}},
            {"geometry":{"type":"Point","coordinates":["NaN",1]}}
        ]}"#;
        let result = parse_job_features(json, 42, "PS42").unwrap();
        assert_eq!(result.features.len(), 2);
        assert_eq!(result.features[0].source_name, "water");
        assert_eq!(result.features[0].attributes, ["", "", ""]);
        assert!(matches!(result.features[1].geometry, Geometry::Polyline(_)));
        assert_eq!(result.job_id, 42);
    }
    #[test]
    fn invalid_and_empty_json() {
        assert!(parse_job_features("no", 1, "x").is_err());
        assert!(parse_job_features(r#"{"features":[]}"#, 1, "x").is_err());
        assert!(parse_job_features(r#"{"hello":true}"#, 1, "x").is_err());
    }
}