//! SleufBase Rust project files and durable rotating backups.
//! The native format is intentionally distinct from legacy Python project JSON
//! until a separately verified lossless converter is implemented.

use crate::bgt::BgtLayer;
use crate::cadastral::CadastralLayer;
use crate::dxf_read::DxfOverlay;
use crate::geotiff::GeoTiffInfo;
use crate::model::{Bounds, Dataset, Geometry, Vertex};
use crate::storage::atomic_write;
use crate::virtual_trench::VirtualTrench;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub const FORMAT_VERSION: u32 = 4;
pub const MAX_PROJECT_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comment {
    pub x: f64,
    pub y: f64,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeProject {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub datasets: Vec<Dataset>,
    pub rasters: Vec<GeoTiffInfo>,
    pub overlays: Vec<DxfOverlay>,
    #[serde(default)]
    pub bgt_layers: Vec<BgtLayer>,
    #[serde(default)]
    pub cadastral: Option<CadastralLayer>,
    #[serde(default)]
    pub virtual_trenches: Vec<VirtualTrench>,
    pub comments: Vec<Comment>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub template_export:Option<crate::template_order::State>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub template_preferences:Option<crate::template_prefs::TemplatePreferences>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub legacy_archive:Option<crate::legacy_project::LegacyArchive>,
}

impl Default for NativeProject {
    fn default() -> Self {
        Self {
            format: "sleufbase-native-rust".into(),
            version: FORMAT_VERSION,
            name: "Naamloos project".into(),
            datasets: Vec::new(),
            rasters: Vec::new(),
            overlays: Vec::new(),
            bgt_layers: Vec::new(),
            cadastral: None,
            virtual_trenches: Vec::new(),
            comments: Vec::new(),
            template_export:None,
            template_preferences:None,
            legacy_archive:None,
        }
    }
}

impl NativeProject {
    pub fn validate(&self) -> Result<(), String> {
        if self.format != "sleufbase-native-rust" {
            return Err("Geen geldig SleufBase Rust-project".into());
        }
        if self.version != FORMAT_VERSION {
            return Err(format!("Niet ondersteunde projectversie {}", self.version));
        }
        if let Some(archive)=&self.legacy_archive {archive.validate()?;}
        if let Some(state)=&self.template_export{state.validate()?;}
        if let Some(settings)=&self.template_preferences {settings.validate()?;}
        let mut count = 0usize;
        let mut text_controls=0usize;
        let mut vertices = |points: &[Vertex]| -> Result<(), String> {
            count = count
                .checked_add(points.len())
                .ok_or("Te veel projectpunten")?;
            if count > 10_000_000 {
                return Err("Te veel projectpunten".into());
            }
            if points.iter().any(|p| {
                !p.x.is_finite() || !p.y.is_finite() || p.z.is_some_and(|z| !z.is_finite())
            }) {
                return Err("Project bevat niet-eindige coördinaten".into());
            }
            Ok(())
        };
        for trench in &self.virtual_trenches {
            trench.validate()?;
            vertices(std::slice::from_ref(&trench.start))?;
            vertices(std::slice::from_ref(&trench.end))?;
            vertices(&trench.boundary_3d)?;
            for object in &trench.objects {
                vertices(std::slice::from_ref(&object.position))?;
            }
        }
        for ds in &self.datasets {
            if ds.features.len() > 2_000_000 {
                return Err("Te veel objecten in project".into());
            }
            for feature in &ds.features {
                match &feature.geometry {
                    Geometry::Point(p) => vertices(std::slice::from_ref(p))?,
                    Geometry::Polyline(points) => vertices(points)?,
                }
            }
        }
        for overlay in &self.overlays {
            for layer in overlay.layers.values(){layer.validate()?;}
            for (key,block) in &overlay.blocks {
                if *key!=block.name.to_lowercase(){return Err("Ongeldige opgeslagen CAD-bloknaam".into());}
                crate::cad_geometry::finite(block.base)?;
                for feature in block.features.iter().flat_map(|f|std::iter::once(f).chain(f.linked_attributes())) {
                    if !feature.linked_attributes().is_empty()&&feature.entity_type!="INSERT" {return Err("Alleen INSERT mag ATTRIB bezitten".into());}
                    vertices(&feature.points)?;
                    if let Some(mesh)=feature.cad.as_deref().and_then(|p|p.mesh.as_deref()){vertices(&mesh.vertices)?;}
                    if let Some(cad)=feature.cad.as_deref(){
                        cad.validate_geometry(feature.points.len())?;
                        text_controls=text_controls.saturating_add(cad.text.as_deref().map_or(0,|t|t.contours.points.len()));
                        if text_controls>crate::cad_text_path::MAX_CONTROLS{return Err("Project overschrijdt CAD-tekstbudget".into());}
                    }
                }
            }
            for feature in overlay.features.iter().flat_map(|f|std::iter::once(f).chain(f.linked_attributes())) {
                if !feature.linked_attributes().is_empty()&&feature.entity_type!="INSERT" {return Err("Alleen INSERT mag ATTRIB bezitten".into());}
                vertices(&feature.points)?;
                if let Some(mesh)=feature.cad.as_deref().and_then(|p|p.mesh.as_deref()){vertices(&mesh.vertices)?;}
                if let Some(cad)=feature.cad.as_deref(){
                    cad.validate_geometry(feature.points.len())?;
                    text_controls=text_controls.saturating_add(cad.text.as_deref().map_or(0,|t|t.contours.points.len()));
                    if text_controls>crate::cad_text_path::MAX_CONTROLS{return Err("Project overschrijdt CAD-tekstbudget".into());}
                }
            }
        }
        for raster in &self.rasters {
            if u64::from(raster.width) * u64::from(raster.height)
                > crate::geotiff::MAX_GEOTIFF_PIXELS
            {
                return Err("Te veel rasterpixels in project".into());
            }
            raster.transform.bounds(raster.width, raster.height)?;
            raster
                .transform
                .world_to_pixel(raster.bounds.min_x, raster.bounds.min_y)?;
            validate_bounds(raster.bounds)?;
        }
        let mut paths = |paths: &[Vec<(f64, f64)>]| -> Result<(), String> {
            for path in paths {
                count = count
                    .checked_add(path.len())
                    .ok_or("Te veel projectpunten")?;
                if count > 10_000_000 {
                    return Err("Te veel projectpunten".into());
                }
                if path.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
                    return Err("Kaartlaag bevat niet-eindige coördinaten".into());
                }
            }
            Ok(())
        };
        for layer in &self.bgt_layers {
            validate_bounds(layer.bounds)?;
            paths(&layer.paths)?;
        }
        if let Some(cad) = &self.cadastral {
            validate_bounds(cad.bounds)?;
            for linework in &cad.linework {
                paths(&linework.paths)?;
            }
            if cad.labels.iter().any(|l| {
                !l.position.0.is_finite() || !l.position.1.is_finite() || !l.rotation.is_finite()
            }) {
                return Err("Kadastraal label bevat ongeldige positie of rotatie".into());
            }
        }
        if self
            .comments
            .iter()
            .any(|c| !c.x.is_finite() || !c.y.is_finite())
        {
            return Err("Opmerking bevat ongeldige coördinaten".into());
        }
        Ok(())
    }
}

