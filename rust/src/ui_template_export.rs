//! Prepared-template DXF export dialog for the native desktop preview.
//! This shares the same streaming/transactional Rust exporter as --template-dxf.
//! It deliberately does not claim to migrate the full Python slot editor yet.
use super::*;
use sleufbase_rust::{export_control::{Control, Stage}, template_export};
use std::sync::{Arc, mpsc::{self, TryRecvError}};

#[derive(Default)]
pub(super) struct TemplateDialog {
    manifest: Option<PathBuf>,
    output: Option<PathBuf>,
}

enum Event {
    Progress(String),
    Finished(Result<(usize, usize, usize), String>),
    ProjectFinished(
        Result<(usize, usize, usize), String>,
        sleufbase_rust::export_worker::Inputs,
        Vec<sleufbase_rust::dxf_read::DxfOverlay>,
    ),
}

pub(super) struct TemplateJob {
    control: Arc<Control>,
    events: Receiver<Event>,
    output: PathBuf,
    status: String,
    queued_action: Option<ProjectAction>,
}

impl SleufBaseApp {
    pub(super) fn open_template_dialog(&mut self) {
        if self.export_intent.is_some() || self.file_intent.is_some() || self.template_job.is_some() {
            self.message = "Rond eerst de huidige laad- of exporttaak af".into();
            return;
        }
        self.template_dialog = Some(TemplateDialog::default());
    }

