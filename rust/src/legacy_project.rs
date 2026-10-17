//! Explicit Python .klicproject v1 import. The original JSON is retained intact
//! in native saves, including fields not yet editable/rendered by the preview.
//! This does not claim full workflow parity or a Python-format exporter.
use crate::{
    dxf_read::load_dxf,
    geo::GeoTransform,
    geotiff::read_geotiff_metadata,
    marxact_alignment::MarXactAlignment,
    model::Vertex,
    project::{Comment, NativeProject, MAX_PROJECT_BYTES},
    virtual_trench::{TrenchPoint, VirtualTrench},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LegacyArchive {
    pub source_path: PathBuf,
    pub original: Arc<Value>,
    pub notices: Vec<String>,
}
impl LegacyArchive {
    pub fn validate(&self) -> Result<(), String> {
        if !self.original.is_object() || self.original["version"].as_u64() != Some(1) {
            return Err("Ongeldig bewaard Python-project".into());
        }
        Ok(())
    }
}

fn text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.trim().into(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}
fn number(v: Option<&Value>, field: &str) -> Result<f64, String> {
    let result = match v {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    };
    result
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("Python-project: ongeldige waarde voor {field}"))
}
fn optional_z(v: Option<&Value>) -> Result<Option<f64>, String> {
    if v.is_none_or(|v| v.is_null() || v.as_str() == Some("")) {
        Ok(None)
    } else {
        number(v, "Z").map(Some)
    }
}
fn xyz(v: &Value) -> Result<Vertex, String> {
    if let Some(a) = v.as_array() {
        Ok(Vertex {
            x: number(a.first(), "X")?,
            y: number(a.get(1), "Y")?,
            z: optional_z(a.get(2))?,
        })
    } else {
        Ok(Vertex {
            x: number(v.get("x"), "X")?,
            y: number(v.get("y"), "Y")?,
            z: optional_z(v.get("z"))?,
        })
    }
}
fn items<'a>(v: &'a Value, field: &str) -> Result<&'a [Value], String> {
    match v.get(field) {
        None => Ok(&[]),
        Some(Value::Array(a)) => Ok(a),
        _ => Err(format!("Python-project: {field} is geen lijst")),
    }
}
fn asset_path(source: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    // Windows absolute paths must also stay absolute when inspecting on Linux.
    if path.is_absolute() || value.as_bytes().get(1) == Some(&b':') || value.starts_with("\\\\") {
        path.into()
    } else {
        source.parent().unwrap_or_else(|| Path::new(".")).join(path)
    }
}

fn virtual_trench(item: &Value, source: &Path, index: usize) -> Result<VirtualTrench, String> {
    let metadata = &item["metadata"];
    let payload = metadata
        .get("virtual_trench_payload")
        .filter(|v| v.is_object())
        .ok_or("Virtuele proefsleuf mist payload")?;
    let points = items(payload, "points")?;
    if points.len() < 2 {
        return Err("Virtuele proefsleuf heeft te weinig punten".into());
    }
    let role = |p: &Value| text(p.get("role")).to_lowercase();
    let start = points.iter().position(|p| role(p) == "start").unwrap_or(0);
    let end = points
        .iter()
        .position(|p| role(p) == "end")
        .unwrap_or(points.len() - 1);
    if start == end {
        return Err("Begin- en eindpunt zijn hetzelfde projectitem".into());
    }
    let name = text(item.get("name"));
    let name = if name.is_empty() {
        format!("Proefsleuf {}", index + 1)
    } else {
        name
    };
    let width = if payload.get("width_meters").is_some() {
        number(payload.get("width_meters"), "breedte")?.max(0.1)
    } else {
        0.6
    };
    let mut trench = VirtualTrench::new(name, xyz(&points[start])?, xyz(&points[end])?, width)?;
    trench.job_id = text(metadata.get("kickthemap_job_id"))
        .parse::<u64>()
        .unwrap_or(0);
    let source_name = text(metadata.get("marxact_source_path"));
    if !source_name.is_empty() {
        trench.source_path = Some(asset_path(source, &source_name));
    }
    for (i, p) in points
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != start && *i != end)
    {
        let name = text(p.get("object_name"));
        trench.objects.push(TrenchPoint {
            role: role(p),
            object_name: if name.is_empty() {
                format!("Object {}", i + 1)
            } else {
                name
            },
            source_name: text(p.get("source_name")),
            position: xyz(p)?,
            attributes: [
                text(p.get("attribute_1")),
                text(p.get("attribute_2")),
                text(p.get("attribute_3")),
            ],
        });
    }
    if let Some(boundary) = payload
        .get("boundary_3d")
        .filter(|v| v.is_array())
        .or_else(|| metadata.get("marxact_boundary_3d"))
    {
        let points = boundary.as_array().ok_or("MarXact-grens is geen lijst")?;
        trench.boundary_3d = points.iter().map(xyz).collect::<Result<_, _>>()?;
        if !trench.boundary_3d.is_empty() && trench.boundary_3d.len() < 3 {
            return Err("MarXact-grens heeft te weinig meetpunten".into());
        }
    }
    if !trench.boundary_3d.is_empty() {
        let (auto_start, auto_end) = match (
            metadata.get("marxact_alignment_auto_start"),
            metadata.get("marxact_alignment_auto_end"),
        ) {
            (Some(a), Some(b)) => (xyz(a)?, xyz(b)?),
            (None, None) => (trench.start, trench.end),
            _ => return Err("MarXact-uitlijningsbasis is onvolledig".into()),
        };
        trench.alignment = Some(MarXactAlignment {
            auto_start,
            auto_end,
            auto_width: metadata
                .get("marxact_alignment_auto_width")
                .map(|v| number(Some(v), "automatische breedte"))
                .transpose()?
                .unwrap_or(width),
            rotation_degrees: metadata
                .get("marxact_alignment_rotation_degrees")
                .map(|v| number(Some(v), "rotatie"))
                .transpose()?
                .unwrap_or(0.0),
        });
    }
    trench.validate()?;
    Ok(trench)
}