fn validate_bounds(bounds: Bounds) -> Result<(), String> {
    if [bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y]
        .iter()
        .any(|v| !v.is_finite())
        || bounds.max_x <= bounds.min_x
        || bounds.max_y <= bounds.min_y
    {
        return Err("Project bevat ongeldige kaartgrenzen".into());
    }
    Ok(())
}

pub fn read_project(path: &Path) -> Result<NativeProject, String> {
    read_project_with_cancel(path,&crate::input::NEVER_CANCEL)
}
pub fn read_project_with_cancel(path:&Path,cancel:&std::sync::atomic::AtomicBool)->Result<NativeProject,String>{
    let bytes=crate::input::read(path,MAX_PROJECT_BYTES,"Projectbestand",cancel)?;
    let mut project: NativeProject =
        serde_json::from_slice(&bytes).map_err(|e| format!("Projectbestand ongeldig: {e}"))?;
    crate::input::check(cancel)?;
    // V3 stores source block/curve definitions separately from derived WCS
    // caches. V1/V2 remain readable; opening never rewrites their source bytes.
    if matches!(project.version,1..=3) {project.version=FORMAT_VERSION;}
    let mut used=0;
    for overlay in &mut project.overlays {
        crate::cad_mesh::restore_features(&mut overlay.features,&mut used,cancel)?;
        for block in overlay.blocks.values_mut(){crate::cad_mesh::restore_features(&mut Arc::make_mut(block).features,&mut used,cancel)?;}
    }
    project.validate()?;
    for overlay in &mut project.overlays {crate::input::check(cancel)?;overlay.rebuild_block_cache_with_cancel(cancel)?;}
    crate::input::check(cancel)?;
    Ok(project)
}

pub fn save_project(path: &Path, project: &NativeProject) -> Result<(), String> {
    write_project(path, project, true)
}

/// Export/conversion must never replace an existing destination, even in a race.
pub fn save_project_new(path: &Path, project: &NativeProject) -> Result<(), String> {
    write_project(path, project, false)
}

