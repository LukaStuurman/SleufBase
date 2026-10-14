//! SleufBase Rust project files and durable rotating backups.
//! The native format is intentionally distinct from legacy Python project JSON
//! until a separately verified lossless converter is implemented.

use crate::dxf_read::DxfOverlay;
use crate::geotiff::GeoTiffInfo;
use crate::model::Dataset;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::NamedTempFile;

pub const FORMAT_VERSION: u32=1;
pub const MAX_PROJECT_BYTES: u64=128*1024*1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comment {
    pub x: f64,
    pub y: f64,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeProject {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub datasets: Vec<Dataset>,
    pub rasters: Vec<GeoTiffInfo>,
    pub overlays: Vec<DxfOverlay>,
    pub comments: Vec<Comment>,
}

impl Default for NativeProject {
    fn default() -> Self {
        Self {
            format:"sleufbase-native-rust".into(),
            version:FORMAT_VERSION,
            name:"Naamloos project".into(),
            datasets:Vec::new(),
            rasters:Vec::new(),
            overlays:Vec::new(),
            comments:Vec::new(),
        }
    }
}

impl NativeProject {
    pub fn validate(&self) -> Result<(),String> {
        if self.format!="sleufbase-native-rust" {
            return Err("Geen geldig SleufBase Rust-project".into());
        }
        if self.version!=FORMAT_VERSION {
            return Err(format!("Niet ondersteunde projectversie {}", self.version));
        }
        for ds in &self.datasets {
            if ds.features.len()>2_000_000 {
                return Err("Te veel objecten in project".into());
            }
            for feature in &ds.features {
                let points:Vec<&crate::model::Vertex>=match &feature.geometry {
                    crate::model::Geometry::Point(p) => vec![p],
                    crate::model::Geometry::Polyline(points) => points.iter().collect(),
                };
                if points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite() || p.z.is_some_and(|z|!z.is_finite())) {
                    return Err("Project bevat niet-eindige coördinaten".into());
                }
            }
        }
        Ok(())
    }
}

pub fn read_project(path: &Path) -> Result<NativeProject,String> {
    let size=fs::metadata(path).map_err(|e| format!("Projectbestand niet bereikbaar: {e}"))?.len();
    if size>MAX_PROJECT_BYTES { return Err("Projectbestand is te groot".into()); }
    let file=File::open(path).map_err(|e|e.to_string())?;
    let project:NativeProject=serde_json::from_reader(file)
        .map_err(|e|format!("Projectbestand ongeldig: {e}"))?;
    project.validate()?;
    Ok(project)
}

