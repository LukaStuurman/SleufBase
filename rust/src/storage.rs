//! Atomic file publication after flushing data, with explicit overwrite semantics.
use std::{
    fs::{self, File},
    path::Path,
};
use tempfile::NamedTempFile;

pub fn atomic_write(
    path: &Path,
    overwrite: bool,
    write: impl FnOnce(&mut File) -> Result<(), String>,
) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut temp = NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    write(temp.as_file_mut())?;
    temp.as_file_mut().sync_all().map_err(|e| e.to_string())?;
    if overwrite {
        temp.persist(path)
    } else {
        temp.persist_noclobber(path)
    }
    .map_err(|e| format!("Bestand opslaan mislukt: {}", e.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        sync::{Arc, Barrier},
        thread,
    };

    #[test]
    fn failure_preserves_old_output_and_removes_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("project.json");
        fs::write(&path, b"original").unwrap();
        let result = atomic_write(&path, true, |file| {
            file.write_all(b"partial").unwrap();
            Err("simulated serialization error".into())
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn concurrent_new_exports_never_overwrite_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("export.dxf");
        let barrier = Arc::new(Barrier::new(2));
        let threads: Vec<_> = (*b"12")
            .into_iter()
            .map(|byte| {
                let path = path.clone();
                let barrier = barrier.clone();
                thread::spawn(move || {
                    atomic_write(&path, false, |file| {
                        file.write_all(&vec![byte; 4096])
                            .map_err(|e| e.to_string())?;
                        barrier.wait();
                        Ok(())
                    })
                })
            })
            .collect();
        let successes = threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .filter(Result::is_ok)
            .count();
        assert_eq!(successes, 1);
        let saved = fs::read(&path).unwrap();
        assert!(saved == vec![b'1'; 4096] || saved == vec![b'2'; 4096]);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
