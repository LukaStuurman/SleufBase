//! Explicit Python v1 exchange. Publication always creates a new file; an
//! ambiguous association with unknown point fields is rejected before writing.
use super::*;
use serde_json::{json,Map};
use std::{collections::HashSet,io::Write,sync::atomic::AtomicBool};
const ORIGIN:&str="_sleufbase_python_tiff_index";

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(default,deny_unknown_fields)]
pub struct ProjectUiState {
    pub center_x:f64,pub center_y:f64,pub meters_per_pixel:f64,
    pub opacity_percent:f64,pub use_background:bool,pub background_source:String,
    pub selected_tiff_index:Option<usize>,pub selected_dxf_index:Option<usize>,
}
impl Default for ProjectUiState {
    fn default()->Self{Self{center_x:155000.,center_y:463000.,meters_per_pixel:1.,opacity_percent:80.,use_background:true,background_source:"PDOK luchtfoto".into(),selected_tiff_index:None,selected_dxf_index:None}}
}
impl ProjectUiState {
    pub fn from_archive(project:&NativeProject)->Result<Option<Self>,String>{
        project.legacy_archive.as_ref().and_then(|a|a.original.get("ui")).map(|v|{
            let known=serde_json::to_value(Self::default()).map_err(|e|e.to_string())?;
            let fields=v.as_object().ok_or("Python-kaartvoorkeuren zijn geen object")?.iter().filter(|(k,_)|known.get(k.as_str()).is_some()).map(|(k,v)|(k.clone(),v.clone())).collect::<Map<_,_>>();
            let out:Self=serde_json::from_value(fields.into()).map_err(|e|format!("Python-kaartvoorkeuren: {e}"))?;out.validate()?;Ok(out)
        }).transpose()
    }
    pub fn from_project(project:&NativeProject)->Result<Option<Self>,String>{
        if let Some(view)=&project.view{return view.for_export(project).map(Some);}
        project.legacy_archive.as_ref().and_then(|a|a.original.get("ui")).map(|v|{
            let known=serde_json::to_value(Self::default()).map_err(|e|e.to_string())?;
            let fields=v.as_object().ok_or("Python-kaartvoorkeuren zijn geen object")?.iter().filter(|(k,_)|known.get(k.as_str()).is_some()).map(|(k,v)|(k.clone(),v.clone())).collect::<Map<_,_>>();
            let mut out:Self=serde_json::from_value(fields.into()).map_err(|e|format!("Python-kaartvoorkeuren: {e}"))?;
            out.validate()?;
            // Native lists group virtual trenches before rasters. Archive UI
            // indices refer to the original interleaved order instead.
            let archive=project.legacy_archive.as_ref().unwrap();
            let exported=legacy_payload(project,&out)?;
            out.selected_tiff_index=out.selected_tiff_index.and_then(|index|{
                let old=archive.original["tiffs"].get(index)?;
                if old["type"]=="virtual_trench"{
                    return project.virtual_trenches.iter().position(|trench|trench.metadata.get(ORIGIN).and_then(Value::as_u64)==Some(index as u64));
                }
                let path=old["path"].as_str().map(|p|asset_path(&archive.source_path,p));
                let hits=exported["tiffs"].as_array()?.iter().enumerate().filter(|(_,item)|match &path{
                    Some(path)=>item["path"].as_str().is_some_and(|p|Path::new(p)==path),
                    None=>*item==old,
                }).map(|(i,_)|i).collect::<Vec<_>>();
                (hits.len()==1).then(||hits[0])
            });
            out.selected_dxf_index=out.selected_dxf_index.and_then(|index|{
                let path=asset_path(&archive.source_path,archive.original["dxfs"].get(index)?["path"].as_str()?);
                let hits=exported["dxfs"].as_array()?.iter().enumerate().filter(|(_,item)|item["path"].as_str().is_some_and(|p|Path::new(p)==path)).map(|(i,_)|i).collect::<Vec<_>>();
                (hits.len()==1).then(||hits[0])
            });
            Ok(out)
        }).transpose()
    }
    pub fn validate(&self)->Result<(),String>{
        if !self.center_x.is_finite()||!self.center_y.is_finite()||self.center_x.abs()>1e12||self.center_y.abs()>1e12||!self.meters_per_pixel.is_finite()||!(1e-8..=1e9).contains(&self.meters_per_pixel)||!self.opacity_percent.is_finite()||!(0. ..=100.).contains(&self.opacity_percent)||self.background_source.len()>256 {
            return Err("Ongeldige kaartvoorkeuren".into());
        }Ok(())
    }
}
fn path_value(path:&Path)->Value {Value::String(path.to_string_lossy().into_owned())}
fn vertex(point:Vertex)->Value{json!([point.x,point.y,point.z])}
fn point(mut out:Map<String,Value>,position:Vertex,role:&str)->Value {
    out.insert("role".into(),role.into());out.insert("x".into(),json!(position.x));out.insert("y".into(),json!(position.y));out.insert("z".into(),json!(position.z));Value::Object(out)
}
fn object(v:&Value)->Map<String,Value>{v.as_object().cloned().unwrap_or_default()}
fn extras(point:&Value)->bool{point.as_object().is_some_and(|m|m.keys().any(|k|!matches!(k.as_str(),"role"|"x"|"y"|"z"|"object_name"|"source_name"|"attribute_1"|"attribute_2"|"attribute_3")))}
fn export_trench(trench:&VirtualTrench,old:Option<&Value>)->Result<Value,String>{
    let mut entry=old.map(object).unwrap_or_default();
    let mut metadata=old.map(|v|object(&v["metadata"])).unwrap_or_default();
    if let Some(active)=trench.metadata.as_object(){metadata.extend(active.iter().filter(|(k,_)|k.as_str()!=ORIGIN).map(|(k,v)|(k.clone(),v.clone())));}
    // Removed active references/notes must remain removed, even if the archive
    // still contains their original values.
    for key in [crate::raster_points::REFERENCE,"template_map_comments"]{if trench.metadata.get(key).is_none(){metadata.remove(key);}}
    let mut payload=object(metadata.get("virtual_trench_payload").unwrap_or(&Value::Null));
    let original_bands=metadata.get("template_dekband_lines").and_then(Value::as_array).cloned().unwrap_or_default();
    let normalized=dekband_rows(&Value::Object(metadata.clone()))?.into_iter().map(|v|serde_json::to_value(v).map_err(|e|e.to_string())).collect::<Result<Vec<_>,_>>()?;
    let band_extras=|v:&Value|v.as_object().is_some_and(|m|m.keys().any(|k|!matches!(k.as_str(),"start_chainage"|"end_chainage"|"start_depth"|"end_depth"|"source_name"|"display_rgb"|"label")));
    let mut consumed=HashSet::new();let mut bands=Vec::new();
    for band in &trench.dekband_rows{
        let active=serde_json::to_value(band).map_err(|e|e.to_string())?;
        let hits=normalized.iter().enumerate().filter(|(i,v)|!consumed.contains(i)&&**v==active).map(|(i,_)|i).collect::<Vec<_>>();
        if hits.len()>1&&hits.iter().any(|i|band_extras(&original_bands[*i])){return Err("Dubbelzinnige extra dekbandmetadata; Python-export afgebroken".into());}
        let mut raw=hits.first().map(|i|{consumed.insert(*i);object(&original_bands[*i])}).unwrap_or_default();raw.extend(object(&active));bands.push(Value::Object(raw));
    }
    if original_bands.iter().enumerate().any(|(i,v)|!consumed.contains(&i)&&band_extras(v)){return Err("Gewijzigde dekband met onbekende metadata kan niet verliesvrij naar Python worden geëxporteerd".into());}
    if bands.is_empty(){metadata.remove("template_dekband_lines");}else{metadata.insert("template_dekband_lines".into(),bands.into());}
    let originals=payload.get("points").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut used=HashSet::new();
    let find=|role:&str,name:Option<(&str,&str)>,used:&mut HashSet<usize>|->Result<Map<String,Value>,String>{
        let hits=originals.iter().enumerate().filter(|(i,p)|!used.contains(i)&&p["role"].as_str()==Some(role)&&name.is_none_or(|(n,s)|p["object_name"].as_str().unwrap_or("")==n&&p["source_name"].as_str().unwrap_or("")==s)).map(|(i,_)|i).collect::<Vec<_>>();
        if hits.len()==1{used.insert(hits[0]);Ok(object(&originals[hits[0]]))}else if hits.iter().any(|i|extras(&originals[*i])){Err("Dubbelzinnige oorspronkelijke objectmetadata; Python-export afgebroken zonder gegevensverlies".into())}else{Ok(Map::new())}
    };
    let start=if trench.start_metadata.is_object(){let _=find("start",None,&mut used);object(&trench.start_metadata)}else{find("start",None,&mut used)?};
    let mut points=vec![point(start,trench.start,"start")];
    for p in &trench.objects {
        let mut raw=if p.metadata.is_object(){
            // New imports keep source context with the point itself, including
            // user reordering/renaming. Consume its archived equivalent only
            // for old-preview ambiguity detection below.
            let _=find(&p.role,Some((&p.object_name,&p.source_name)),&mut used);
            object(&p.metadata)
        }else{find(&p.role,Some((&p.object_name,&p.source_name)),&mut used)?};
        raw.insert("object_name".into(),p.object_name.clone().into());raw.insert("source_name".into(),p.source_name.clone().into());
        for (key,value) in ["attribute_1","attribute_2","attribute_3"].into_iter().zip(&p.attributes){raw.insert(key.into(),value.clone().into());}
        points.push(point(raw,p.position,&p.role));
    }
    let end=if trench.end_metadata.is_object(){let _=find("end",None,&mut used);object(&trench.end_metadata)}else{find("end",None,&mut used)?};
    points.push(point(end,trench.end,"end"));
    if trench.objects.iter().any(|p|!p.metadata.is_object())&&originals.iter().enumerate().any(|(i,p)|!used.contains(&i)&&extras(p)) {
        return Err("Een oorspronkelijk object met extra metadata is verwijderd of hernoemd; Python-export kan die koppeling niet verliesvrij bepalen".into());
    }
    payload.insert("width_meters".into(),json!(trench.width_meters));payload.insert("points".into(),points.into());
    if !trench.boundary_3d.is_empty(){let boundary:Value=trench.boundary_3d.iter().copied().map(vertex).collect::<Vec<_>>().into();payload.insert("boundary_3d".into(),boundary.clone());metadata.insert("marxact_boundary_3d".into(),boundary);}else{payload.remove("boundary_3d");metadata.remove("marxact_boundary_3d");}
    metadata.insert("virtual_trench_payload".into(),payload.into());
    if let Some(a)=&trench.alignment{metadata.insert("marxact_alignment_auto_start".into(),vertex(a.auto_start));metadata.insert("marxact_alignment_auto_end".into(),vertex(a.auto_end));metadata.insert("marxact_alignment_auto_width".into(),json!(a.auto_width));metadata.insert("marxact_alignment_rotation_degrees".into(),json!(a.rotation_degrees));}else{
        for key in ["marxact_alignment_auto_start","marxact_alignment_auto_end","marxact_alignment_auto_width","marxact_alignment_rotation_degrees"]{metadata.remove(key);}
    }
    metadata.insert("kickthemap_job_id".into(),json!(trench.job_id));
    if let Some(path)=&trench.source_path{metadata.insert("marxact_source_path".into(),path_value(path));}else{metadata.remove("marxact_source_path");}
    entry.insert("type".into(),"virtual_trench".into());entry.insert("name".into(),trench.label.clone().into());entry.insert("path".into(),Value::Null);entry.insert("metadata".into(),metadata.into());Ok(entry.into())
}
pub fn legacy_payload(project:&NativeProject,ui:&ProjectUiState)->Result<Value,String>{
    legacy_payload_with_cancel(project,ui,&crate::input::NEVER_CANCEL)
}
fn legacy_payload_with_cancel(project:&NativeProject,ui:&ProjectUiState,cancel:&AtomicBool)->Result<Value,String>{
    crate::input::check(cancel)?;
    project.validate()?;ui.validate()?;
    if project.template_preferences.is_some()||project.template_export.is_some()||project.project_settings.is_some(){return Err("Python-projectexport ondersteunt de gewijzigde Rust-sjabloonvoorkeuren en slotvolgorde nog niet verliesvrij; bewaar dit project als Rust-project".into());}
    if !project.datasets.is_empty()||!project.bgt_layers.is_empty()||project.cadastral.is_some(){return Err("Python-projectexport ondersteunt nog geen losse jobdatasets/BGT/Kadasterlagen; sla deze gegevens eerst in het Rust-project op".into());}
    let archive=project.legacy_archive.as_ref();
    let mut output=archive.map(|a|object(&a.original)).unwrap_or_default();
    let original=archive.and_then(|a|a.original["tiffs"].as_array());
    let mut used=HashSet::new();let mut tiffs=Vec::new();
    for trench in &project.virtual_trenches {
        crate::input::check(cancel)?;
        let old=if let Some(index)=trench.metadata.get(ORIGIN).and_then(Value::as_u64){
            let index=usize::try_from(index).map_err(|_|"Ongeldige Python-laagherkomst")?;
            if !used.insert(index){return Err("Twee sleuven verwijzen naar dezelfde oorspronkelijke laag; Python-export is dubbelzinnig".into());}
            Some(original.and_then(|a|a.get(index)).filter(|v|v["type"]=="virtual_trench").ok_or("Oorspronkelijke sleuf ontbreekt uit het bronarchief")?)
        }else{None};
        tiffs.push(export_trench(trench,old)?);
    }
    let source=archive.map(|a|a.source_path.as_path()).unwrap_or(Path::new("."));
    for raster in &project.rasters {
        crate::input::check(cancel)?;
        // The original Python loader calls set_geotiff_layer_rotation only
        // for a nonzero saved angle; at zero it ignores base_transform_matrix.
        // A changed native affine then needs a new georeferenced TIFF asset.
        if crate::geotiff_rotation::angle(raster).abs()<1e-9{
            let disk=read_geotiff_metadata(&raster.path).map_err(|e|format!("Python-export kan rasterbron {} niet controleren: {e}",raster.path.display()))?;
            let a=raster.transform;let b=disk.transform;
            if [a.a-b.a,a.b-b.b,a.c-b.c,a.d-b.d,a.e-b.e,a.f-b.f].iter().any(|v|v.abs()>1e-8){return Err("Python-projectexport kan een gewijzigde rastertransformatie zonder rotatie niet verliesvrij opslaan; exporteer eerst een nieuw gegeorefereerd TIFF-bestand".into());}
        }
        crate::input::check(cancel)?;
        let old=original.into_iter().flatten().enumerate().find(|(_,v)|v["path"].as_str().is_some_and(|p|asset_path(source,p)==raster.path));
        let mut entry=old.map(|(_,v)|object(v)).unwrap_or_default();if let Some((i,_))=old{used.insert(i);}
        let mut metadata=object(&raster.metadata);metadata.remove(ORIGIN);
        metadata.insert("base_transform_matrix".into(),crate::geotiff_rotation::matrix(raster.transform));
        // Native affine is already the displayed transform; the saved original
        // basis/angle, when present, are what Python must rotate exactly once.
        if let Some(base)=raster.metadata.get(crate::geotiff_rotation::BASE){metadata.insert("base_transform_matrix".into(),base.clone());}
        entry.insert("type".into(),"geotiff".into());entry.insert("name".into(),raster.path.file_stem().unwrap_or_default().to_string_lossy().into_owned().into());entry.insert("path".into(),path_value(&raster.path));entry.insert("metadata".into(),metadata.into());tiffs.push(entry.into());
    }
    // Missing assets and unknown layer kinds were not materialized by import;
    // preserve them with absolute links when exporting beside another folder.
    for (i,old) in original.into_iter().flatten().enumerate(){if !used.contains(&i)&&old["type"]!="virtual_trench" {
        crate::input::check(cancel)?;
        let retain=old["path"].as_str().is_none_or(|p|{
            let path=asset_path(source,p);
            !path.exists()||read_geotiff_metadata(&path).is_err()
        })||!matches!(old["type"].as_str(),Some("geotiff")|Some(""));
        crate::input::check(cancel)?;
        if retain{let mut old=old.clone();if let Some(p)=old["path"].as_str(){old["path"]=path_value(&asset_path(source,p));}tiffs.push(old);}
    }}
    let mut dxfs=project.overlays.iter().map(|o|{
        let old=archive.and_then(|a|a.original["dxfs"].as_array()).into_iter().flatten().find(|v|v["path"].as_str().is_some_and(|p|asset_path(source,p)==o.path));
        let mut raw=old.map(object).unwrap_or_default();raw.insert("path".into(),path_value(&o.path));raw.insert("visible".into(),o.visible.into());Value::Object(raw)
    }).collect::<Vec<_>>();
    if let Some(a)=archive{for old in items(&a.original,"dxfs")?{crate::input::check(cancel)?;if let Some(p)=old["path"].as_str(){let path=asset_path(source,p);if !project.overlays.iter().any(|o|o.path==path)&&(!path.exists()||crate::dxf_read::load_dxf_with_cancel(&path,cancel).is_err()){crate::input::check(cancel)?;let mut old=old.clone();old["path"]=path_value(&path);dxfs.push(old);}}}}
    let mut ui_raw=object(output.get("ui").unwrap_or(&Value::Null));ui_raw.extend(object(&serde_json::to_value(ui).map_err(|e|e.to_string())?));
    output.insert("version".into(),1.into());output.insert("project_name".into(),project.name.clone().into());output.insert("ui".into(),ui_raw.into());output.insert("tiffs".into(),tiffs.into());output.insert("dxfs".into(),dxfs.into());output.insert("map_comments".into(),serde_json::to_value(&project.comments).map_err(|e|e.to_string())?);
    crate::input::check(cancel)?;Ok(output.into())
}
pub fn write_legacy_project(project:&NativeProject,path:&Path,ui:&ProjectUiState,cancel:&AtomicBool)->Result<(),String>{
    write_legacy_project_checked(project,path,ui,cancel,||crate::input::check(cancel))
}
pub fn write_legacy_project_controlled(project:&NativeProject,path:&Path,ui:&ProjectUiState,control:&crate::export_control::Control)->Result<(),String>{
    write_legacy_project_checked(project,path,ui,control.flag(),||control.begin_publish())
}
fn write_legacy_project_checked(project:&NativeProject,path:&Path,ui:&ProjectUiState,cancel:&AtomicBool,before_publish:impl FnOnce()->Result<(),String>)->Result<(),String>{
    crate::input::check(cancel)?;
    if path.extension().and_then(|e|e.to_str()).is_none_or(|e|!e.eq_ignore_ascii_case("klicproject")){return Err("Kies een nieuw .klicproject-bestand voor Python-export".into());}
    let payload=legacy_payload_with_cancel(project,ui,cancel)?;
    crate::storage::atomic_write_checked(path,false,|file|{
        let mut writer=ProjectWriter{out:std::io::BufWriter::new(file),cancel,written:0};
        serde_json::to_writer_pretty(&mut writer,&payload).map_err(|e|e.to_string())?;
        writer.write_all(b"\n").and_then(|_|writer.flush()).map_err(|e|e.to_string())
    },before_publish)
}
struct ProjectWriter<'a,W>{out:W,cancel:&'a AtomicBool,written:u64}
impl<W:Write> Write for ProjectWriter<'_,W>{
    fn write(&mut self,bytes:&[u8])->std::io::Result<usize>{
        crate::input::check(self.cancel).map_err(std::io::Error::other)?;
        if (bytes.len() as u64)>MAX_PROJECT_BYTES.saturating_sub(self.written){return Err(std::io::Error::other("Python-project is groter dan 128 MiB"));}
        let written=self.out.write(bytes)?;self.written+=written as u64;Ok(written)
    }
    fn flush(&mut self)->std::io::Result<()>{crate::input::check(self.cancel).map_err(std::io::Error::other)?;self.out.flush()}
}

#[cfg(test)]
mod writer_tests {
    use super::*;
    #[test]
    fn json_budget_includes_newline_and_cancel_stops_serialization(){
        let cancel=AtomicBool::new(false);let mut writer=ProjectWriter{out:Vec::new(),cancel:&cancel,written:MAX_PROJECT_BYTES-1};
        writer.write_all(b"x").unwrap();assert!(writer.write_all(b"\n").is_err());
        let cancel=AtomicBool::new(true);let mut writer=ProjectWriter{out:Vec::new(),cancel:&cancel,written:0};
        assert!(serde_json::to_writer_pretty(&mut writer,&json!({"data":[1,2,3]})).is_err());assert!(writer.out.is_empty());
    }
}
