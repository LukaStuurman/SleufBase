//! Responsive export screen; the worker owns the unchanged inputs until return.
use super::*;
use sleufbase_rust::{
    export_control::Stage,
    export_worker::{Exporter, Inputs, Kind, Request},
};
pub(super) struct ExportIntent {
    path: PathBuf,
    kind: Kind,
    overlay: bool,
    queued_action: Option<ProjectAction>,
}
pub(super) fn report_notice(companion: Option<&std::path::Path>, warnings: &[String]) -> String {
    let mut text = companion.map_or_else(String::new, |p| format!(". Excel: {}", p.display()));
    for warning in warnings.iter().take(10) {
        text.push('\n');
        text.push_str(warning);
    }
    if warnings.len() > 10 {
        text.push_str(&format!("\nEn {} overige meldingen", warnings.len() - 10));
    }
    text
}

impl SleufBaseApp {
    pub(super) fn open_template_export(&mut self) {
        if self.export_intent.is_some()
            || self.template_job.is_some()
            || self.file_intent.is_some()
            || self.pending_project_action.is_some()
        {
            self.message = "Rond de lopende taak eerst af".into();
            return;
        }
        if self.geotiffs.is_empty() {
            self.message = "Laad eerst GeoTIFF-proefsleuven voor de sjabloonexport".into();
            return;
        }
        let state =
            self.template_state
                .clone()
                .unwrap_or_else(|| sleufbase_rust::template_order::State {
                    project: self.project_name.clone(),
                    auto_bgt: self
                        .migrated_settings
                        .template_auto_fill_bgt_fysiek_voorkomen,
                    label_gap: self.migrated_settings.dxf_label_gap,
                    ..Default::default()
                });
        self.template_dialog = None;
        self.project_template_dialog = None;
        self.template_order_dialog = Some(crate::template_dialog::Dialog::new(
            state,
            &self.geotiffs,
            self.project_generation,
        ));
    }
    pub(super) fn finish_template_export(
        &mut self,
        path: PathBuf,
        state: sleufbase_rust::template_order::State,
    ) {
        let state = match state.confirmed() {
            Ok(state) => state,
            Err(e) => {
                if let Some(d) = &mut self.template_order_dialog {
                    d.error = e;
                }
                return;
            }
        };
        let request = Request {
            kind: Kind::Template,
            path,
            selection: sleufbase_rust::object_selection::Selection::all(0, 0),
            rules: self.migrated_settings.object_rules.clone(),
            extra_layers: false,
            options: self.migrated_settings.object_dxf_options(),
            template: Some(Box::new(state.clone())),
            discipline_excel: self.migrated_settings.template_auto_export_discipline_excel,
        };
        if self.start_export(request, false) {
            self.template_state = Some(state);
            self.dirty = true;
        }
    }
    pub(super) fn take_export_inputs(&mut self) -> Inputs {
        Inputs {
            datasets: std::mem::take(&mut self.datasets),
            trenches: std::mem::take(&mut self.virtual_trenches),
            cadastral: self.cadastral.take(),
            bgt: std::mem::take(&mut self.bgt_layers),
            rasters: std::mem::take(&mut self.geotiffs),
            overlays: std::mem::take(&mut self.overlays),
            comments: std::mem::take(&mut self.comments),
            protected_paths: self
                .project_path
                .iter()
                .cloned()
                .chain(self.legacy_archive.iter().map(|a| a.source_path.clone()))
                .collect(),
        }
    }
    pub(super) fn restore_export_inputs(&mut self, inputs: Inputs) {
        debug_assert!(
            self.datasets.is_empty()
                && self.virtual_trenches.is_empty()
                && self.cadastral.is_none()
                && self.bgt_layers.is_empty()
                && self.geotiffs.is_empty()
                && self.overlays.is_empty()
        );
        self.datasets = inputs.datasets;
        self.virtual_trenches = inputs.trenches;
        self.cadastral = inputs.cadastral;
        self.bgt_layers = inputs.bgt;
        self.geotiffs = inputs.rasters;
        self.overlays = inputs.overlays;
        self.comments = inputs.comments;
    }
    fn start_export(&mut self, request: Request, overlay: bool) -> bool {
        if self.export_intent.is_some() || self.template_job.is_some() {
            self.message = "Er loopt al een export".into();
            return false;
        }
        if self.file_intent.is_some() {
            self.message = "Wacht tot het laden is afgerond voordat je exporteert".into();
            return false;
        }
        if self.pending_project_action.is_some() {
            self.message = "Rond eerst de projectkeuze af".into();
            return false;
        }
        if self.exporter.as_ref().is_none_or(Exporter::is_closed) {
            match Exporter::new() {
                Ok(worker) => self.exporter = Some(worker),
                Err(error) => {
                    self.message = error;
                    return false;
                }
            }
        }
        let path = request.path.clone();
        let kind = request.kind;
        let inputs = self.take_export_inputs();
        match self.exporter.as_mut().unwrap().start(inputs, request) {
            Ok(()) => {
                self.export_intent = Some(ExportIntent {
                    path,
                    kind,
                    overlay,
                    queued_action: None,
                });
                self.message = "Export wordt voorbereid…".into();
                true
            }
            Err(error) => {
                let failure = *error;
                self.restore_export_inputs(failure.inputs);
                self.message = failure.message;
                false
            }
        }
    }
    pub(super) fn start_excel_export(&mut self, path: PathBuf) {
        let request = Request {
            kind: Kind::Xlsx,
            path,
            selection: sleufbase_rust::object_selection::Selection::all(
                self.datasets.len(),
                self.virtual_trenches.len(),
            ),
            rules: self.migrated_settings.object_rules.clone(),
            extra_layers: false,
            options: self.migrated_settings.object_dxf_options(),
            template: None,
            discipline_excel: false,
        };
        self.start_export(request, false);
    }
    pub(super) fn finish_dxf_export(
        &mut self,
        path: PathBuf,
        action: crate::export_dialog::Action,
    ) {
        let Some(dialog) = self.export_dialog.as_ref() else {
            return;
        };
        let request = Request {
            kind: Kind::Dxf,
            path,
            selection: dialog.selection.clone(),
            rules: self.migrated_settings.object_rules.clone(),
            extra_layers: dialog.extra_layers,
            options: dialog.options(),
            template: None,
            discipline_excel: false,
        };
        self.start_export(request, action.overlay);
    }
    pub(super) fn defer_action_for_export(&mut self, action: &ProjectAction) -> bool {
        if let Some(intent) = &mut self.export_intent {
            intent.queued_action = Some(action.clone());
            if let Some(worker) = &self.exporter {
                worker.cancel();
            }
            true
        } else {
            false
        }
    }
    pub(super) fn poll_export_jobs(&mut self, ctx: &egui::Context) {
        let Some(ready) = self.exporter.as_mut().and_then(Exporter::poll) else {
            return;
        };
        self.restore_export_inputs(ready.inputs);
        let Some(intent) = self.export_intent.take() else {
            self.message = "Projectgegevens van export hersteld".into();
            return;
        };
        match ready.result {
            Ok(exported) => {
                if intent.kind == Kind::Dxf {
                    self.export_dialog = None;
                }
                if intent.kind == Kind::Template {
                    self.template_order_dialog = None;
                }
                if intent.overlay && intent.queued_action.is_none() {
                    self.load_exported_overlay(intent.path.clone());
                }
                let skipped = if exported.skipped.is_empty() {
                    String::new()
                } else {
                    format!(
                        ". Zonder objecten overgeslagen: {}",
                        exported.skipped.join(", ")
                    )
                };
                self.message = format!(
                    "Export opgeslagen: {}{skipped}{}",
                    intent.path.display(),
                    report_notice(exported.companion.as_deref(), &exported.warnings)
                );
            }
            Err(error) => {
                self.message = format!("Export niet opgeslagen; projectgegevens behouden: {error}");
                if intent.kind == Kind::Dxf {
                    if let Some(dialog) = self.export_dialog.as_mut() {
                        dialog.error = error.clone();
                    }
                }
                if intent.kind == Kind::Template {
                    if let Some(dialog) = self.template_order_dialog.as_mut() {
                        dialog.error = error;
                    }
                }
            }
        }
        if let Some(action) = intent.queued_action {
            self.export_dialog = None;
            self.template_order_dialog = None;
            self.request_project_action(action, ctx);
        }
    }
    pub(super) fn show_export_progress(&mut self, ctx: &egui::Context) {
        let progress = self
            .exporter
            .as_ref()
            .and_then(Exporter::control)
            .map(|c| (c.progress(), c.can_cancel(), c.is_cancelled()));
        let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Exporteren");
            ui.label(format!("Project: {}", self.project_name));
            if let Some(intent) = &self.export_intent {
                ui.label(intent.path.display().to_string());
            }
            if let Some(((stage, bytes), can_cancel, cancelled)) = progress {
                ui.label(if cancelled {
                    "Annuleren wordt afgerond…"
                } else {
                    match stage {
                        Stage::Preparing => "Objecten verzamelen…",
                        Stage::Planning => "Geometrie en rapport voorbereiden…",
                        Stage::Writing => "Bestand schrijven…",
                        Stage::Publishing => "Bestand definitief opslaan…",
                    }
                });
                let detail = self
                    .exporter
                    .as_ref()
                    .and_then(Exporter::control)
                    .map(|c| c.detail())
                    .unwrap_or_default();
                if !detail.is_empty() {
                    ui.label(detail);
                }
                if bytes > 0 {
                    ui.label(format!(
                        "{:.1} MiB geschreven",
                        bytes as f64 / (1024. * 1024.)
                    ));
                }
                if ui
                    .add_enabled(can_cancel, egui::Button::new("Annuleren"))
                    .clicked()
                {
                    cancel = true;
                }
            }
            ui.label(
                "Projectgegevens blijven behouden. Wacht op afronding voordat je verder werkt.",
            );
            ui.spinner();
        });
        // Cancel wins before consuming a ready result if publication hasn't begun.
        if cancel {
            if let Some(worker) = &self.exporter {
                worker.cancel();
            }
        }
        self.poll_export_jobs(ctx);
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    fn template_app() -> SleufBaseApp {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/template-export/reference.json"
        ))
        .unwrap();
        let r: sleufbase_rust::template_export::Request = serde_json::from_value(
            cases
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["name"] == "native-tiff")
                .unwrap()["request"]
                .clone(),
        )
        .unwrap();
        let source = r.slots[0].as_ref().unwrap().tiff_source.as_ref().unwrap();
        let path = root.join("..").join(&source.path);
        let pixels = sleufbase_rust::raster::decode_full_with_cancel(
            &path,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        SleufBaseApp {
            datasets: r.datasets,
            migrated_settings: MigratedSettings::default(),
            geotiffs: vec![GeoTiffInfo {
                path,
                width: pixels.width,
                height: pixels.height,
                transform: source.transform,
                bounds: source
                    .transform
                    .bounds(pixels.width, pixels.height)
                    .unwrap(),
                epsg: Some(28992),
                kickthemap_job_id: Some(1),
                kickthemap_job_title: "PS1".into(),
                kickthemap_job_sidecar: None,
            }],
            raster_textures: vec![None],
            raster_errors: vec![None],
            ..Default::default()
        }
    }
    #[test]
    fn project_export_never_retargets_reviewed_slots_after_source_lists_change() {
        for change in 0..3 {
            let mut app = template_app();
            app.open_project_template_dialog();
            assert!(app.project_template_dialog.is_some());
            match change {
                0 => app.datasets.swap(0, 1),
                1 => app.geotiffs[0].kickthemap_job_id = Some(999),
                _ => app.virtual_trenches.push(
                    VirtualTrench::new(
                        "Nieuwe sleuf".into(),
                        Vertex {
                            x: 0.,
                            y: 0.,
                            z: None,
                        },
                        Vertex {
                            x: 10.,
                            y: 0.,
                            z: None,
                        },
                        0.6,
                    )
                    .unwrap(),
                ),
            }
            let ctx = egui::Context::default();
            let _ = ctx.run(Default::default(), |ctx| {
                app.show_project_template_dialog(ctx)
            });
            assert!(app.project_template_dialog.is_none());
            assert!(app.template_job.is_none() && app.export_intent.is_none());
            assert!(app.message.contains("Projectbronnen gewijzigd"));
        }
    }
    #[test]
    fn project_template_worker_moves_rasters_cad_comments_and_blocks_all_other_exports() {
        use sleufbase_rust::template_project::{BackgroundKind, Choice, Options, Source};
        let mut app = template_app();
        app.comments.push(Comment {
            x: 1.,
            y: 2.,
            text: "Behouden".into(),
        });
        app.overlays.push(DxfOverlay {
            visible: true,
            ..Default::default()
        });
        let pointers = (
            app.datasets[0].features.as_ptr(),
            app.geotiffs.as_ptr(),
            app.overlays.as_ptr(),
            app.comments.as_ptr(),
        );
        app.open_template_export();
        app.open_project_template_dialog();
        assert!(app.template_order_dialog.is_none());
        assert!(app.project_template_dialog.is_some());
        let options = Options {
            template: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../assets/cadastral_template.dxf"),
            project: "Gecombineerd project".into(),
            dpi: 72,
            background: BackgroundKind::Blank,
            include_bgt: false,
            include_cadastral: false,
            include_cad: true,
            choices: vec![Choice {
                source: Source::Job(0),
                raster: Some(0),
                include: true,
                title: "PS12".into(),
                profile: false,
                reverse: false,
                comments: String::new(),
                photos: Vec::new(),
            }],
            ..Default::default()
        };
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("combined.dxf");
        app.start_project_template_export(options, out.clone());
        assert!(app.template_job.is_some());
        assert!(
            app.datasets.is_empty()
                && app.geotiffs.is_empty()
                && app.overlays.is_empty()
                && app.comments.is_empty()
        );
        assert!(!app.save_project_to_path(dir.path().join("empty.json")));
        let ctx = egui::Context::default();
        app.start_excel_export(dir.path().join("blocked.xlsx"));
        app.open_template_export();
        assert!(app.export_intent.is_none() && app.template_order_dialog.is_none());
        let deadline = Instant::now() + Duration::from_secs(45);
        while app.template_job.is_some() {
            app.poll_template_job(&ctx);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(out.exists(), "{}", app.message);
        let drawing=sleufbase_rust::dxf_document::Document::load(&out,&std::sync::atomic::AtomicBool::new(false)).unwrap();
        assert_eq!(drawing.records().filter(|r|r.kind()=="BLOCKVISIBILITYPARAMETER" && r.first(301)==Some("Versie")).count(),1);
        assert!(app.template_preferences.as_ref().unwrap().paired);

        assert!(out.with_extension("xlsx").exists(), "{}", app.message);
        assert!(app.message.contains("Excel:"));
        assert_eq!(
            (
                app.datasets[0].features.as_ptr(),
                app.geotiffs.as_ptr(),
                app.overlays.as_ptr(),
                app.comments.as_ptr()
            ),
            pointers
        );
        assert_eq!(
            app.snapshot().template_preferences.unwrap().project,
            "Gecombineerd project"
        );
        assert!(app.save_project_to_path(dir.path().join("restored.json")));
        app.open_template_export();
        assert!(app.template_order_dialog.is_some() && app.project_template_dialog.is_none());
    }
    #[test]
    fn native_template_window_worker_snapshot_and_project_switch_keep_complete_sources() {
        let mut app = template_app();
        let ctx = egui::Context::default();
        app.open_template_export();
        let mut state = app.template_order_dialog.as_ref().unwrap().state.clone();
        assert!(state.paired);
        state.project = "  Uitvoerproject Ø  ".into();
        state.aerial = false;
        state.auto_bgt = false;
        let pointers = (app.datasets[0].features.as_ptr(), app.geotiffs.as_ptr());
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("native-window.dxf");
        let export_day=sleufbase_rust::template_metadata::Date::today().unwrap().to_string();
        app.finish_template_export(output.clone(), state);
        assert!(app.datasets.is_empty() && app.geotiffs.is_empty());
        assert!(!app.save_project_to_path(temp.path().join("empty.json")));
        let deadline = Instant::now() + Duration::from_secs(45);
        while app.export_intent.is_some() {
            app.poll_export_jobs(&ctx);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(output.exists(), "{}", app.message);
        let drawing=sleufbase_rust::dxf_document::Document::load(&output,&std::sync::atomic::AtomicBool::new(false)).unwrap();
        assert_eq!(drawing.records().filter(|r|r.kind()=="BLOCKVISIBILITYPARAMETER" && r.first(301)==Some("Versie")).count(),1);
        let today=sleufbase_rust::template_metadata::Date::today().unwrap().to_string();
        assert!(drawing.records().any(|r|r.kind()=="ATTRIB" && matches!(r.first(2),Some("DATUM1"|"DATUM")) && (r.first(1)==Some(today.as_str()) || r.first(1)==Some(export_day.as_str()))));
        assert!(drawing.records().any(|r|r.kind()=="ATTRIB" && r.first(1)==Some("Uitvoerproject Ø")));

        assert!(output.with_extension("xlsx").exists(), "{}", app.message);
        app.migrated_settings.template_auto_export_discipline_excel = false;
        let without_excel = temp.path().join("disabled-report.dxf");
        app.finish_template_export(without_excel.clone(), app.template_state.clone().unwrap());
        while app.export_intent.is_some() {
            app.poll_export_jobs(&ctx);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(without_excel.exists(), "{}", app.message);
        assert!(!without_excel.with_extension("xlsx").exists());
        assert!(app.template_order_dialog.is_none());
        assert_eq!(
            (app.datasets[0].features.as_ptr(), app.geotiffs.as_ptr()),
            pointers
        );
        assert_eq!(
            app.snapshot().template_export.unwrap().project,
            "Uitvoerproject Ø"
        );
        app.open_template_export();
        assert!(app.template_order_dialog.is_some());
        app.request_project_action(ProjectAction::New, &ctx);
        assert!(app.pending_project_action.is_some());
        app.finish_project_action(false, &ctx);
        assert_eq!(app.geotiffs.len(), 1);
        assert_eq!(
            app.template_state.as_ref().unwrap().project,
            "Uitvoerproject Ø"
        );
    }
    #[test]
    fn template_cancellation_defers_close_until_the_rasters_return_and_rejected_start_keeps_metadata(
    ) {
        let mut app = template_app();
        let (tx, rx) = mpsc::sync_channel(1);
        let (gate, waiting) = mpsc::sync_channel(1);
        app.exporter = Some(
            Exporter::with_processor(move |inputs, request, control| {
                tx.send(()).unwrap();
                waiting.recv_timeout(Duration::from_secs(10)).unwrap();
                sleufbase_rust::export_worker::process(inputs, request, control)
            })
            .unwrap(),
        );
        app.open_template_export();
        let state = app.template_order_dialog.as_ref().unwrap().state.clone();
        let original = app.geotiffs.as_ptr();
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("cancelled.dxf");
        let ctx = egui::Context::default();
        app.pending_project_action = Some(ProjectAction::New);
        app.finish_template_export(out.clone(), state.clone());
        assert!(app.template_state.is_none());
        assert!(app.export_intent.is_none());
        assert_eq!(app.geotiffs.as_ptr(), original);
        app.pending_project_action = None;
        app.finish_template_export(out.clone(), state);
        rx.recv_timeout(Duration::from_secs(10)).unwrap();
        app.request_project_action(ProjectAction::Close, &ctx);
        assert!(app.pending_project_action.is_none());
        assert!(!app.close_confirmed);
        let frame = ctx.run(Default::default(), |ctx| app.render(ctx));
        assert!(frame.shapes.iter().any(
            |s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.job.text.contains("Exporteren"))
        ));
        gate.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.export_intent.is_some() {
            app.poll_export_jobs(&ctx);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(app.geotiffs.as_ptr(), original);
        assert_eq!(app.pending_project_action, Some(ProjectAction::Close));
        assert!(app.template_order_dialog.is_none());
        assert!(!out.exists());
        assert!(!app.close_confirmed);
    }
    #[test]
    fn real_ui_stays_responsive_and_restores_inputs_before_new_or_close_confirmation() {
        for action in [ProjectAction::New, ProjectAction::Close] {
            let (started_tx, started_rx) = mpsc::sync_channel(1);
            let (gate_tx, gate_rx) = mpsc::sync_channel(1);
            let mut app = SleufBaseApp {
                exporter: Some(
                    Exporter::with_processor(move |inputs, request, control| {
                        started_tx.send(()).unwrap();
                        gate_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                        sleufbase_rust::export_worker::process(inputs, request, control)
                    })
                    .unwrap(),
                ),
                ..Default::default()
            };
            app.datasets = vec![sleufbase_rust::parse_job_features(
                include_str!("../tests/fixtures/jobFeatures.json"),
                1,
                "PS1",
            )
            .unwrap()];
            app.virtual_trenches = vec![VirtualTrench::new(
                "PS2".into(),
                Vertex {
                    x: 0.,
                    y: 0.,
                    z: None,
                },
                Vertex {
                    x: 10.,
                    y: 0.,
                    z: None,
                },
                1.,
            )
            .unwrap()];
            app.trench_markers = vec![vec![]];
            app.dirty = true;
            let bounds = Bounds {
                min_x: 0.,
                min_y: 0.,
                max_x: 10.,
                max_y: 10.,
            };
            app.cadastral = Some(CadastralLayer {
                visible: true,
                bounds,
                linework: vec![],
                labels: vec![],
            });
            app.bgt_layers = vec![BgtLayer {
                visible: true,
                bounds,
                kind: BgtKind::Wegdeel,
                paths: vec![vec![(0., 0.), (1., 1.)]],
            }];
            let expected = serde_json::to_value((
                &app.datasets,
                &app.virtual_trenches,
                &app.cadastral,
                &app.bgt_layers,
            ))
            .unwrap();
            let pointer = app.datasets[0].features.as_ptr();
            let marker_pointer = app.trench_markers.as_ptr();
            app.save_dxf();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("cancelled.dxf");
            app.finish_dxf_export(path.clone(), crate::export_dialog::Action { overlay: true });
            started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            assert!(app.datasets.is_empty());
            assert!(app.virtual_trenches.is_empty());
            let ctx = egui::Context::default();
            let frame = ctx.run(
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
                |ctx| app.render(ctx),
            );
            assert!(frame.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t)if t.galley.job.text.contains("Exporteren"))));
            assert!(app.export_intent.is_some());
            assert_eq!(app.trench_markers.as_ptr(), marker_pointer);
            assert!(!app.save_project_to_path(dir.path().join("must-not-save-empty.json")));
            app.execute_project_action(action.clone(), &ctx);
            assert!(app.export_intent.is_some());
            assert!(app.pending_project_action.is_none());
            assert!(!app.close_confirmed);
            gate_tx.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while app.export_intent.is_some() {
                app.poll_export_jobs(&ctx);
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(2));
            }
            assert_eq!(app.pending_project_action, Some(action));
            assert!(app.export_dialog.is_none());
            assert!(!app.close_confirmed);
            assert_eq!(app.datasets[0].features.as_ptr(), pointer);
            assert_eq!(
                serde_json::to_value((
                    &app.datasets,
                    &app.virtual_trenches,
                    &app.cadastral,
                    &app.bgt_layers
                ))
                .unwrap(),
                expected
            );
            assert!(app.dirty);
            assert!(!path.exists());
            assert!(app.overlays.is_empty());
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
            app.finish_project_action(false, &ctx);
            assert_eq!(app.datasets.len(), 1);
        }
    }
}
