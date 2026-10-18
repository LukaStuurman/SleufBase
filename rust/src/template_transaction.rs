//! Publish a template drawing and its permanent image bundle as one attempt.
//! Old bundles survive replacement: saved drawing copies may still reference them.
use crate::export_control::Control;
use std::{
    fs::{self, OpenOptions},
    io,
    path::{Component, Path, PathBuf},
};
use tempfile::TempDir;

fn canonical_directory(path: &Path) -> Result<PathBuf, String> {
    let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        // CAD IMAGEDEF paths use normal drive/UNC spelling, not Win32's \?\ namespace.
        use std::{
            ffi::OsString,
            os::windows::ffi::{OsStrExt, OsStringExt},
            path::Prefix,
        };
        let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        if let Some(Component::Prefix(p)) = path.components().next() {
            match p.kind() {
                Prefix::VerbatimDisk(_) => {
                    return Ok(PathBuf::from(OsString::from_wide(&wide[4..])))
                }
                Prefix::VerbatimUNC(_, _) => {
                    let mut normal = vec![92, 92];
                    normal.extend_from_slice(&wide[8..]);
                    return Ok(PathBuf::from(OsString::from_wide(&normal)));
                }
                _ => {}
            }
        }
    }
    Ok(path)
}

pub struct Transaction {
    output: PathBuf,
    bundle: TempDir,
    staged: PathBuf,
}
#[derive(Debug)]
pub struct Published {
    pub drawing: PathBuf,
    pub assets: PathBuf,
}
impl Transaction {
    pub fn new(output: &Path, control: &Control) -> Result<Self, String> {
        control.check()?;
        let name = output
            .file_name()
            .ok_or("Geef een geldige DXF-uitvoerbestandsnaam")?;
        Self::check_name(
            name.to_str()
                .ok_or("DXF-uitvoerpad is geen geldige Unicode")?,
        )?;
        let stem = output
            .file_stem()
            .ok_or("DXF-uitvoer mist een bestandsnaam")?;
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let parent = canonical_directory(parent)?;
        let mut asset_name = stem.to_os_string();
        asset_name.push("_assets");
        let asset_root = parent.join(asset_name);
        fs::create_dir_all(&asset_root).map_err(|e| e.to_string())?;
        let asset_root = canonical_directory(&asset_root)?;
        control.check()?;
        let bundle = tempfile::Builder::new()
            .prefix("export-")
            .tempdir_in(&asset_root)
            .map_err(|e| e.to_string())?;
        // Cleanup is owned by this unique TempDir, never by the shared asset root.
        if bundle.path().parent() != Some(asset_root.as_path()) {
            return Err("DXF-exportmap ligt buiten de assetmap".into());
        }
        let staged = bundle.path().join(name);
        Ok(Self {
            output: parent.join(name),
            bundle,
            staged,
        })
    }
    /// The caller closes all writers before publish. The directory is permanent
    /// after success, so image definitions can use these final absolute paths.
    pub fn directory(&self) -> &Path {
        self.bundle.path()
    }
    pub fn staged_drawing(&self) -> &Path {
        &self.staged
    }
    fn check_name(name: &str) -> Result<(), String> {
        let mut components = Path::new(name).components();
        let basename = name
            .split('.')
            .next()
            .unwrap_or("")
            .trim_end_matches(' ')
            .to_ascii_uppercase();
        let device = matches!(
            basename.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) || ["COM", "LPT"].iter().any(|prefix| {
            basename.strip_prefix(prefix).is_some_and(|s| {
                matches!(
                    s,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        });
        if !matches!(components.next(), Some(Component::Normal(_)))
            || components.next().is_some()
            || name.chars().any(|c| {
                c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
            })
            || name.ends_with(['.', ' '])
            || device
        {
            return Err("Ongeldige bestandsnaam voor sjabloonexport".into());
        }
        Ok(())
    }
    /// A single flat image filename, with no path traversal or Windows device name.
    pub fn asset_path(&self, name: &str) -> Result<PathBuf, String> {
        Self::check_name(name)?;
        let path = self.bundle.path().join(name);
        if self
            .staged
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| {
                crate::python_values::uppercase(n) == crate::python_values::uppercase(name)
            })
        {
            return Err("Een asset mag de tijdelijke DXF niet vervangen".into());
        }
        Ok(path)
    }
    /// Replaces an existing drawing, matching Python's template transaction.
    /// Cancellation wins until the final gate after all file flushes.
    pub fn publish(self, control: &Control) -> Result<Published, String> {
        let result = self.publish_with(control, || Ok(()), |from, to| fs::rename(from, to));
        control.finish();
        result
    }
    fn publish_with(
        self,
        control: &Control,
        before_gate: impl FnOnce() -> Result<(), String>,
        rename: impl FnOnce(&Path, &Path) -> io::Result<()>,
    ) -> Result<Published, String> {
        control.check()?;
        let metadata = fs::symlink_metadata(&self.staged)
            .map_err(|_| "DXF-sjabloonexport heeft geen compleet bestand gemaakt".to_owned())?;
        if !metadata.file_type().is_file() || metadata.len() == 0 {
            return Err("DXF-sjabloonexport heeft geen compleet bestand gemaakt".into());
        }
        // Native template assets live directly in this owned bundle. A directory
        // or link is rejected rather than recursively flushing unrelated files.
        for entry in fs::read_dir(self.bundle.path()).map_err(|e| e.to_string())? {
            control.check()?;
            let entry = entry.map_err(|e| e.to_string())?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
            if !metadata.file_type().is_file() || metadata.len() == 0 {
                return Err(
                    "Sjabloonbundle bevat een onvolledig bestand of onverwachte map/link".into(),
                );
            }
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(entry.path())
                .map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            control.check()?;
        }
        before_gate()?;
        control.begin_publish()?;
        rename(&self.staged, &self.output).map_err(|e| format!("DXF publiceren mislukt: {e}"))?;
        // No fallible work after rename: otherwise an error/drop could remove
        // images already referenced by the newly published drawing.
        let assets = self.bundle.keep();
        Ok(Published {
            drawing: self.output,
            assets,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn old_export(root: &Path) -> (PathBuf, PathBuf) {
        let output = root.join("test.dxf");
        fs::write(&output, b"previous drawing").unwrap();
        let assets = root.join("test_assets");
        fs::create_dir(&assets).unwrap();
        fs::write(assets.join("logo.png"), b"previous image").unwrap();
        (output, assets)
    }
    fn intact(output: &Path, assets: &Path) {
        assert_eq!(fs::read(output).unwrap(), b"previous drawing");
        assert_eq!(
            fs::read(assets.join("logo.png")).unwrap(),
            b"previous image"
        );
        assert_eq!(fs::read_dir(assets).unwrap().count(), 1);
    }
    #[test]
    fn failed_phases_and_panics_remove_only_the_owned_attempt() {
        let root = tempfile::tempdir().unwrap();
        let (output, assets) = old_export(root.path());
        for phase in ["raster", "normal", "reverse", "dynamic validation"] {
            let result = (|| -> Result<(), String> {
                let tx = Transaction::new(&output, &Control::default())?;
                fs::write(tx.asset_path("logo.png")?, b"new image").unwrap();
                fs::write(tx.staged_drawing(), b"partial drawing").unwrap();
                Err(phase.into())
            })();
            assert_eq!(result.unwrap_err(), phase);
            intact(&output, &assets);
        }
        let result = std::panic::catch_unwind(|| {
            let tx = Transaction::new(&output, &Control::default()).unwrap();
            fs::write(tx.staged_drawing(), b"partial").unwrap();
            panic!("encoder failure");
        });
        assert!(result.is_err());
        intact(&output, &assets);
    }
    #[test]
    fn missing_empty_and_invalid_assets_preserve_the_previous_export() {
        let root = tempfile::tempdir().unwrap();
        let (output, assets) = old_export(root.path());
        for variant in 0..4 {
            let control = Control::default();
            let tx = Transaction::new(&output, &control).unwrap();
            if variant > 0 {
                fs::write(
                    tx.staged_drawing(),
                    if variant == 1 {
                        &b""[..]
                    } else {
                        &b"complete"[..]
                    },
                )
                .unwrap();
            }
            if variant == 2 {
                fs::write(tx.asset_path("empty.png").unwrap(), b"").unwrap();
            }
            if variant == 3 {
                fs::create_dir(tx.directory().join("unexpected")).unwrap();
            }
            assert!(tx.publish(&control).is_err());
            intact(&output, &assets);
        }
    }
    #[test]
    fn cancellation_after_sync_and_rename_failure_leave_no_new_bundle() {
        let root = tempfile::tempdir().unwrap();
        let (output, assets) = old_export(root.path());
        let control = Control::default();
        let tx = Transaction::new(&output, &control).unwrap();
        fs::write(tx.staged_drawing(), b"complete").unwrap();
        let result = tx.publish_with(
            &control,
            || {
                assert!(control.cancel());
                Ok(())
            },
            |_, _| panic!("must not rename after cancellation"),
        );
        assert!(result.unwrap_err().contains("geannuleerd"));
        intact(&output, &assets);
        let control = Control::default();
        let tx = Transaction::new(&output, &control).unwrap();
        fs::write(tx.staged_drawing(), b"complete").unwrap();
        assert!(tx
            .publish_with(
                &control,
                || Ok(()),
                |_, _| Err(io::Error::new(io::ErrorKind::PermissionDenied, "locked"))
            )
            .is_err());
        intact(&output, &assets);
    }
    #[test]
    fn success_keeps_prior_images_and_copied_drawings() {
        let root = tempfile::tempdir().unwrap();
        let (output, assets) = old_export(root.path());
        let mut paths = Vec::new();
        for content in [b"first image".as_slice(), b"second image".as_slice()] {
            let control = Control::default();
            let tx = Transaction::new(&output, &control).unwrap();
            let image = tx.asset_path("logo.png").unwrap();
            fs::write(&image, content).unwrap();
            fs::write(tx.staged_drawing(), image.to_str().unwrap()).unwrap();
            let published = tx.publish(&control).unwrap();
            assert_eq!(published.assets, image.parent().unwrap());
            assert_eq!(
                fs::canonicalize(published.drawing).unwrap(),
                fs::canonicalize(&output).unwrap()
            );
            #[cfg(windows)]
            assert!(!published.assets.to_string_lossy().starts_with(r"\\?\"));
            assert!(!control.can_cancel());
            assert!(!control.cancel());
            paths.push(image);
        }
        assert_ne!(paths[0], paths[1]);
        assert_eq!(fs::read(&paths[0]).unwrap(), b"first image");
        assert_eq!(fs::read(&paths[1]).unwrap(), b"second image");
        assert_eq!(
            fs::read_to_string(output).unwrap(),
            paths[1].to_str().unwrap()
        );
        assert_eq!(
            fs::read(assets.join("logo.png")).unwrap(),
            b"previous image"
        );
    }
    #[test]
    fn overlapping_attempts_own_distinct_bundles_and_last_publish_wins() {
        let root = tempfile::tempdir().unwrap();
        let (output, assets) = old_export(root.path());
        let first_control = Control::default();
        let first = Transaction::new(&output, &first_control).unwrap();
        let second_control = Control::default();
        let second = Transaction::new(&output, &second_control).unwrap();
        assert_ne!(first.directory(), second.directory());
        fs::write(first.staged_drawing(), b"first").unwrap();
        fs::write(second.staged_drawing(), b"second").unwrap();
        let second = second.publish(&second_control).unwrap();
        assert_eq!(fs::read(&output).unwrap(), b"second");
        let first = first.publish(&first_control).unwrap();
        assert_eq!(fs::read(&output).unwrap(), b"first");
        assert!(first.assets.exists() && second.assets.exists());
        assert_eq!(fs::read_dir(assets).unwrap().count(), 3);
    }
    #[test]
    fn filenames_cannot_escape_or_select_windows_devices_or_the_drawing() {
        let root = tempfile::tempdir().unwrap();
        let tx = Transaction::new(&root.path().join("test.dxf"), &Control::default()).unwrap();
        for name in [
            "",
            ".",
            "..",
            "../escape.png",
            "a/b.png",
            "a\\b.png",
            "C:escape.png",
            "CON.png",
            "CON .png",
            "com¹.png",
            "LPT9",
            "test.dxf",
            "TEST.DXF",
            "bad.",
            "bad ",
            "bad\0.png",
        ] {
            assert!(tx.asset_path(name).is_err(), "{name:?}");
        }
        assert_eq!(
            tx.asset_path("PS2__slot001_Ø.png").unwrap().parent(),
            Some(tx.directory())
        );
        let cancelled = Control::default();
        cancelled.cancel();
        assert!(Transaction::new(&root.path().join("cancelled/none.dxf"), &cancelled).is_err());
        assert!(!root.path().join("cancelled").exists());
        assert!(Transaction::new(&root.path().join("CON.dxf"), &Control::default()).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn a_real_locked_windows_destination_preserves_output_and_images() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = tempfile::tempdir().unwrap();
        let (output, assets) = old_export(root.path());
        let lock = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&output)
            .unwrap();
        let control = Control::default();
        let tx = Transaction::new(&output, &control).unwrap();
        fs::write(tx.staged_drawing(), b"complete").unwrap();
        assert!(tx.publish(&control).is_err());
        intact(&output, &assets);
        drop(lock);
    }
}
