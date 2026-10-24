//! Standalone A4 PNG batches. Render everything before replacing any old output.
use crate::{
    export_control::{CheckedWriter, Control, Stage},
    export_worker::Inputs,
    map_export::{self, Background, Renderer},
    map_page, map_text, template_raster,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub dpi: u32,
    pub scale: f64,
    pub include_cad: bool,
    /// Supported native standalone map backgrounds.
    #[serde(deserialize_with = "read_background")]
    pub background: String,
}
fn read_background<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    // Removed from the Rust product at the user's request. Reading an older
    // configuration is still possible without reviving the removed provider.
    Ok(if value == "cyclomedia_aerial" {
        "pdok".into()
    } else {
        value
    })
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            dpi: 300,
            scale: 225.,
            include_cad: false,
            background: "pdok".into(),
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        map_page::layout(
            crate::Bounds {
                min_x: 155000.,
                min_y: 463000.,
                max_x: 155001.,
                max_y: 463001.,
            },
            self.page(),
        )?;
        if !matches!(
            self.background.as_str(),
            "pdok" | "osm" | "cadastral" | "white"
        ) {
            return Err("Onbekende kaartachtergrond".into());
        }
        Ok(())
    }
    fn page(&self) -> map_page::Settings {
        map_page::Settings {
            dpi: self.dpi,
            scale: self.scale,
        }
    }
}
/// Stable, Windows-case-insensitive names; duplicates never overwrite each other.
pub fn filenames(paths: impl IntoIterator<Item = impl AsRef<Path>>) -> Vec<String> {
    let mut used = HashSet::new();
    paths
        .into_iter()
        .map(|path| {
            let stem = path
                .as_ref()
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy();
            let mut stem = stem
                .chars()
                .take(90)
                .map(|c| {
                    if c.is_control() || "<>:\"/\\|?*".contains(c) {
                        '_'
                    } else {
                        c
                    }
                })
                .collect::<String>();
            stem = stem.trim_matches([' ', '.']).to_owned();
            if stem.is_empty() {
                stem = "kaart".into();
            }
            let mut n = 1usize;
            loop {
                let name = if n == 1 {
                    format!("{stem}_kaart.png")
                } else {
                    format!("{stem}_kaart_{n}.png")
                };
                if used.insert(crate::python_values::uppercase(&name)) {
                    break name;
                }
                n += 1;
            }
        })
        .collect()
}
fn sources(inputs: &Inputs) -> Vec<&Path> {
    let mut paths = inputs
        .protected_paths
        .iter()
        .map(PathBuf::as_path)
        .collect::<Vec<_>>();
    paths.extend(inputs.datasets.iter().map(|d| d.source_path.as_path()));
    paths.extend(inputs.overlays.iter().map(|d| d.path.as_path()));
    paths.extend(
        inputs
            .trenches
            .iter()
            .filter_map(|d| d.source_path.as_deref()),
    );
    for raster in &inputs.rasters {
        paths.push(&raster.path);
        paths.extend(raster.kickthemap_job_sidecar.as_deref());
    }
    paths
}
fn check_targets(targets: &[PathBuf], protected: &[&Path]) -> Result<(), String> {
    let protected = protected
        .iter()
        .filter_map(|p| fs::canonicalize(p).ok())
        .collect::<HashSet<_>>();
    for target in targets {
        match fs::symlink_metadata(target) {
            Ok(meta) => {
                if !meta.file_type().is_file() {
                    return Err(format!(
                        "Kaartuitvoer is geen gewoon bestand: {}",
                        target.display()
                    ));
                }
                let real = fs::canonicalize(target).map_err(|e| e.to_string())?;
                if protected.contains(&real) {
                    return Err("Kaartuitvoer mag geen geladen projectbron vervangen".into());
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}
/// PNG stores physical resolution in pixels per metre. Encode RGB one row at a
/// time, avoiding a second complete RGB page allocation alongside the RGBA page.
fn write_png(
    path: &Path,
    page: &map_export::Page,
    dpi: u32,
    control: &Control,
) -> Result<(), String> {
    let mut file = File::create(path).map_err(|e| e.to_string())?;
    {
        let out = CheckedWriter {
            out: &mut file,
            control,
        };
        let mut encoder = png::Encoder::new(out, page.image.width(), page.image.height());
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let ppm = (f64::from(dpi) / 0.0254).round_ties_even() as u32;
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu: ppm,
            yppu: ppm,
            unit: png::Unit::Meter,
        }));
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        {
            let mut stream = writer.stream_writer().map_err(|e| e.to_string())?;
            let mut row = Vec::with_capacity(page.image.width() as usize * 3);
            for pixels in page.image.rows() {
                control.check()?;
                row.clear();
                for pixel in pixels {
                    row.extend_from_slice(&pixel.0[..3]);
                }
                stream.write_all(&row).map_err(|e| e.to_string())?;
            }
            stream.finish().map_err(|e| e.to_string())?;
        }
        writer.finish().map_err(|e| e.to_string())?;
    }
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(())
}
/// No output is replaced until all pages are rendered, encoded and flushed.
/// Publication is a sequence of renames, with rollback on ordinary I/O failures;
/// this does not promise crash-atomic replacement of multiple independent files.
fn publish(
    stage: tempfile::TempDir,
    targets: &[PathBuf],
    control: &Control,
    mut rename: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<(), String> {
    let mut manifest =
        File::create(stage.path().join("destinations.json")).map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut manifest, targets).map_err(|e| e.to_string())?;
    manifest.sync_all().map_err(|e| e.to_string())?;
    drop(manifest);
    control.begin_publish()?;
    let mut changes = Vec::new();
    let result: Result<(), String> = (|| {
        for (i, target) in targets.iter().enumerate() {
            let backup = stage.path().join(format!("old-{i}"));
            let existed = target.try_exists().map_err(|e| e.to_string())?;
            if existed {
                rename(target, &backup).map_err(|e| e.to_string())?;
            }
            changes.push((i, existed, false));
            rename(&stage.path().join(format!("new-{i}")), target).map_err(|e| e.to_string())?;
            changes.last_mut().unwrap().2 = true;
        }
        Ok(())
    })();
    if let Err(error) = result {
        let mut failed = false;
        for &(i, existed, published) in changes.iter().rev() {
            if published && fs::remove_file(&targets[i]).is_err() {
                failed = true;
            }
            if existed && rename(&stage.path().join(format!("old-{i}")), &targets[i]).is_err() {
                failed = true;
            }
        }
        if failed {
            let recovery = stage.keep();
            return Err(format!(
                "Kaarten publiceren mislukt: {error}. Herstelbestanden bewaard in {}",
                recovery.display()
            ));
        }
        return Err(format!(
            "Kaarten publiceren mislukt; vorige bestanden hersteld: {error}"
        ));
    }
    Ok(())
}
pub fn export(
    directory: &Path,
    settings: &Settings,
    inputs: &Inputs,
    control: &Control,
) -> Result<Vec<PathBuf>, String> {
    export_with(directory, settings, inputs, None, control)
}
/// Explicit fonts make independent render comparisons reproducible.
pub fn export_with(
    directory: &Path,
    settings: &Settings,
    inputs: &Inputs,
    fonts: Option<map_text::Paths>,
    control: &Control,
) -> Result<Vec<PathBuf>, String> {
    settings.validate()?;
    control.check()?;
    if inputs.rasters.is_empty() {
        return Err("Laad eerst minimaal een GeoTIFF-bestand.".into());
    }
    if inputs.rasters.len() > 5000 {
        return Err("Te veel kaarten voor één export".into());
    }
    for raster in &inputs.rasters {
        if raster.epsg.is_some_and(|e| e != 28992) {
            return Err("Kaartuitvoer vereist RD (EPSG:28992)".into());
        }
        map_page::layout(raster.bounds, settings.page())?;
    }
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let directory = fs::canonicalize(directory).map_err(|e| e.to_string())?;
    let targets = filenames(inputs.rasters.iter().map(|r| &r.path))
        .into_iter()
        .map(|n| directory.join(n))
        .collect::<Vec<_>>();
    let mut protected = sources(inputs);
    if let Some(f) = &fonts {
        protected.extend([f.regular.as_path(), f.bold.as_path()]);
    }
    check_targets(&targets, &protected)?;
    let stage = tempfile::Builder::new()
        .prefix(".sleufbase-kaarten-")
        .tempdir_in(&directory)
        .map_err(|e| e.to_string())?;
    let mut renderer = Renderer::default();
    let mut bytes = 0u64;
    for (i, raster) in inputs.rasters.iter().enumerate() {
        control.check()?;
        control.set_stage(Stage::Planning);
        control.set_detail(&format!(
            "Exporteer kaart {}/{}: {}",
            i + 1,
            inputs.rasters.len(),
            raster
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ));
        let layout = map_page::layout(raster.bounds, settings.page())?;
        let mut strokes = Vec::new();
        if settings.include_cad {
            crate::template_cad::append_visible(
                &inputs.overlays,
                layout.bounds,
                &mut strokes,
                control,
            )?;
        }
        let background = if settings.background == "white" {
            Background::Solid { color: [255; 4] }
        } else if settings.background == "cadastral" {
            Background::Cadastral
        } else if settings.background == "osm" {
            let map = crate::osm_tiles::request_export_map_with_cancel(
                layout.bounds,
                layout.map_size[0],
                layout.map_size[1],
                &crate::project::app_data_directory().join("osm-tiles"),
                control.flag(),
            )?;
            if map.stale_cache {
                return Err("OSM-achtergrond gebruikt verlopen cache; probeer de export opnieuw met netwerk".into());
            }
            let path = stage.path().join("background.png");
            image::RgbaImage::from_raw(map.width, map.height, map.rgba)
                .ok_or("Ongeldige OSM-pixels")?
                .save(&path)
                .map_err(|e| e.to_string())?;
            Background::Png { path }
        } else {
            Background::Pdok
        };
        let source = map_export::Source {
            settings: settings.page(),
            background,
            overlays: Arc::new(strokes),
            fonts: fonts.clone(),
            attribution: Some(if settings.background == "osm" {
                "(c) OpenStreetMap contributors".into()
            } else if settings.background == "cadastral" {
                "Kadastrale kaart: Kadaster / PDOK".into()
            } else {
                String::new()
            }),
            tiff_opacity: 0.7,
            ..Default::default()
        };
        let tiff = template_raster::Source {
            path: raster.path.clone(),
            transform: raster.transform,
            reverse_orientation: false,
            axis: None,
        };
        let page = renderer.render(&source, raster.bounds, Some(&tiff), None, control.flag())?;
        control.set_stage(Stage::Writing);
        let path = stage.path().join(format!("new-{i}"));
        write_png(&path, &page, settings.dpi, control)?;
        bytes = bytes.saturating_add(fs::metadata(&path).map_err(|e| e.to_string())?.len());
        if bytes > 8 * 1024 * 1024 * 1024 {
            return Err("Kaartbatch overschrijdt 8 GiB uitvoerbudget".into());
        }
    }
    check_targets(&targets, &protected)?;
    publish(stage, &targets, control, |from, to| fs::rename(from, to))?;
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_at_publication_preserves_previous_files() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("map.png");
        fs::write(&target, b"old").unwrap();
        let stage = tempfile::tempdir_in(dir.path()).unwrap();
        fs::write(stage.path().join("new-0"), b"new").unwrap();
        let control = Control::default();
        assert!(control.cancel());
        assert!(publish(
            stage,
            std::slice::from_ref(&target),
            &control,
            |_, _| panic!("Cancelled publication attempted rename")
        )
        .is_err());
        assert_eq!(fs::read(target).unwrap(), b"old");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn failed_rollback_keeps_original_bytes_and_destination_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let targets = [dir.path().join("a.png"), dir.path().join("b.png")];
        for target in &targets {
            fs::write(target, b"original").unwrap();
        }
        let stage = tempfile::tempdir_in(dir.path()).unwrap();
        let recovery = stage.path().to_owned();
        for i in 0..2 {
            fs::write(stage.path().join(format!("new-{i}")), b"new").unwrap();
        }
        let error = publish(stage, &targets, &Control::default(), |from, to| {
            if from.file_name().unwrap() == "new-1" || from.file_name().unwrap() == "old-0" {
                Err(io::Error::other("locked file"))
            } else {
                fs::rename(from, to)
            }
        })
        .unwrap_err();
        assert!(error.contains("Herstelbestanden bewaard"));
        assert!(error.contains(&recovery.display().to_string()));
        assert_eq!(fs::read(recovery.join("old-0")).unwrap(), b"original");
        assert_eq!(fs::read(&targets[1]).unwrap(), b"original");
        let manifest: Vec<PathBuf> =
            serde_json::from_slice(&fs::read(recovery.join("destinations.json")).unwrap()).unwrap();
        assert_eq!(manifest, targets);
    }
    #[test]
    fn late_publish_failure_restores_all_previous_files() {
        let dir = tempfile::tempdir().unwrap();
        let targets = [dir.path().join("a.png"), dir.path().join("b.png")];
        for p in &targets {
            fs::write(p, b"old").unwrap();
        }
        let stage = tempfile::tempdir_in(dir.path()).unwrap();
        for i in 0..2 {
            fs::write(stage.path().join(format!("new-{i}")), b"new").unwrap();
        }
        let error = publish(stage, &targets, &Control::default(), |from, to| {
            if from.file_name().unwrap() == "new-1" {
                Err(io::Error::other("injected rename failure"))
            } else {
                fs::rename(from, to)
            }
        })
        .unwrap_err();
        assert!(error.contains("hersteld"));
        for p in &targets {
            assert_eq!(fs::read(p).unwrap(), b"old");
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
