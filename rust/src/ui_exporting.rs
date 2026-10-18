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

impl SleufBaseApp {
    fn take_export_inputs(&mut self) -> Inputs {
        Inputs {
            datasets: std::mem::take(&mut self.datasets),
            trenches: std::mem::take(&mut self.virtual_trenches),
            cadastral: self.cadastral.take(),
            bgt: std::mem::take(&mut self.bgt_layers),
        }
    }
    fn restore_export_inputs(&mut self, inputs: Inputs) {
        debug_assert!(
            self.datasets.is_empty()
                && self.virtual_trenches.is_empty()
                && self.cadastral.is_none()
                && self.bgt_layers.is_empty()
        );
        self.datasets = inputs.datasets;
        self.virtual_trenches = inputs.trenches;
        self.cadastral = inputs.cadastral;
        self.bgt_layers = inputs.bgt;
    }
    fn start_export(&mut self, request: Request, overlay: bool) {
        if self.export_intent.is_some() {
            self.message = "Er loopt al een export".into();
            return;
        }
        if self.file_intent.is_some() {
            self.message = "Wacht tot het laden is afgerond voordat je exporteert".into();
            return;
        }
        if self.pending_project_action.is_some() {
            self.message = "Rond eerst de projectkeuze af".into();
            return;
        }
        if self.exporter.as_ref().is_none_or(Exporter::is_closed) {
            match Exporter::new() {
                Ok(worker) => self.exporter = Some(worker),
                Err(error) => {
                    self.message = error;
                    return;
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
            }
            Err(error) => {
                let failure = *error;
                self.restore_export_inputs(failure.inputs);
                self.message = failure.message;
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
                self.message = format!("Export opgeslagen: {}{skipped}", intent.path.display());
            }
            Err(error) => {
                self.message = format!("Export niet opgeslagen; projectgegevens behouden: {error}");
                if intent.kind == Kind::Dxf {
                    if let Some(dialog) = self.export_dialog.as_mut() {
                        dialog.error = error;
                    }
                }
            }
        }
        if let Some(action) = intent.queued_action {
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