pub fn save_project(path: &Path, project: &NativeProject) -> Result<(),String> {
    project.validate()?;
    let parent=path.parent().filter(|p|!p.as_os_str().is_empty())
        .unwrap_or_else(||Path::new("."));
    fs::create_dir_all(parent).map_err(|e|e.to_string())?;
    let mut temp=NamedTempFile::new_in(parent).map_err(|e|e.to_string())?;
    serde_json::to_writer_pretty(temp.as_file_mut(),project).map_err(|e|e.to_string())?;
    temp.as_file_mut().write_all(b"\n").map_err(|e|e.to_string())?;
    temp.as_file_mut().sync_all().map_err(|e|e.to_string())?;
    temp.persist(path).map_err(|e|format!("Project opslaan mislukt: {}",e.error))?;
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AutosaveSettings {
    pub enabled: bool,
    pub interval_minutes: u32,
    pub max_backups: usize,
}
impl Default for AutosaveSettings {
    fn default() -> Self {
        Self {enabled:true,interval_minutes:10,max_backups:20}
    }
}
impl AutosaveSettings {
    pub fn normalize(mut self) -> Self {
        self.interval_minutes=self.interval_minutes.clamp(1,1440);
        self.max_backups=self.max_backups.clamp(1,200);
        self
    }
}

pub fn app_data_directory() -> PathBuf {
    let base=std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        .or_else(||std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(||PathBuf::from("."));
    base.join("SleufBase").join("Rust")
}
pub fn read_autosave_settings(path: &Path) -> AutosaveSettings {
    let Ok(contents)=fs::read_to_string(path) else {
        return AutosaveSettings::default();
    };
    serde_json::from_str::<AutosaveSettings>(&contents)
        .unwrap_or_default().normalize()
}
pub fn write_autosave_settings(path: &Path, value: &AutosaveSettings) -> Result<(),String> {
    let settings=value.clone().normalize();
    let parent=path.parent().filter(|p|!p.as_os_str().is_empty())
        .unwrap_or_else(||Path::new("."));
    fs::create_dir_all(parent).map_err(|e|e.to_string())?;
    let mut temp=NamedTempFile::new_in(parent).map_err(|e|e.to_string())?;
    serde_json::to_writer_pretty(temp.as_file_mut(), &settings).map_err(|e|e.to_string())?;
    temp.as_file_mut().sync_all().map_err(|e|e.to_string())?;
    temp.persist(path).map_err(|e|format!("Instellingen opslaan mislukt: {}",e.error))?;
    Ok(())
}
pub fn backup_directory() -> PathBuf {
    app_data_directory().join("Backups")
}
/// Does not mutate the active project path or dirty state.
pub fn create_backup(project: &NativeProject, dir: &Path, limit: usize) -> Result<PathBuf,String> {
    fs::create_dir_all(dir).map_err(|e|e.to_string())?;
    let nonce=SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|e|e.to_string())?.as_nanos();
    let destination=dir.join(format!("autosave_{}_{}.json",std::process::id(),nonce));
    if destination.exists() {return Err("Back-upnaamconflict; probeer opnieuw".into());}
    save_project(&destination,project)?;
    prune_backups(dir,limit)?;
    Ok(destination)
}
pub fn prune_backups(dir: &Path, limit: usize) -> Result<usize,String> {
    let mut candidates:Vec<(PathBuf,SystemTime)>=Vec::new();
    if !dir.exists() {return Ok(0);}
    for entry in fs::read_dir(dir).map_err(|e|e.to_string())? {
        let entry=entry.map_err(|e|e.to_string())?;
        let path=entry.path();
        let Some(name)=path.file_name().and_then(|v|v.to_str()) else{continue};
        if !name.starts_with("autosave_") || path.extension().and_then(|x|x.to_str())!=Some("json") {
            continue;
        }
        if !path.is_file() {continue;}
        let modified=path.metadata().and_then(|m|m.modified()).unwrap_or(UNIX_EPOCH);
        candidates.push((path,modified));
    }
    candidates.sort_by(|a,b|a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    let to_remove=candidates.len().saturating_sub(limit.clamp(1,200));
    for (path,_) in candidates.into_iter().take(to_remove) {
        fs::remove_file(path).map_err(|e|format!("Back-upopruiming mislukt: {e}"))?;
    }
    Ok(to_remove)
}
/// A lightweight preview of saved metadata for compatibility diagnostics.
pub fn preview_project_metadata(path: &Path) -> Result<String,String> {
    let mut file=File::open(path).map_err(|e|e.to_string())?;
    let mut buf=Vec::new();
    (&mut file).take(MAX_PROJECT_BYTES+1)
        .read_to_end(&mut buf).map_err(|e|e.to_string())?;
    if buf.len() as u64>MAX_PROJECT_BYTES {return Err("Projectbestand is te groot".into());}
    let value:serde_json::Value=serde_json::from_slice(&buf).map_err(|e|e.to_string())?;
    let Some(object)=value.as_object() else{return Err("Project JSON is geen object".into());};
    Ok(format!("{} velden: {}",object.len(),
        object.keys().take(40).cloned().collect::<Vec<_>>().join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn save_load_overwrite_roundtrip() {
        let dir=tempdir().unwrap();
        let path=dir.path().join("Project.sleufbase-rust.json");
        let mut project=NativeProject::default();
        project.name="Proefsleuf 1".into();
        project.comments.push(Comment{x:168000.0,y:401000.0,text:"Test".into()});
        save_project(&path,&project).unwrap();
        let first=fs::read(&path).unwrap();
        save_project(&path,&project).unwrap();
        assert_eq!(first,fs::read(&path).unwrap());
        let result=read_project(&path).unwrap();
        assert_eq!(result.name,project.name);
        assert_eq!(result.comments[0].text,"Test");
    }
    #[test]
    fn backup_rotation_and_failure_safety() {
        let dir=tempdir().unwrap();
        let p=NativeProject::default();
        for _ in 0..3 { create_backup(&p,dir.path(),2).unwrap(); }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(),2);
        let previous:Vec<PathBuf>=fs::read_dir(dir.path()).unwrap().map(|e|e.unwrap().path()).collect();
        let mut bad=p.clone();
        bad.version=999;
        assert!(create_backup(&bad,dir.path(),2).is_err());
        for path in previous {assert!(path.exists());}
    }
    #[test]
    fn autosave_defaults_and_normalization() {
        assert_eq!(AutosaveSettings::default().interval_minutes,10);
        assert_eq!(AutosaveSettings::default().max_backups,20);
        assert_eq!(AutosaveSettings{enabled:true,interval_minutes:0,max_backups:9999}.normalize().max_backups,200);
        let dir=tempdir().unwrap();
        assert_eq!(read_autosave_settings(&dir.path().join("missing")),AutosaveSettings::default());
    }
}