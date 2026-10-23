//! Local-file progress screen and transactional application to the active project.
use super::*;
use sleufbase_rust::file_loader::{FileContent, FileKind, FileLoader, FileOutcome};
use std::{collections::BTreeSet, sync::Arc};

pub(super) enum LoadPurpose {
    Project(ProjectAction),
    Dxf,
    ExportedDxf(PathBuf),
    Reload { index: usize, original: PathBuf },
    MarXact,
    Datasets,
    GeoTiffs,
}

pub(super) struct FileIntent {
    purpose: LoadPurpose,
    ticket: u64,
    project_generation: u64,
}

impl SleufBaseApp {
    pub(super) fn cancel_file_load(&mut self) {
        if let Some(loader) = &mut self.file_loader {
            loader.cancel();
        }
        self.file_intent = None;
    }
    fn start_file_load(&mut self, purpose: LoadPurpose, kind: FileKind, paths: Vec<PathBuf>) {
        if self.export_intent.is_some(){self.message="Wacht tot de export is afgerond voordat je bestanden laadt".into();return;}
        self.cancel_file_load();
        if paths.is_empty() {
            self.message =
                "Geen nieuwe bestanden om te laden; deze bronnen zijn al aanwezig.".into();
            return;
        }
        if self.file_loader.as_ref().is_none_or(FileLoader::is_closed) {
            match FileLoader::new() {
                Ok(loader) => self.file_loader = Some(loader),
                Err(e) => {
                    self.message = e;
                    return;
                }
            }
        }
        match self.file_loader.as_mut().unwrap().start(kind, paths) {
            Ok(ticket) => {
                self.file_intent = Some(FileIntent {
                    purpose,
                    ticket,
                    project_generation: self.project_generation,
                });
                self.message="Bestanden worden geladen. Huidige gegevens blijven behouden tot de laadactie klaar is.".into();
            }
            Err(e) => self.message = e,
        }
    }
    pub(super) fn start_project_load(&mut self, action: ProjectAction) {
        let (kind, path) = match &action {
            ProjectAction::Open(path) | ProjectAction::Restore(path) => {
                (FileKind::Project, path.clone())
            }
            ProjectAction::ImportPython(path) => (FileKind::PythonProject, path.clone()),
            _ => return,
        };
        self.start_file_load(LoadPurpose::Project(action), kind, vec![path]);
    }
    pub(super) fn load_files(&mut self, paths: Vec<PathBuf>) {
        let paths = self.new_paths(paths, |path| {
            self.datasets.iter().any(|d| d.source_path == *path)
        });
        self.start_file_load(LoadPurpose::Datasets, FileKind::Dataset, paths);
    }
    pub(super) fn load_geotiffs(&mut self, paths: Vec<PathBuf>, _ctx: &egui::Context) {
        let paths = self.new_paths(paths, |path| self.geotiffs.iter().any(|d| d.path == *path));
        self.start_file_load(LoadPurpose::GeoTiffs, FileKind::GeoTiff, paths);
    }
    pub(super) fn load_overlays(&mut self, paths: Vec<PathBuf>) {
        let paths = self.new_paths(paths, |path| self.overlays.iter().any(|d| d.path == *path));
        self.start_file_load(LoadPurpose::Dxf, FileKind::Dxf, paths);
    }
    pub(super) fn load_exported_overlay(&mut self,path:PathBuf) {
        self.start_file_load(LoadPurpose::ExportedDxf(path.clone()),FileKind::Dxf,vec![path]);
    }
    pub(super) fn import_marxact(&mut self, paths: Vec<PathBuf>) {
        let paths = self.new_paths(paths, |path| {
            self.virtual_trenches.iter().any(|t| {
                t.source_path.as_deref() == Some(path.as_path()) && !t.boundary_3d.is_empty()
            })
        });
        self.start_file_load(
            LoadPurpose::MarXact,
            FileKind::MarXact(Arc::new(
                self.migrated_settings.marxact_name_mappings.clone(),
            )),
            paths,
        );
    }
    fn new_paths(&self, paths: Vec<PathBuf>, exists: impl Fn(&PathBuf) -> bool) -> Vec<PathBuf> {
        let mut seen = BTreeSet::new();
        paths
            .into_iter()
            .filter(|path| !exists(path) && seen.insert(path.clone()))
            .collect()
    }
    pub(super) fn replace_dxf_source(&mut self, index: usize, path: PathBuf) {
        let Some(previous) = self.overlays.get(index) else {
            return;
        };
        let purpose = LoadPurpose::Reload {
            index,
            original: previous.path.clone(),
        };
        let kind = FileKind::ReloadDxf {
            visible: previous.visible,
            layers: Arc::new(
                previous
                    .layers
                    .iter()
                    .map(|(n, l)| (n.clone(), l.visible))
                    .collect(),
            ),
        };
        self.start_file_load(purpose, kind, vec![path]);
    }
    pub(super) fn poll_file_jobs(&mut self, ctx: &egui::Context) {
        let Some(ready) = self.file_loader.as_mut().and_then(FileLoader::poll) else {
            return;
        };
        let Some(intent) = self.file_intent.take() else {
            return;
        };
        if intent.ticket != ready.ticket {
            self.file_intent = Some(intent);
            return;
        }
        if intent.project_generation != self.project_generation {
            return;
        }
        match ready.result {
            Ok(outcomes) => self.apply_file_outcomes(intent.purpose, outcomes, ctx),
            Err(e) => {
                self.message = format!("Bestanden niet geladen; huidige gegevens behouden: {e}")
            }
        }
    }
    fn apply_file_outcomes(
        &mut self,
        purpose: LoadPurpose,
        outcomes: Vec<FileOutcome>,
        ctx: &egui::Context,
    ) {
        self.raster_points.mode=None;self.raster_points.rows.clear();
        self.raster_rotation=Default::default();
        if let LoadPurpose::Project(action) = purpose {
            let Some(outcome) = outcomes.into_iter().next() else {
                self.message = "Geen projectgegevens ontvangen; huidige gegevens behouden.".into();
                return;
            };
            match outcome.result {
                Ok(FileContent::Project(prepared)) => {
                    let restore = matches!(action, ProjectAction::Restore(_));
                    let imported = matches!(action, ProjectAction::ImportPython(_));
                    let path = (!restore && !imported).then_some(outcome.path.clone());
                    self.apply_project(
                        prepared.document,
                        path,
                        restore || imported,
                        prepared.markers,
                        ctx,
                    );
                    self.message = if imported {
                        "Python-project geïmporteerd. Sla op als Rust-project; bekijk de importmeldingen in de zijbalk.".into()
                    } else if restore {
                        format!(
                            "Back-up hersteld uit {}. Sla op als nieuw project.",
                            outcome.path.display()
                        )
                    } else {
                        format!("Rust-project geopend: {}", outcome.path.display())
                    };
                }
                Err(e) => {
                    self.message = format!("Project niet geladen; huidige gegevens behouden: {e}")
                }
                _ => {
                    self.message =
                        "Geen geldige projectgegevens ontvangen; huidige gegevens behouden.".into()
                }
            }
            return;
        }
        if let LoadPurpose::Reload { index, original } = purpose {
            let Some(_) = self.overlays.get(index).filter(|p| p.path == original) else {
                return;
            };
            let Some(outcome) = outcomes.into_iter().next() else {
                return;
            };
            match outcome.result {
                Ok(FileContent::Dxf(loaded)) => {
                    self.overlays[index] = *loaded;
                    self.dirty = true;
                    if self
                        .dxf_selection
                        .as_ref()
                        .is_some_and(|s| s.overlay == index)
                    {
                        self.dxf_selection = None;
                    }
                    self.message = format!(
                        "DXF opnieuw geladen: {}. Laagkeuzes zijn behouden.",
                        outcome.path.display()
                    );
                }
                Err(e) => {
                    self.message = format!("DXF niet vervangen; huidige gegevens behouden: {e}")
                }
                _ => self.message = "DXF niet vervangen; huidige gegevens behouden.".into(),
            }
            return;
        }
        let mut added = 0;
        let mut objects = 0;
        let mut errors = Vec::new();
        for outcome in outcomes {
            match (outcome.result, &purpose) {
                (Ok(FileContent::Dxf(overlay)), LoadPurpose::Dxf | LoadPurpose::ExportedDxf(_)) => {
                    if !self.overlays.iter().any(|o| o.path == overlay.path) {
                        self.overlays.push(*overlay);
                        added += 1;
                    }
                }
                (Ok(FileContent::Dataset(dataset)), LoadPurpose::Datasets) => {
                    if !self
                        .datasets
                        .iter()
                        .any(|d| d.source_path == dataset.source_path)
                    {
                        self.datasets.push(*dataset);
                        self.selected = Some(self.datasets.len() - 1);
                        added += 1;
                    }
                }
                (Ok(FileContent::GeoTiff(info)), LoadPurpose::GeoTiffs) => {
                    if !self.geotiffs.iter().any(|r| r.path == info.path) {
                        self.geotiffs.push(*info);
                        self.raster_textures.push(None);
                        added += 1;
                    }
                }
                (Ok(FileContent::MarXact(prepared)), LoadPurpose::MarXact) => {
                    if self.trench_markers.len() != self.virtual_trenches.len() {
                        self.trench_markers = self
                            .virtual_trenches
                            .iter()
                            .map(sleufbase_rust::cross_section::markers)
                            .collect();
                    }
                    added += prepared.result.trenches.len();
                    objects += prepared.result.assigned_insert_count;
                    self.virtual_trenches.extend(prepared.result.trenches);
                    self.trench_markers.extend(prepared.markers);
                }
                (Err(e), _) => errors.push(format!("{}: {e}", outcome.path.display())),
                _ => errors.push(format!(
                    "{}: onverwacht bestandsformaat",
                    outcome.path.display()
                )),
            }
        }
        if added > 0 {
            self.dirty = true;
            if matches!(purpose, LoadPurpose::GeoTiffs) {
                self.schedule_raster_previews();
            }
            if matches!(purpose, LoadPurpose::MarXact) {
                self.zoom = 1.;
                self.pan = egui::Vec2::ZERO;
            }
        }
        let summary = match purpose {
            LoadPurpose::Dxf => format!("{added} DXF-overlays geladen."),
            LoadPurpose::ExportedDxf(path) => format!("DXF opgeslagen: {}. {added} overlay(s) geladen.",path.display()),
            LoadPurpose::Datasets => format!("{added} KickTheMap-job(s) geladen."),
            LoadPurpose::GeoTiffs => format!("{added} GeoTIFF-bestanden geladen."),
            LoadPurpose::MarXact => {
                format!("MarXact: {added} proefsleuven, {objects} objecten ingelezen.")
            }
            _ => String::new(),
        };
        self.message = format!("{summary} {}", errors.join("; "));
    }
    /// Return before rendering editable controls or polling project-mutating
    /// network results. Process Cancel before applying a newly arrived result.
    pub(super) fn show_file_loading(&mut self, ctx: &egui::Context) {
        let progress = self
            .file_loader
            .as_ref()
            .and_then(FileLoader::progress)
            .map(|p| {
                (
                    p.completed,
                    p.total,
                    p.path.map(|p| p.display().to_string()),
                )
            });
        let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::CentralPanel::default().show(ctx,|ui|{
            ui.heading("Bestanden laden");
            ui.label(format!("Huidig project: {}",self.project_name));
            ui.label("De huidige gegevens blijven behouden. Annuleren voegt niets uit deze laadactie toe.");
            if let Some((done,total,path))=&progress{
                ui.add(egui::ProgressBar::new(*done as f32/(*total).max(1)as f32).text(format!("{done} van {total} bestanden verwerkt")));
                if let Some(path)=path{ui.label(path);}
            }
            ui.spinner();
            if ui.button("Annuleren").clicked(){cancel=true;}
        });
        if cancel {
            let exported=self.file_intent.as_ref().and_then(|intent|match &intent.purpose {
                LoadPurpose::ExportedDxf(path)=>Some(path.display().to_string()),_=>None
            });
            self.cancel_file_load();
            self.message = exported.map_or_else(||"Laden geannuleerd; huidige gegevens behouden.".into(),|path|format!("Overlay laden geannuleerd. DXF blijft opgeslagen: {path}"));
        } else {
            self.poll_file_jobs(ctx);
        }
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sleufbase_rust::file_loader::PreparedProject;
    fn edited() -> SleufBaseApp {
        let mut app = SleufBaseApp::default();
        app.comments.push(Comment {
            x: 1.,
            y: 2.,
            text: "keep me".into(),
        });
        app.dirty = true;
        app
    }
    #[test]
    fn cancelling_overlay_loading_keeps_the_already_published_drawing() {
        let (started_tx,started_rx)=mpsc::sync_channel(1);
        let (gate_tx,gate_rx)=mpsc::sync_channel(1);
        let (done_tx,done_rx)=mpsc::sync_channel(1);
        let mut app=edited();
        app.file_loader=Some(FileLoader::with_processor(move |_,path,_|{
            started_tx.send(()).unwrap();gate_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            let result=load_dxf(path).map(|v|FileContent::Dxf(Box::new(v)));
            done_tx.send(()).unwrap();result
        }).unwrap());
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("published.dxf");
        let dataset=sleufbase_rust::parse_job_features(r#"{"features":[{"geometry":{"type":"Point","coordinates":[1,2]}}]}"#,1,"PS1").unwrap();
        sleufbase_rust::write_dxf(&path,&[dataset],RULES).unwrap();
        let before=std::fs::read(&path).unwrap();
        app.load_exported_overlay(path.clone());started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let ctx=egui::Context::default();
        let _=ctx.run(egui::RawInput{events:vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}],..Default::default()},|ctx|app.show_file_loading(ctx));
        assert!(app.file_intent.is_none());assert!(app.message.contains("DXF blijft opgeslagen"));
        assert_eq!(std::fs::read(&path).unwrap(),before);
        gate_tx.send(()).unwrap();done_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        app.poll_file_jobs(&ctx);assert!(app.overlays.is_empty());
    }
    fn drain(app: &mut SleufBaseApp, ctx: &egui::Context) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while app.file_intent.is_some() {
            app.poll_file_jobs(ctx);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn escape_renders_and_cancels_while_a_project_reader_is_actually_running() {
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (gate_tx, gate_rx) = mpsc::sync_channel(1);
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        let mut app = edited();
        app.file_loader = Some(
            FileLoader::with_processor(move |_, _, _| {
                started_tx.send(()).unwrap();
                gate_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                done_tx.send(()).unwrap();
                Ok(FileContent::Project(Box::new(PreparedProject {
                    document: NativeProject::default(),
                    markers: vec![],
                })))
            })
            .unwrap(),
        );
        let ctx = egui::Context::default();
        app.execute_project_action(ProjectAction::Open("slow.json".into()), &ctx);
        started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let output = ctx.run(egui::RawInput::default(), |ctx| app.show_file_loading(ctx));
        assert!(output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.job.text.contains("Bestanden laden"))));
        assert!(app.file_intent.is_some());
        assert_eq!(app.comments[0].text, "keep me");
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
                ..Default::default()
            },
            |ctx| app.show_file_loading(ctx),
        );
        assert!(app.file_intent.is_none());
        assert_eq!(app.comments[0].text, "keep me");
        assert!(app.dirty);
        gate_tx.send(()).unwrap();
        done_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        app.poll_file_jobs(&ctx);
        assert_eq!(app.comments[0].text, "keep me");
        assert!(app.project_path.is_none());
        assert!(app.dirty);
    }
    #[test]
    fn a_new_project_cannot_receive_a_late_opened_document() {
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (gate_tx, gate_rx) = mpsc::sync_channel(1);
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        let mut app = edited();
        app.file_loader = Some(
            FileLoader::with_processor(move |_, _, _| {
                started_tx.send(()).unwrap();
                gate_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                done_tx.send(()).unwrap();
                let document = NativeProject {
                    name: "OLD RESULT".into(),
                    ..Default::default()
                };
                Ok(FileContent::Project(Box::new(PreparedProject {
                    document,
                    markers: vec![],
                })))
            })
            .unwrap(),
        );
        let ctx = egui::Context::default();
        app.execute_project_action(ProjectAction::Open("old.json".into()), &ctx);
        started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let old = app.project_generation;
        app.execute_project_action(ProjectAction::New, &ctx);
        assert_ne!(app.project_generation, old);
        gate_tx.send(()).unwrap();
        done_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        app.poll_file_jobs(&ctx);
        assert_eq!(app.project_name, "Naamloos project");
        assert!(app.comments.is_empty());
        assert!(!app.dirty);
        assert!(app.project_path.is_none());
    }
    #[test]
    fn mixed_file_results_keep_successes_and_use_prepared_marxact_markers() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let mut app = SleufBaseApp::default();
        let ctx = egui::Context::default();
        let path = root.join("splines/splines-binary.dxf");
        let missing = root.join("missing.dxf");
        app.load_overlays(vec![path.clone(), path, missing]);
        assert!(app.overlays.is_empty());
        assert!(!app.dirty);
        assert_eq!(
            app.file_loader.as_ref().unwrap().progress().unwrap().total,
            2
        );
        drain(&mut app, &ctx);
        assert_eq!(app.overlays.len(), 1);
        assert!(app.dirty);
        assert!(app.message.contains("missing.dxf"));
        app.import_marxact(vec![root.join("marxact-cad/nested-leaders.dxf")]);
        drain(&mut app, &ctx);
        assert!(!app.virtual_trenches.is_empty());
        assert_eq!(app.trench_markers.len(), app.virtual_trenches.len());
        assert!(app.trench_markers.iter().any(|m| !m.is_empty()));
    }
}