fn write_project(path: &Path, project: &NativeProject, overwrite: bool) -> Result<(), String> {
    project.validate()?;
    atomic_write(path, overwrite, |file| {
        serde_json::to_writer_pretty(&mut *file, project).map_err(|e| e.to_string())?;
        file.write_all(b"\n").map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > MAX_PROJECT_BYTES {
            return Err("Projectbestand is te groot om veilig terug te lezen".into());
        }
        Ok(())
    })
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AutosaveSettings {
    pub enabled: bool,
    pub interval_minutes: u32,
    pub max_backups: usize,
}
impl Default for AutosaveSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_minutes: 10,
            max_backups: 20,
        }
    }
}
impl AutosaveSettings {
    pub fn normalize(mut self) -> Self {
        self.interval_minutes = self.interval_minutes.clamp(1, 1440);
        self.max_backups = self.max_backups.clamp(1, 200);
        self
    }
}

pub fn app_data_directory() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("SleufBase").join("Rust")
}
pub fn read_autosave_settings(path: &Path) -> AutosaveSettings {
    let Ok(contents) = fs::read_to_string(path) else {
        return AutosaveSettings::default();
    };
    serde_json::from_str::<AutosaveSettings>(&contents)
        .unwrap_or_default()
        .normalize()
}
pub fn write_autosave_settings(path: &Path, value: &AutosaveSettings) -> Result<(), String> {
    let settings = value.clone().normalize();
    atomic_write(path, true, |file| {
        serde_json::to_writer_pretty(file, &settings).map_err(|e| e.to_string())
    })
}
pub fn backup_directory() -> PathBuf {
    app_data_directory().join("Backups")
}
/// Does not mutate the active project path or dirty state.
pub fn create_backup(project: &NativeProject, dir: &Path, limit: usize) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let destination = dir.join(format!("autosave_{}_{}.json", std::process::id(), nonce));
    if destination.exists() {
        return Err("Back-upnaamconflict; probeer opnieuw".into());
    }
    save_project_new(&destination, project)?;
    prune_backups(dir, limit)?;
    Ok(destination)
}
pub fn prune_backups(dir: &Path, limit: usize) -> Result<usize, String> {
    let mut candidates: Vec<(PathBuf, SystemTime)> = Vec::new();
    if !dir.exists() {
        return Ok(0);
    }
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
            continue;
        };
        if !name.starts_with("autosave_")
            || path.extension().and_then(|x| x.to_str()) != Some("json")
        {
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let modified = path
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(UNIX_EPOCH);
        candidates.push((path, modified));
    }
    candidates.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    let to_remove = candidates.len().saturating_sub(limit.clamp(1, 200));
    for (path, _) in candidates.into_iter().take(to_remove) {
        fs::remove_file(path).map_err(|e| format!("Back-upopruiming mislukt: {e}"))?;
    }
    Ok(to_remove)
}
/// A lightweight preview of saved metadata for compatibility diagnostics.
pub fn preview_project_metadata(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    (&mut file)
        .take(MAX_PROJECT_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    if buf.len() as u64 > MAX_PROJECT_BYTES {
        return Err("Projectbestand is te groot".into());
    }
    let value: serde_json::Value = serde_json::from_slice(&buf).map_err(|e| e.to_string())?;
    let Some(object) = value.as_object() else {
        return Err("Project JSON is geen object".into());
    };
    Ok(format!(
        "{} velden: {}",
        object.len(),
        object
            .keys()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn save_load_overwrite_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Project.sleufbase-rust.json");
        let mut project = NativeProject::default();
        project.name = "Proefsleuf 1".into();
        project.comments.push(Comment {
            x: 168000.0,
            y: 401000.0,
            text: "Test".into(),
        });
        save_project(&path, &project).unwrap();
        let first = fs::read(&path).unwrap();
        save_project(&path, &project).unwrap();
        assert_eq!(first, fs::read(&path).unwrap());
        let result = read_project(&path).unwrap();
        assert_eq!(result.name, project.name);
        assert_eq!(result.comments[0].text, "Test");
    }
    #[test]
    fn backup_rotation_and_failure_safety() {
        let dir = tempdir().unwrap();
        let p = NativeProject::default();
        for _ in 0..3 {
            create_backup(&p, dir.path(), 2).unwrap();
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        let previous: Vec<PathBuf> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        let mut bad = p.clone();
        bad.version = 999;
        assert!(create_backup(&bad, dir.path(), 2).is_err());
        for path in previous {
            assert!(path.exists());
        }
    }
    #[test]
    fn autosave_defaults_and_normalization() {
        assert_eq!(AutosaveSettings::default().interval_minutes, 10);
        assert_eq!(AutosaveSettings::default().max_backups, 20);
        assert_eq!(
            AutosaveSettings {
                enabled: true,
                interval_minutes: 0,
                max_backups: 9999
            }
            .normalize()
            .max_backups,
            200
        );
        let dir = tempdir().unwrap();
        assert_eq!(
            read_autosave_settings(&dir.path().join("missing")),
            AutosaveSettings::default()
        );
    }
}
