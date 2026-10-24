//! Explicit Python .klicproject v1 import. The original JSON is retained intact
//! in native saves, including fields not yet editable/rendered by the preview.
//! This does not claim full workflow parity or a Python-format exporter.
use crate::{
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
    path::{Path, PathBuf},
    sync::Arc,
};
#[path="legacy_exchange.rs"]
mod exchange;
pub use exchange::{ProjectUiState,legacy_payload,write_legacy_project,write_legacy_project_controlled};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
pub(crate) fn asset_path(source: &Path, value: &str) -> PathBuf {
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
    trench.dekband_rows=dekband_rows(metadata)?;
    trench.start_metadata=points[start].clone();
    trench.end_metadata=points[end].clone();
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
            metadata:p.clone(),
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
    if let Some(metadata)=metadata.as_object() {
        let mut active=metadata.clone();
        // Geometry already lives in typed source vectors; the complete original
        // payload (including unknown fields) remains in the import archive.
        active.remove("virtual_trench_payload");
        active.remove("marxact_boundary_3d");
        active.insert("_sleufbase_python_tiff_index".into(),serde_json::json!(index));
        trench.metadata=Value::Object(active);
    }
    trench.validate()?;
    Ok(trench)
}

fn dekband_rows(metadata:&Value)->Result<Vec<crate::virtual_trench::Dekband>,String>{
    items(metadata,"template_dekband_lines")?.iter().map(|row|{
        let mut start=number(row.get("start_chainage"),"dekband begin")?;
        let mut end=number(row.get("end_chainage"),"dekband einde")?;
        let mut start_depth=row.get("start_depth").map(|v|number(Some(v),"dekband begindiepte")).transpose()?.unwrap_or(0.).max(0.);
        let mut end_depth=row.get("end_depth").map(|v|number(Some(v),"dekband einddiepte")).transpose()?.unwrap_or(start_depth).max(0.);
        if end<start{std::mem::swap(&mut start,&mut end);std::mem::swap(&mut start_depth,&mut end_depth);}
        let mut source_name=text(row.get("source_name"));if source_name.is_empty(){source_name=text(row.get("label"));}if source_name.is_empty(){source_name="Dekband".into();}
        let display_rgb=row.get("display_rgb").filter(|v|!v.is_null()).map(|v|serde_json::from_value(v.clone()).map_err(|e|format!("Dekbandkleur: {e}"))).transpose()?;
        Ok(crate::virtual_trench::Dekband{start_chainage:start,end_chainage:end,start_depth,end_depth,source_name,display_rgb})
    }).collect()
}

pub fn read_legacy_project(source: &Path) -> Result<NativeProject, String> {
    read_legacy_project_with_cancel(source,&crate::input::NEVER_CANCEL)
}
pub fn read_legacy_project_with_cancel(source:&Path,cancel:&std::sync::atomic::AtomicBool)->Result<NativeProject,String>{
    let bytes=crate::input::read(source,MAX_PROJECT_BYTES,"Python-project",cancel)?;
    let original: Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("Ongeldig Python-project: {e}"))?;
    import_value_with_cancel(original, source,cancel)
}

pub fn import_value(original: Value, source: &Path) -> Result<NativeProject, String> {
    import_value_with_cancel(original,source,&crate::input::NEVER_CANCEL)
}
fn import_value_with_cancel(original:Value,source:&Path,cancel:&std::sync::atomic::AtomicBool)->Result<NativeProject,String>{
    crate::input::check(cancel)?;
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
        crate::input::check(cancel)?;
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
                        raster.metadata=metadata.clone();
                        crate::raster_points::validate_metadata(&raster.metadata)?;
                        if let Some(path)=raster.metadata.get("kickthemap_job_features_path").and_then(Value::as_str).filter(|p|!p.is_empty()){
                            let resolved=asset_path(source,path);
                            raster.metadata["kickthemap_job_features_path"]=Value::String(resolved.to_string_lossy().into_owned());
                        }
                        let angle=metadata.get("rotation_degrees").map(|v|number(Some(v),"rasterrotatie")).transpose()?.unwrap_or(0.0);
                        if angle.abs()>=1e-9 {
                            if let Some(matrix)=metadata.get("base_transform_matrix") {
                                let m=matrix.as_array().filter(|m|m.len()==9).ok_or("Ongeldige basisrastermatrix")?;
                                raster.transform=GeoTransform{a:number(m.first(),"matrix a")?,b:number(m.get(1),"matrix b")?,c:number(m.get(2),"matrix c")?,
                                    d:number(m.get(3),"matrix d")?,e:number(m.get(4),"matrix e")?,f:number(m.get(5),"matrix f")?};
                            }
                            if raster.metadata.get(crate::geotiff_rotation::BASE).is_none(){raster.metadata[crate::geotiff_rotation::BASE]=crate::geotiff_rotation::matrix(raster.transform);}
                            let angle=crate::geotiff_rotation::normalize(angle)?;
                            raster.metadata[crate::geotiff_rotation::ANGLE]=serde_json::json!(angle);
                            if angle!=0.{raster.transform=raster.transform.rotate_degrees(angle,raster.width,raster.height)?;}
                        }
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
        crate::input::check(cancel)?;
        let name = text(item.get("path"));
        if name.is_empty() {
            return Err("DXF-projectitem mist een pad".into());
        }
        let path = asset_path(source, &name);
        match crate::dxf_read::load_dxf_with_cancel(&path,cancel) {
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
    project.view=crate::project_view::ProjectView::from_legacy(&project)?;
    project.validate()?;
    crate::input::check(cancel)?;
    Ok(project)
}
