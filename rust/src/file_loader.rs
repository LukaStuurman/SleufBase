//! One bounded background worker for local input. A batch publishes only when
//! finished; cancellation/replacement drops all staged data and stale results.
use crate::{
    cross_section::Marker, dxf_read::DxfOverlay, geotiff::GeoTiffInfo, marxact::MarXactResult,
    model::Dataset, project::NativeProject,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
        Arc,
    },
};

#[derive(Clone, Debug)]
pub enum FileKind {
    Project,
    PythonProject,
    Dxf,
    ReloadDxf {
        visible: bool,
        layers: Arc<BTreeMap<String, bool>>,
    },
    MarXact(Arc<BTreeMap<String, String>>),
    Dataset,
    GeoTiff,
}
#[derive(Debug)]
pub struct PreparedProject {
    pub document: NativeProject,
    pub markers: Vec<Vec<Marker>>,
}
#[derive(Debug)]
pub struct PreparedMarXact {
    pub result: MarXactResult,
    pub markers: Vec<Vec<Marker>>,
}
#[derive(Debug)]
pub enum FileContent {
    Project(Box<PreparedProject>),
    Dxf(Box<DxfOverlay>),
    MarXact(Box<PreparedMarXact>),
    Dataset(Box<Dataset>),
    GeoTiff(Box<GeoTiffInfo>),
}
#[derive(Debug)]
pub struct FileOutcome {
    pub path: PathBuf,
    pub result: Result<FileContent, String>,
}
pub struct BatchReady {
    pub ticket: u64,
    pub result: Result<Vec<FileOutcome>, String>,
}
struct Work {
    ticket: u64,
    kind: FileKind,
    paths: Arc<Vec<PathBuf>>,
    cancel: Arc<AtomicBool>,
    completed: Arc<AtomicUsize>,
}
struct Active {
    ticket: u64,
    paths: Arc<Vec<PathBuf>>,
    cancel: Arc<AtomicBool>,
    completed: Arc<AtomicUsize>,
}
pub struct FileProgress<'a> {
    pub completed: usize,
    pub total: usize,
    pub path: Option<&'a Path>,
}
pub struct FileLoader {
    requests: SyncSender<Work>,
    results: Receiver<BatchReady>,
    pending: Option<Work>,
    active: Option<Active>,
    next_ticket: u64,
    closed: bool,
}