pub fn read_legacy_project(source: &Path) -> Result<NativeProject, String> {
    let file = File::open(source).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_PROJECT_BYTES {
        return Err("Python-project is te groot".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_PROJECT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_PROJECT_BYTES {
        return Err("Python-project is te groot".into());
    }
    let original: Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("Ongeldig Python-project: {e}"))?;
    import_value(original, source)
}

pub fn import_value(original: Value, source: &Path) -> Result<NativeProject, String> {
    if original.get("version").and_then(Value::as_u64) != Some(1) || !original.is_object() {
        return Err("Alleen het Python .klicproject formaat versie 1 wordt ondersteund".into());
    }
    if original.get("format").is_some() {
        return Err("Dit bestand is geen Python .klicproject".into());
    }
    let mut project = NativeProject::default();
    let name = text(original.get("project_name"));
    if !name.is_empty() {
        project.name = name;
    }
    let mut notices=vec!["Originele laagmetadata, sjablooninstellingen, slotvolgorde en UI-instellingen zijn bewaard in het Rust-project; deze worden nog niet allemaal toegepast. Dit is nog geen volledige Python-projectpariteit.".into()];
    for (index, item) in items(&original, "tiffs")?.iter().enumerate() {
        if !item.is_object() {
            return Err(format!("Ongeldig proefsleufitem {}", index + 1));
        }
        let kind = text(item.get("type"));
        match kind.as_str() {
            "virtual_trench"=>project.virtual_trenches.push(virtual_trench(item,source,index)?),
            ""|"geotiff"=>{
                let path=text(item.get("path"));
                if path.is_empty() {return Err(format!("GeoTIFF-item {} mist een pad",index+1));}
                let path=asset_path(source,&path);
                match read_geotiff_metadata(&path) {
                    Ok(mut raster)=>{
                        let metadata=&item["metadata"];
                        if let Some(matrix)=metadata.get("base_transform_matrix") {
                            let m=matrix.as_array().filter(|m|m.len()==9).ok_or("Ongeldige basisrastermatrix")?;
                            raster.transform=GeoTransform{a:number(m.first(),"matrix a")?,b:number(m.get(1),"matrix b")?,c:number(m.get(2),"matrix c")?,
                                d:number(m.get(3),"matrix d")?,e:number(m.get(4),"matrix e")?,f:number(m.get(5),"matrix f")?};
                        }
                        let angle=metadata.get("rotation_degrees").map(|v|number(Some(v),"rasterrotatie")).transpose()?.unwrap_or(0.0);
                        raster.transform=raster.transform.rotate_degrees(angle,raster.width,raster.height)?;
                        raster.bounds=raster.transform.bounds(raster.width,raster.height)?;
                        if let Ok(id)=text(metadata.get("kickthemap_job_id")).parse::<u64>() {raster.kickthemap_job_id=(id>0).then_some(id);}
                        let title=text(metadata.get("kickthemap_job_title"));if !title.is_empty(){raster.kickthemap_job_title=title;}
                        project.rasters.push(raster);
                    }
                    Err(err)=>notices.push(format!("GeoTIFF {} niet geladen; bronverwijzing bewaard: {err}",path.display())),
                }
            }
            _=>notices.push(format!("Proefsleufitem {} heeft onbekend type {kind}; volledig bewaard in het bronarchief.",index+1)),
        }
    }
    for item in items(&original, "dxfs")? {
        let name = text(item.get("path"));
        if name.is_empty() {
            return Err("DXF-projectitem mist een pad".into());
        }
        let path = asset_path(source, &name);
        match load_dxf(&path) {
            Ok(mut overlay) => {
                overlay.visible = item.get("visible").and_then(Value::as_bool).unwrap_or(true);
                project.overlays.push(overlay);
            }
            Err(err) => notices.push(format!(
                "DXF {} niet geladen; bronverwijzing bewaard: {err}",
                path.display()
            )),
        }
    }
    for item in items(&original, "map_comments")? {
        project.comments.push(Comment {
            x: number(item.get("x"), "opmerking X")?,
            y: number(item.get("y"), "opmerking Y")?,
            text: item
                .get("text")
                .and_then(Value::as_str)
                .ok_or("Opmerkingtekst is geen tekst")?
                .to_owned(),
        });
    }
    project.legacy_archive = Some(LegacyArchive {
        source_path: source.into(),
        original: Arc::new(original),
        notices,
    });
    project.validate()?;
    Ok(project)
}
