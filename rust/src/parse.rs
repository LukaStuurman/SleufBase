use crate::model::{Dataset, Feature, Geometry, Vertex};
use serde_json::Value;
use std::path::Path;

const MAX_JSON_SIZE: u64 = 256 * 1024 * 1024;

fn vertex(value: &Value) -> Option<Vertex> {
    let coordinates = value.as_array()?;
    Some(Vertex {
        x: crate::python_values::coordinate(coordinates.first())?,
        y: crate::python_values::coordinate(coordinates.get(1))?,
        z: crate::python_values::coordinate(coordinates.get(2)),
    })
}

/// Parse a KickTheMap jobFeatures.json using the same filtering rules as
/// kickthemap_dxf_export.parse_job_features_dataset_text.
pub fn parse_job_features(content: &str, job_id: u64, title: &str) -> Result<Dataset, String> {
    parse_job_features_with_cancel(content, job_id, title, &crate::input::NEVER_CANCEL)
}
pub fn parse_job_features_with_cancel(
    content: &str,
    job_id: u64,
    title: &str,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Dataset, String> {
    parse_with_limits(content, job_id, title, cancel, 2_000_000, 10_000_000)
}
fn parse_with_limits(
    content: &str,
    job_id: u64,
    title: &str,
    cancel: &std::sync::atomic::AtomicBool,
    max_features: usize,
    max_points: usize,
) -> Result<Dataset, String> {
    use crate::python_values as py;
    crate::input::check(cancel)?;
    let root: Value =
        serde_json::from_str(content).map_err(|e| format!("Ongeldige jobFeatures.json: {e}"))?;
    let raw = root
        .get("features")
        .and_then(Value::as_array)
        .ok_or("jobFeatures.json bevat geen features-lijst")?;
    let mut features = Vec::new();
    let mut used = 0usize;
    let mut consume = || -> Result<(), String> {
        used = used
            .checked_add(1)
            .filter(|n| *n <= max_points)
            .ok_or("Objectinvoer overschrijdt het puntbudget")?;
        Ok(())
    };

    for (index, item) in raw.iter().enumerate() {
        if index % 256 == 0 {
            crate::input::check(cancel)?;
        }
        let Some(geometry) = item.get("geometry").and_then(Value::as_object) else {
            continue;
        };
        let kind = py::string(geometry.get("type"))?.to_lowercase();
        if !matches!(kind.as_str(), "point" | "polyline" | "linestring") {
            continue;
        }
        let properties = item.get("properties").and_then(Value::as_object);
        let property = |key: &str| properties.and_then(|p| p.get(key));
        let group_type = py::string(property("groupType"))?.to_lowercase();
        if !matches!(group_type.as_str(), "" | "objects") {
            continue;
        }
        // Python selects the truthy value BEFORE trimming its string form.
        let source_name = py::string(
            property("idName")
                .filter(|v| py::truth(v))
                .or_else(|| property("name")),
        )?;
        let object_name = if source_name.is_empty() {
            format!("Object {}", index + 1)
        } else {
            source_name.clone()
        };
        let attribute = |key| py::string(property(key).filter(|v| py::truth(v)));
        let attributes = [
            attribute("properties1")?,
            attribute("properties2")?,
            attribute("properties3")?,
        ];
        let coordinates = geometry.get("coordinates");
        let parsed_geometry = if kind == "point" {
            if let Some(point) = coordinates.and_then(vertex) {
                consume()?;
                Some(Geometry::Point(point))
            } else {
                None
            }
        } else if let Some(array) = coordinates.and_then(Value::as_array) {
            let mut points = Vec::new();
            for (at, raw) in array.iter().enumerate() {
                if at % 1024 == 0 {
                    crate::input::check(cancel)?;
                }
                if let Some(point) = vertex(raw) {
                    consume()?;
                    points
                        .try_reserve(1)
                        .map_err(|_| "Onvoldoende geheugen voor objectpunten")?;
                    points.push(point);
                }
            }
            (points.len() >= 2).then_some(Geometry::Polyline(points))
        } else {
            None
        };
        if let Some(geometry) = parsed_geometry {
            if features.len() >= max_features {
                return Err("Objectinvoer overschrijdt het objectbudget".into());
            }
            features
                .try_reserve(1)
                .map_err(|_| "Onvoldoende geheugen voor objecten")?;
            features.push(Feature {
                object_name,
                source_name,
                geometry,
                attributes,
            });
        }
    }
    if features.is_empty() {
        return Err("Geen bruikbare objectpunten of objectpolylijnen gevonden".to_owned());
    }
    crate::input::check(cancel)?;
    Ok(Dataset {
        job_id,
        title: title.to_owned(),
        source_path: Default::default(),
        features,
    })
}

pub fn load_job_features(path: &Path) -> Result<Dataset, String> {
    load_job_features_with_cancel(path, &crate::input::NEVER_CANCEL)
}
pub fn load_job_features_with_cancel(
    path: &Path,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Dataset, String> {
    let bytes = crate::input::read(path, MAX_JSON_SIZE, "JSON-bestand", cancel)?;
    let content = std::str::from_utf8(&bytes).map_err(|e| format!("JSON lezen mislukt: {e}"))?;
    // Signed-URL downloads and existing Python TIFF/jobFeatures bundles can
    // carry a safe job sidecar. Do not derive job identity only from a folder:
    // Rust stores all downloaded jobs together in a /Rust directory.
    let sidecar = crate::geotiff::job_sidecar(path);
    let filename_id = path
        .file_stem()
        .and_then(|v| v.to_str())
        .and_then(|v| v.strip_suffix("_jobFeatures"))
        .and_then(|v| v.split('_').next_back())
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0);
    let folder_id = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|s| s.to_str())
        .and_then(|s| s.split('_').next_back())
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|v| *v > 0);
    let job_id = sidecar
        .as_ref()
        .map(|v| v.0)
        .or(filename_id)
        .or(folder_id)
        .unwrap_or(0);
    let title = sidecar
        .as_ref()
        .map(|v| v.1.as_str())
        .filter(|v| !v.is_empty())
        .or_else(|| {
            path.parent()
                .and_then(Path::file_name)
                .and_then(|s| s.to_str())
        })
        .or_else(|| path.file_stem().and_then(|s| s.to_str()))
        .unwrap_or("Onbekende job");
    let mut dataset = parse_job_features_with_cancel(content, job_id, title, cancel)?;
    crate::input::check(cancel)?;
    dataset.source_path = path.to_owned();
    Ok(dataset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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
    fn downloaded_job_id_and_sidecar_are_not_lost() {
        let dir = tempfile::tempdir().unwrap();
        let json = dir.path().join("Breda_123_jobFeatures.json");
        let content = r#"{"features":[{"geometry":{"type":"Point","coordinates":[1,2]},
            "properties":{"idName":"water"}}]}"#;
        fs::write(&json, content).unwrap();
        let first = load_job_features(&json).unwrap();
        assert_eq!(first.job_id, 123);
        let sidecar = dir.path().join("Breda_123_jobFeatures.json.job.json");
        fs::write(&sidecar, r#"{"job_id":456,"title":"PS456"}"#).unwrap();
        let second = load_job_features(&json).unwrap();
        assert_eq!(second.job_id, 456);
        assert_eq!(second.title, "PS456");
    }
    #[test]
    fn invalid_and_empty_json() {
        assert!(parse_job_features("no", 1, "x").is_err());
        assert!(parse_job_features(r#"{"features":[]}"#, 1, "x").is_err());
        assert!(parse_job_features(r#"{"hello":true}"#, 1, "x").is_err());
    }

    #[test]
    fn whole_job_and_polyline_budgets_fail_without_partial_success() {
        let json = r#"{"features":[
            {"geometry":{"type":"Point","coordinates":[1,2]}},
            {"geometry":{"type":"LineString","coordinates":[[3,4],[5,6],[7,8]]}}
        ]}"#;
        let parse = |objects, points| {
            parse_with_limits(json, 1, "PS1", &crate::input::NEVER_CANCEL, objects, points)
        };
        assert_eq!(parse(2, 4).unwrap().features.len(), 2);
        assert!(parse(1, 4).unwrap_err().contains("objectbudget"));
        assert!(parse(2, 3).unwrap_err().contains("puntbudget"));
        assert!(parse(0, 4).unwrap_err().contains("objectbudget"));
        assert!(parse(2, 0).unwrap_err().contains("puntbudget"));
    }
}