fn markers(
    trenches: &[crate::virtual_trench::VirtualTrench],
    cancel: &AtomicBool,
) -> Result<Vec<Vec<Marker>>, String> {
    trenches
        .iter()
        .map(|trench| crate::cross_section::markers_with_cancel(trench, cancel))
        .collect()
}
fn process(kind: &FileKind, path: &Path, cancel: &AtomicBool) -> Result<FileContent, String> {
    crate::input::check(cancel)?;
    let result = match kind {
        FileKind::Project | FileKind::PythonProject => {
            let document = if matches!(kind, FileKind::Project) {
                crate::project::read_project_with_cancel(path, cancel)?
            } else {
                crate::legacy_project::read_legacy_project_with_cancel(path, cancel)?
            };
            let markers = markers(&document.virtual_trenches, cancel)?;
            FileContent::Project(Box::new(PreparedProject { document, markers }))
        }
        FileKind::Dxf | FileKind::ReloadDxf { .. } => {
            let mut overlay = crate::dxf_read::load_dxf_with_cancel(path, cancel)?;
            if let FileKind::ReloadDxf { visible, layers } = kind {
                overlay.visible = *visible;
                let folded: BTreeMap<_, _> =
                    layers.iter().map(|(n, v)| (n.to_lowercase(), *v)).collect();
                for (name, layer) in &mut overlay.layers {
                    crate::input::check(cancel)?;
                    if let Some(visible) = layers
                        .get(name)
                        .or_else(|| folded.get(&name.to_lowercase()))
                    {
                        layer.visible = *visible;
                    }
                }
                overlay.refresh_block_visibility();
            }
            FileContent::Dxf(Box::new(overlay))
        }
        FileKind::MarXact(mapping) => {
            let result = crate::marxact::import_dxf_with_cancel(path, mapping, cancel)?;
            let markers = markers(&result.trenches, cancel)?;
            FileContent::MarXact(Box::new(PreparedMarXact { result, markers }))
        }
        FileKind::Dataset => FileContent::Dataset(Box::new(
            crate::parse::load_job_features_with_cancel(path, cancel)?,
        )),
        FileKind::GeoTiff => {
            FileContent::GeoTiff(Box::new(crate::geotiff::read_geotiff_metadata(path)?))
        }
    };
    crate::input::check(cancel)?;
    Ok(result)
}
impl FileLoader {
    pub fn new() -> Result<Self, String> {
        Self::with_processor(process)
    }
    /// Injection seam for deterministic tests; production uses `new()`.
    #[doc(hidden)]
    pub fn with_processor<F>(processor: F) -> Result<Self, String>
    where
        F: Fn(&FileKind, &Path, &AtomicBool) -> Result<FileContent, String> + Send + 'static,
    {
        let (requests, rx) = mpsc::sync_channel::<Work>(1);
        let (tx, results) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("sleufbase-files".into())
            .spawn(move || {
                while let Ok(work) = rx.recv() {
                    if work.cancel.load(Ordering::Relaxed) {
                        continue;
                    }
                    let result = (|| {
                        let mut outcomes = Vec::new();
                        outcomes.try_reserve(work.paths.len()).map_err(|_| {
                            "Onvoldoende geheugen voor deze bestandsselectie".to_owned()
                        })?;
                        for (index, path) in work.paths.iter().enumerate() {
                            crate::input::check(&work.cancel)?;
                            let result =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    processor(&work.kind, path, &work.cancel)
                                }))
                                .unwrap_or_else(|_| {
                                    Err("Bestandslezer is onverwacht gestopt bij dit bestand"
                                        .into())
                                });
                            crate::input::check(&work.cancel)?;
                            outcomes.push(FileOutcome {
                                path: path.clone(),
                                result,
                            });
                            work.completed.store(index + 1, Ordering::Relaxed);
                        }
                        Ok(outcomes)
                    })();
                    if !work.cancel.load(Ordering::Relaxed)
                        && tx
                            .send(BatchReady {
                                ticket: work.ticket,
                                result,
                            })
                            .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| format!("Bestandsachtergrondtaak kon niet starten: {e}"))?;
        Ok(Self {
            requests,
            results,
            pending: None,
            active: None,
            next_ticket: 0,
            closed: false,
        })
    }
    pub fn is_closed(&self) -> bool {
        self.closed
    }
    pub fn is_busy(&self) -> bool {
        self.active.is_some()
    }
    pub fn progress(&self) -> Option<FileProgress<'_>> {
        self.active.as_ref().map(|a| {
            let completed = a.completed.load(Ordering::Relaxed).min(a.paths.len());
            FileProgress {
                completed,
                total: a.paths.len(),
                path: a.paths.get(completed).map(PathBuf::as_path),
            }
        })
    }
    pub fn start(&mut self, kind: FileKind, paths: Vec<PathBuf>) -> Result<u64, String> {
        self.cancel();
        if self.closed {
            return Err("Bestandsachtergrondtaak is gestopt".into());
        }
        if paths.is_empty() {
            return Err("Geen bestanden geselecteerd".into());
        }
        self.next_ticket = self.next_ticket.wrapping_add(1);
        let ticket = self.next_ticket;
        let paths = Arc::new(paths);
        let cancel = Arc::new(AtomicBool::new(false));
        let completed = Arc::new(AtomicUsize::new(0));
        self.active = Some(Active {
            ticket,
            paths: paths.clone(),
            cancel: cancel.clone(),
            completed: completed.clone(),
        });
        self.pending = Some(Work {
            ticket,
            kind,
            paths,
            cancel,
            completed,
        });
        self.pump();
        Ok(ticket)
    }
    pub fn cancel(&mut self) {
        if let Some(active) = self.active.take() {
            active.cancel.store(true, Ordering::Relaxed);
        }
        if let Some(pending) = self.pending.take() {
            pending.cancel.store(true, Ordering::Relaxed);
        }
        // Release completed but not yet applied old documents immediately.
        while self.results.try_recv().is_ok() {}
    }
    fn pump(&mut self) {
        let Some(work) = self.pending.take() else {
            return;
        };
        match self.requests.try_send(work) {
            Ok(()) => {}
            Err(TrySendError::Full(work)) => self.pending = Some(work),
            Err(TrySendError::Disconnected(work)) => {
                self.pending = Some(work);
                self.closed = true;
            }
        }
    }
    pub fn poll(&mut self) -> Option<BatchReady> {
        loop {
            match self.results.try_recv() {
                Ok(ready)
                    if self
                        .active
                        .as_ref()
                        .is_some_and(|a| a.ticket == ready.ticket) =>
                {
                    self.active = None;
                    self.pending = None;
                    return Some(ready);
                }
                Ok(_) => {}
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.closed = true;
                    break;
                }
            }
        }
        self.pump();
        if self.closed {
            self.pending = None;
            return self.active.take().map(|a| BatchReady {
                ticket: a.ticket,
                result: Err(
                    "Bestandsachtergrondtaak is gestopt; huidige gegevens zijn behouden".into(),
                ),
            });
        }
        None
    }
}
impl Drop for FileLoader {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    fn dataset(path: &Path) -> FileContent {
        FileContent::Dataset(Box::new(Dataset {
            job_id: 1,
            title: path.display().to_string(),
            source_path: path.into(),
            features: vec![],
        }))
    }
    fn drain(loader: &mut FileLoader) -> BatchReady {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(result) = loader.poll() {
                return result;
            }
            assert!(Instant::now() < deadline, "file worker did not finish");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn replacement_is_nonblocking_and_discards_a_running_old_batch() {
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (gate_tx, gate_rx) = mpsc::sync_channel(1);
        let mut loader = FileLoader::with_processor(move |_, path, _| {
            if path == Path::new("old") {
                started_tx.send(std::thread::current().id()).unwrap();
                gate_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            }
            Ok(dataset(path))
        })
        .unwrap();
        loader
            .start(FileKind::Dataset, vec!["old".into(), "must-not-run".into()])
            .unwrap();
        let worker = started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_ne!(worker, std::thread::current().id());
        assert!(loader.poll().is_none());
        let ticket = loader.start(FileKind::Dataset, vec!["new".into()]).unwrap();
        assert_eq!(loader.progress().unwrap().completed, 0);
        gate_tx.send(()).unwrap();
        let ready = drain(&mut loader);
        assert_eq!(ready.ticket, ticket);
        let outcomes = ready.result.unwrap();
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].path, Path::new("new"));
        assert!(!loader.is_busy());
        assert!(!loader.is_closed());
    }
    #[test]
    fn batches_are_staged_until_complete_and_cancel_drops_prior_successes() {
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (gate_tx, gate_rx) = mpsc::sync_channel(1);
        let mut loader = FileLoader::with_processor(move |_, path, _| {
            if path == Path::new("second") {
                started_tx.send(()).unwrap();
                gate_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            }
            Ok(dataset(path))
        })
        .unwrap();
        loader
            .start(FileKind::Dataset, vec!["first".into(), "second".into()])
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(loader.progress().unwrap().completed, 1);
        assert!(loader.poll().is_none());
        loader.cancel();
        gate_tx.send(()).unwrap();
        assert!(!loader.is_busy());
        loader
            .start(FileKind::Dataset, vec!["final".into()])
            .unwrap();
        let ready = drain(&mut loader).result.unwrap();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].path, Path::new("final"));
    }
    #[test]
    fn a_parser_panic_is_a_per_file_error_and_the_same_worker_continues() {
        let mut loader = FileLoader::with_processor(|_, path, _| {
            if path == Path::new("panic") {
                panic!("test parser failure");
            }
            Ok(dataset(path))
        })
        .unwrap();
        loader
            .start(FileKind::Dataset, vec!["panic".into(), "good".into()])
            .unwrap();
        let ready = drain(&mut loader).result.unwrap();
        assert_eq!(ready.len(), 2);
        assert!(ready[0].result.as_ref().unwrap_err().contains("onverwacht"));
        assert!(ready[1].result.is_ok());
        assert!(!loader.is_closed());
        loader
            .start(FileKind::Dataset, vec!["next".into()])
            .unwrap();
        assert!(drain(&mut loader).result.unwrap()[0].result.is_ok());
    }
    #[test]
    fn dropping_the_loader_signals_a_running_reader_without_waiting_for_it() {
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (stopped_tx, stopped_rx) = mpsc::sync_channel(1);
        let mut loader = FileLoader::with_processor(move |_, path, cancel| {
            started_tx.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while !cancel.load(Ordering::Relaxed) {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(2));
            }
            stopped_tx.send(()).unwrap();
            Ok(dataset(path))
        })
        .unwrap();
        loader
            .start(FileKind::Dataset, vec!["slow".into()])
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        drop(loader);
        stopped_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    }
    #[test]
    fn real_local_formats_are_prepared_off_thread_with_ready_marker_caches() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("native.json");
        crate::project::save_project(&project, &NativeProject::default()).unwrap();
        let mut loader = FileLoader::new().unwrap();
        for (kind, path) in [
            (FileKind::Project, project),
            (FileKind::PythonProject, root.join("python-v1.klicproject")),
            (FileKind::Dxf, root.join("splines/splines-binary.dxf")),
            (
                FileKind::MarXact(Arc::new(BTreeMap::new())),
                root.join("marxact-cad/nested-leaders.dxf"),
            ),
            (FileKind::Dataset, root.join("jobFeatures.json")),
            (FileKind::GeoTiff, root.join("rasters/gray16.tiff")),
        ] {
            loader.start(kind, vec![path]).unwrap();
            let outcome = drain(&mut loader)
                .result
                .unwrap()
                .pop()
                .unwrap()
                .result
                .unwrap();
            match outcome {
                FileContent::Project(p) => {
                    assert_eq!(p.markers.len(), p.document.virtual_trenches.len())
                }
                FileContent::MarXact(p) => {
                    assert!(!p.result.trenches.is_empty());
                    assert_eq!(p.markers.len(), p.result.trenches.len());
                }
                FileContent::Dxf(d) => assert_eq!(d.rendered_features().count(), 60),
                FileContent::Dataset(d) => assert!(!d.features.is_empty()),
                FileContent::GeoTiff(t) => assert!(t.width > 0),
            }
        }
    }
}