    pub(super) fn show_template_dialog(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut launch = None;
        if let Some(dialog) = &mut self.template_dialog {
            egui::Window::new("Sjabloon-DXF vanuit Rust (preview)")
                .open(&mut open)
                .default_width(490.)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Gebruik een voorbereid export-JSON met de sjabloonvakken, TIFF-/kaartbronnen en profielinstellingen.");
                    ui.label("Dit is nog geen automatische export vanuit het geopende Rust-project.");
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Exportinvoer JSON kiezen…").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Exportinvoer JSON", &["json"]).pick_file() {
                                dialog.manifest = Some(path);
                            }
                        }
                    });
                    if let Some(path) = &dialog.manifest {
                        ui.label(path.display().to_string());
                    } else { ui.small("Nog geen voorbereide exportinvoer geselecteerd"); }
                    ui.horizontal(|ui| {
                        if ui.button("Uitvoer-DXF kiezen…").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("AutoCAD DXF", &["dxf"])
                                .set_file_name("SleufBase-Sjabloon-Rust.dxf").save_file() {
                                dialog.output = Some(path);
                            }
                        }
                    });
                    if let Some(path) = &dialog.output {
                        ui.label(path.display().to_string());
                    } else { ui.small("Nog geen uitvoerbestand geselecteerd"); }
                    ui.separator();
                    ui.label("De export loopt op een achtergrondthread. Bestaande uitvoer en rasterbundels worden niet overschreven.");
                    let ready = dialog.manifest.is_some() && dialog.output.is_some();
                    if ui.add_enabled(ready, egui::Button::new("Sjabloonexport starten")).clicked() {
                        launch = Some((dialog.manifest.clone().unwrap(), dialog.output.clone().unwrap()));
                    }
                });
        }
        if !open { self.template_dialog = None; }
        if let Some((manifest, output)) = launch {
            self.start_template_export(manifest, output);
        }
    }

    pub(super) fn start_project_template_export(
        &mut self,
        options: sleufbase_rust::template_project::Options,
        output: PathBuf,
    ) {
        if self.export_intent.is_some() || self.file_intent.is_some()
            || self.template_job.is_some() || self.pending_project_action.is_some()
        {
            self.message = "Er is al een lopende taak of projectkeuze".into();
            return;
        }
        if !options.template.is_file() ||
            output.extension().is_none_or(|ext| !ext.to_string_lossy().eq_ignore_ascii_case("dxf"))
        {
            self.message = "Kies een geldig DXF-sjabloon en een uitvoer-DXF".into();
            return;
        }
        if output.exists() {
            self.message = "Het uitvoerbestand bestaat al; kies een andere naam".into();
            return;
        }
        if options.choices.iter().filter(|choice|choice.include).count() == 0 {
            self.message = "Selecteer minimaal één proefsleufvak".into();
            return;
        }
        // Move the large datasets instead of deep-cloning every feature and
        // geometry. They are returned by the worker on both success and error.
        let inputs = self.take_export_inputs();
        let owned = Arc::new(std::sync::Mutex::new(Some(inputs)));
        let cad_owned = Arc::new(std::sync::Mutex::new(Some(
            std::mem::take(&mut self.overlays)
        )));
        let worker_cad = Arc::clone(&cad_owned);
        let worker_owned = Arc::clone(&owned);
        let rasters = self.geotiffs.clone();
        let rules = self.migrated_settings.object_rules.clone();
        let (tx, rx) = mpsc::channel();
        let control = Arc::new(Control::default());
        let task_control = Arc::clone(&control);
        let task_output = output.clone();
        let launch = std::thread::Builder::new()
            .name("sleufbase-project-template".into())
            .spawn(move || {
                let inputs = worker_owned.lock().expect("project export ownership lock")
                    .take().expect("project inputs already owned by another worker");
                let cad_layers=worker_cad.lock().expect("CAD export ownership lock")
                    .take().expect("CAD overlays already owned by another worker");
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let request = sleufbase_rust::template_project::build_with_cad(
                        &options, &inputs.datasets, &rasters, &inputs.trenches,
                        &inputs.bgt, inputs.cadastral.as_ref(), &cad_layers,
                        &rules, &task_control,
                    )?;
                    template_export::export_with_progress(
                        &task_output, &request, &task_control,
                        |message| { let _ = tx.send(Event::Progress(message.to_owned())); }
                    ).map(|v| (v.filled_slots, v.profiles, v.added_pages))
                })).unwrap_or_else(|_|Err(
                    "Project-sjabloonexport is onverwacht gestopt; projectgegevens worden hersteld".into()
                ));
                let _ = tx.send(Event::ProjectFinished(outcome, inputs, cad_layers));
            });
        match launch {
            Ok(_) => {
                self.template_job = Some(TemplateJob {
                    control, events: rx, output,
                    status: "Projectbronnen en GeoTIFF-koppelingen voorbereiden…".into(),
                    queued_action: None,
                });
                self.project_template_dialog = None;
                self.message = "Projectgebonden Rust-sjabloonexport gestart".into();
            }
            Err(error) => {
                if let Ok(mut guard) = owned.lock() {
                    if let Some(inputs) = guard.take() { self.restore_export_inputs(inputs); }
                }
                if let Ok(mut guard) = cad_owned.lock() {
                    if let Some(overlays) = guard.take() { self.overlays = overlays; }
                }
                self.message = format!("Exportworker kan niet starten: {error}");
            }
        }
    }

    pub(super) fn start_template_export(&mut self, manifest: PathBuf, output: PathBuf) {
        if self.export_intent.is_some() || self.file_intent.is_some() ||
           self.template_job.is_some() || self.pending_project_action.is_some() {
            self.message = "Er loopt nog een taak of er staat een projectkeuze open".into();
            return;
        }
        if !manifest.is_file() {
            self.message = "De voorbereide JSON-exportinvoer bestaat niet".into();
            return;
        }
        if output.extension().is_none_or(|ext| !ext.to_string_lossy().eq_ignore_ascii_case("dxf")) {
            self.message = "Kies een uitvoerbestand met de extensie .dxf".into();
            return;
        }
        if output.exists() {
            self.message = "Uitvoerbestand bestaat al; kies een nieuwe DXF-naam".into();
            return;
        }
        if manifest.canonicalize().ok() == output.canonicalize().ok() &&
            manifest.canonicalize().is_ok() {
            self.message = "Uitvoer mag niet hetzelfde bestand als de invoer zijn".into();
            return;
        }
        let (tx, rx) = mpsc::channel();
        let control = Arc::new(Control::default());
        let task_control = Arc::clone(&control);
        let task_output = output.clone();
        let result = std::thread::Builder::new()
            .name("sleufbase-template-export".into())
            .spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let request = template_export::read_request(&manifest, &task_control)?;
                    template_export::export_with_progress(&task_output, &request, &task_control,
                        |message| { let _ = tx.send(Event::Progress(message.to_owned())); })
                    .map(|result| (result.filled_slots, result.profiles, result.added_pages))
                })).unwrap_or_else(|_| Err("Sjabloonexport is onverwacht gestopt; uitvoer is niet gepubliceerd".into()));
                let _ = tx.send(Event::Finished(outcome));
            });
        match result {
            Ok(_) => {
                self.template_job = Some(TemplateJob {
                    control, events: rx, output, status: "Exportinvoer controleren…".into(),
                    queued_action: None,
                });
                self.template_dialog = None;
                self.message = "Native Rust-sjabloonexport gestart".into();
            }
            Err(err) => {
                self.message = format!("Sjabloonexportthread kon niet starten: {err}");
            }
        }
    }

    pub(super) fn defer_action_for_template(&mut self, action: &ProjectAction) -> bool {
        if let Some(job) = &mut self.template_job {
            job.queued_action = Some(action.clone());
            job.control.cancel();
            true
        } else { false }
    }

    pub(super) fn poll_template_job(&mut self, ctx: &egui::Context) {
        let Some(job) = &mut self.template_job else { return };
        let mut finished = None;
        let mut return_inputs = None;
        let mut return_cad = None;
        loop {
            match job.events.try_recv() {
                Ok(Event::Progress(status)) => job.status = status,
                Ok(Event::Finished(result)) => { finished = Some(result); break; }
                Ok(Event::ProjectFinished(result, inputs, cad)) => {
                    return_inputs = Some(inputs);
                    return_cad = Some(cad);
                    finished = Some(result);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    finished = Some(Err("Sjabloonexportthread zonder resultaat gestopt".into()));
                    break;
                }
            }
        }
        if let Some(result) = finished {
            if let Some(inputs) = return_inputs { self.restore_export_inputs(inputs); }
            if let Some(overlays) = return_cad { self.overlays = overlays; }
            let job = self.template_job.take().unwrap();
            self.message = match result {
                Ok((slots, profiles, pages)) =>
                    format!("Sjabloon-DXF opgeslagen: {} ({slots} vakken, {profiles} profielen, {pages} extra bladen)", job.output.display()),
                Err(error) => format!("Sjabloonexport niet opgeslagen: {error}"),
            };
            if let Some(action) = job.queued_action {
                self.request_project_action(action, ctx);
            }
        }
    }

    pub(super) fn show_template_progress(&mut self, ctx: &egui::Context) {
        let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let Some(job) = &self.template_job else { return };
        let (stage, bytes) = job.control.progress();
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Rust-sjabloonexport");
            ui.label(job.output.display().to_string());
            ui.label(job.status.as_str());
            ui.label(match stage {
                Stage::Preparing => "Invoer en sjabloon voorbereiden",
                Stage::Planning => "Vakken, profielen en kaartbeelden samenstellen",
                Stage::Writing => "DXF-bestand schrijven",
                Stage::Publishing => "DXF en rasterbundel definitief publiceren",
            });
            if bytes > 0 {
                ui.label(format!("{:.1} MiB DXF geschreven", bytes as f64 / 1_048_576.));
            }
            if ui.add_enabled(job.control.can_cancel(), egui::Button::new("Annuleren")).clicked() {
                cancel = true;
            }
            ui.label("De bestaande DXF wordt niet overschreven. Bij annuleren blijft de vorige export behouden.");
            ui.spinner();
        });
        if cancel {
            if let Some(job) = &self.template_job { job.control.cancel(); }
        }
        self.poll_template_job(ctx);
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn existing_output_is_never_overwritten_by_desktop_template_action() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("input.json");
        let output = dir.path().join("existing.dxf");
        std::fs::write(&manifest, "{}").unwrap();
        std::fs::write(&output, b"keep original").unwrap();
        let mut app = SleufBaseApp::default();
        app.start_template_export(manifest, output.clone());
        assert!(app.template_job.is_none());
        assert_eq!(std::fs::read(&output).unwrap(), b"keep original");
    }
    #[test]
    fn broken_manifest_reports_error_without_touching_original_project() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("invalid.json");
        let output = dir.path().join("new.dxf");
        std::fs::write(&manifest, "{not-json").unwrap();
        let mut app = SleufBaseApp::default();
        app.comments.push(Comment { x: 1., y: 2., text: "unchanged".into() });
        app.dirty = true;
        app.start_template_export(manifest, output.clone());
        assert!(app.template_job.is_some());
        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.template_job.is_some() {
            app.poll_template_job(&ctx);
            assert!(Instant::now() < deadline, "template worker did not finish");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(!output.exists());
        assert!(app.message.contains("niet opgeslagen"));
        assert_eq!(app.comments[0].text, "unchanged");
        assert!(app.dirty);
    }
    #[test]
    fn desktop_template_button_runs_real_native_image_only_export() {
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/template-export/reference.json"
        )).unwrap();
        let mut request = cases.as_array().unwrap().iter()
            .find(|v| v["name"] == "image-only").unwrap()["request"].clone();
        let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let absolute = |name: &str| repository_root.join(name).to_string_lossy().into_owned();
        let template = request["template"].as_str().unwrap().to_owned();
        request["template"] = absolute(&template).into();
        for slot in request["slots"].as_array_mut().unwrap() {
            if slot.is_null() { continue; }
            for key in ["tiff_png", "map_png"] {
                let path = slot[key].as_str().unwrap().to_owned();
                slot[key] = absolute(&path).into();
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("prepared.json");
        let output = dir.path().join("native.dxf");
        std::fs::write(&input, serde_json::to_vec(&request).unwrap()).unwrap();
        let mut app = SleufBaseApp::default();
        app.comments.push(Comment { x: 5., y: 10., text: "retained".into() });
        app.start_template_export(input.clone(), output.clone());
        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(40);
        while app.template_job.is_some() {
            app.poll_template_job(&ctx);
            assert!(Instant::now() < deadline, "native export took too long");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(output.is_file(), "{}", app.message);
        assert!(app.message.contains("2 vakken"), "{}", app.message);
        assert_eq!(app.comments[0].text, "retained");
        assert_eq!(std::fs::read(&input).unwrap(), serde_json::to_vec(&request).unwrap());
        let doc = sleufbase_rust::dxf_document::Document::load(
            &output, &std::sync::atomic::AtomicBool::new(false)).unwrap();
        assert!(doc.records().any(|record| record.kind() == "IMAGEDEF"));
    }
    #[test]
    fn project_bound_worker_restores_owned_datasets_when_mapping_fails() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = SleufBaseApp::default();
        app.datasets.push(sleufbase_rust::parse_job_features(
            include_str!("../tests/fixtures/jobFeatures.json"), 42, "PS42").unwrap());
        let expected = serde_json::to_value(&app.datasets).unwrap();
        let ptr = app.datasets[0].features.as_ptr();
        app.dirty = true;
        let template = dir.path().join("template.dxf");
        std::fs::write(&template, b"not needed for validation").unwrap();
        let opts = sleufbase_rust::template_project::Options {
            template,
            choices: sleufbase_rust::template_project::suggest(&app.datasets, &[], &[]),
            ..Default::default()
        };
        app.start_project_template_export(opts, dir.path().join("never.dxf"));
        assert!(app.datasets.is_empty(), "inputs must be owned by the export worker");
        assert!(app.template_job.is_some());
        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.template_job.is_some() {
            app.poll_template_job(&ctx);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(serde_json::to_value(&app.datasets).unwrap(), expected);
        assert_eq!(app.datasets[0].features.as_ptr(), ptr);
        assert!(app.dirty);
        assert!(app.message.contains("niet opgeslagen"));
        assert!(!dir.path().join("never.dxf").exists());
    }
    #[test]
    fn project_action_is_deferred_and_cancels_template_job() {
        let mut app = SleufBaseApp::default();
        let (tx, rx) = mpsc::channel();
        let control = Arc::new(Control::default());
        app.template_job = Some(TemplateJob {
            control: Arc::clone(&control),
            events: rx,
            output: PathBuf::from("unused.dxf"),
            status: String::new(),
            queued_action: None,
        });
        assert!(app.defer_action_for_template(&ProjectAction::New));
        assert!(control.is_cancelled());
        tx.send(Event::Finished(Err("Export geannuleerd".into()))).unwrap();
        app.poll_template_job(&egui::Context::default());
        assert!(app.template_job.is_none());
        assert_eq!(app.project_name, "Naamloos project");
    }
}
