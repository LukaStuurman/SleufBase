//! Map edits retain project identity and use the existing task/unsaved-work guards.
use super::*;
use sleufbase_rust::map_comments;

pub(super) struct TiffCommentEditor {
    path: PathBuf,
    generation: u64,
    text: String,
    error: String,
}
impl SleufBaseApp {
    pub(super) fn show_map_actions(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.menu_button("Kaartacties", |ui| {
            if ui.button("Wis opmerkingen").clicked() {
                self.clear_map_comments();
                ui.close_menu();
            }
            if ui.button("Wis alles").clicked() {
                self.request_project_action(ProjectAction::ClearLayers, ctx);
                ui.close_menu();
            }
        });
    }
    pub(super) fn show_tiff_comment_controls(&mut self, ui: &mut egui::Ui) {
        let selected = self.raster_points.selected.clone();
        if let Some(path) = selected {
            if let Some(raster) = self.geotiffs.iter().find(|r| r.path == path) {
                if let Ok(text) = map_comments::text(&raster.metadata) {
                    if !text.is_empty() {
                        ui.small(format!(
                            "Opmerking: {}",
                            text.chars().take(100).collect::<String>()
                        ));
                    }
                }
            }
            ui.horizontal(|ui| {
                if ui.button("TIFF-opmerking bewerken").clicked() {
                    self.open_tiff_comment(&path);
                }
                if ui
                    .button("Wissen")
                    .on_hover_text("Wis alleen de opmerking van deze TIFF")
                    .clicked()
                {
                    self.clear_tiff_comment(&path);
                }
            });
        }
    }
    fn open_tiff_comment(&mut self, path: &std::path::Path) {
        if self.geotiffs.iter().filter(|r| r.path == path).count() != 1 {
            self.message = "Kies één unieke TIFF-bron".into();
            return;
        }
        let Some(raster) = self.geotiffs.iter().find(|r| r.path == path) else {
            return;
        };
        match map_comments::text(&raster.metadata) {
            Ok(text) => {
                self.tiff_comment_editor = Some(TiffCommentEditor {
                    path: path.into(),
                    generation: self.project_generation,
                    text,
                    error: String::new(),
                })
            }
            Err(error) => self.message = error,
        }
    }
    fn clear_tiff_comment(&mut self, path: &std::path::Path) {
        if self.geotiffs.iter().filter(|r| r.path == path).count() != 1 {
            self.message = "Kies één unieke TIFF-bron".into();
            return;
        }
        let Some(raster) = self.geotiffs.iter_mut().find(|r| r.path == path) else {
            return;
        };
        let result = (|| {
            if map_comments::text(&raster.metadata)?.is_empty() {
                return Ok(false);
            }
            map_comments::set(raster, "")
        })();
        match result {
            Ok(changed) => {
                self.dirty |= changed;
                self.message = if changed {
                    format!(
                        "Opmerking verwijderd voor {}.",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    )
                } else {
                    format!(
                        "Geen opmerking voor {}.",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    )
                };
                if changed {
                    self.tiff_comment_editor = None;
                }
            }
            Err(error) => self.message = error,
        }
    }
    fn save_tiff_comment(&mut self) {
        let Some(editor) = self.tiff_comment_editor.as_ref() else {
            return;
        };
        let result = (|| {
            if editor.generation != self.project_generation {
                return Err("Project is gewijzigd; open de opmerking opnieuw".into());
            }
            let mut matches = self.geotiffs.iter_mut().filter(|r| r.path == editor.path);
            let raster = matches.next().ok_or("GeoTIFF is niet meer geladen")?;
            if matches.next().is_some() {
                return Err("De TIFF-bron is niet uniek".into());
            }
            let had_note = !map_comments::text(&raster.metadata)?.is_empty();
            let changed = map_comments::set(raster, &editor.text)?;
            Ok::<_, String>((
                changed,
                had_note,
                map_comments::text(&raster.metadata)?.is_empty(),
            ))
        })();
        match result {
            Ok((changed, had_note, empty)) => {
                self.dirty |= changed;
                let file = editor
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                self.message = if empty {
                    if had_note {
                        format!("Opmerking verwijderd voor {file}.")
                    } else {
                        "Geen opmerking opgeslagen.".into()
                    }
                } else {
                    format!(
                        "Opmerking {} voor {file}.",
                        if had_note { "bijgewerkt" } else { "opgeslagen" }
                    )
                };
                self.tiff_comment_editor = None;
            }
            Err(error) => self.tiff_comment_editor.as_mut().unwrap().error = error,
        }
    }
    pub(super) fn show_tiff_comment_editor(&mut self, ctx: &egui::Context) {
        let Some(editor) = self.tiff_comment_editor.as_mut() else {
            return;
        };
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        egui::Window::new("Proefsleuf Opmerking")
            .id(egui::Id::new("tiff_comment_editor"))
            .open(&mut open)
            .default_width(480.)
            .show(ctx, |ui| {
                ui.label(
                    editor
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy(),
                );
                ui.label("Elke regel wordt apart in de export opgenomen.");
                ui.add(
                    egui::TextEdit::multiline(&mut editor.text)
                        .desired_rows(8)
                        .desired_width(f32::INFINITY)
                        .char_limit(64 * 1024),
                );
                if !editor.error.is_empty() {
                    ui.colored_label(DARK, &editor.error);
                }
                ui.horizontal(|ui| {
                    save = ui.button("Opslaan").clicked();
                    cancel = ui.button("Annuleren").clicked();
                });
            });
        if !open || cancel || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.tiff_comment_editor = None;
        } else if save {
            self.save_tiff_comment();
        }
    }
    pub(super) fn clear_map_comments(&mut self) {
        match map_comments::clear_all(&mut self.comments, &mut self.geotiffs, &mut self.virtual_trenches) {
            Ok(count) => {
                self.dirty |= count > 0;
                self.clear_virtual_editor_metadata(map_comments::KEY,None);
                self.tiff_comment_editor = None;
                self.show_comment_dialog = false;
                self.message = if count == 0 {
                    "Geen opmerkingen om te wissen.".into()
                } else {
                    format!("{count} opmerking(en) gewist.")
                };
            }
            Err(error) => self.message = error,
        }
    }
    pub(super) fn clear_map_layers(&mut self, ctx: &egui::Context) {
        let changed = !self.datasets.is_empty()
            || !self.geotiffs.is_empty()
            || !self.overlays.is_empty()
            || !self.virtual_trenches.is_empty()
            || !self.comments.is_empty()
            || !self.bgt_layers.is_empty()
            || self.cadastral.is_some()
            || self.template_state.is_some()
            || self.template_preferences.is_some();
        self.clear_project_requests();
        self.browser_ui
            .cancel
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.browser_ui.queue.clear();
        self.browser_ui.downloaded_json.clear();
        self.browser_ui.downloaded_tiffs.clear();
        self.browser_ui.batch_mode = false;
        self.datasets.clear();
        self.geotiffs.clear();
        self.raster_textures.clear();
        self.raster_errors.clear();
        self.overlays.clear();
        self.dxf_selection = None;
        self.virtual_trenches.clear();
        self.trench_markers.clear();
        self.comments.clear();
        self.bgt_layers.clear();
        self.cadastral = None;
        self.template_state = None;
        self.template_preferences = None;
        self.selected = None;
        self.dirty |= changed;
        self.message = "Alle lagen zijn gewist.".into();
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use sleufbase_rust::{export_worker::Exporter, map_comments as notes};
    fn app() -> SleufBaseApp {
        let value: Value =
            serde_json::from_str(include_str!("../tests/fixtures/map-actions/reference.json"))
                .unwrap();
        let r = &value["cases"][0]["rasters"][0];
        let mut app = SleufBaseApp {
            geotiffs: vec![GeoTiffInfo {
                path: "Proefsleuf Ø.tif".into(),
                width: 10,
                height: 6,
                transform: serde_json::from_value(r["transform"].clone()).unwrap(),
                bounds: serde_json::from_value(r["bounds"].clone()).unwrap(),
                epsg: Some(28992),
                kickthemap_job_id: Some(42),
                kickthemap_job_title: "PS42".into(),
                kickthemap_job_sidecar: None,
                metadata: json!({"keep":true,"tiff_comment_text":"TIFF note","template_reference_point":{"x":1,"y":2}}),
            }],
            comments: vec![Comment {
                x: 1.,
                y: 2.,
                text: "Map note".into(),
            }],
            project_name: "Retained project".into(),
            project_path: Some("project.json".into()),
            ..Default::default()
        };
        app.raster_points.selected = Some("Proefsleuf Ø.tif".into());
        app.raster_points.online = false;
        app
    }
    fn frame(
        app: &mut SleufBaseApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1000., 800.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::TopBottomPanel::top("test_actions")
                    .show(ctx, |ui| app.show_map_actions(ui, ctx));
                app.show_tiff_comment_editor(ctx);
                app.show_unsaved_project_dialog(ctx);
            },
        )
    }
    fn label(output: &egui::FullOutput, text: &str) -> Pos2 {
        output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == text => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("No actual UI text {text}"))
    }
    fn click(app: &mut SleufBaseApp, ctx: &egui::Context, p: Pos2) -> egui::FullOutput {
        frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(p),
                egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(p),
                egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        )
    }
    #[test]
    fn actual_editor_save_and_escape_preserve_points_project_identity_and_unchanged_dirty_state() {
        let ctx = egui::Context::default();
        let mut app = app();
        let pointer = app.geotiffs.as_ptr();
        let source = app.geotiffs[0].path.clone();
        app.open_tiff_comment(&source);
        app.tiff_comment_editor.as_mut().unwrap().text = "  Nieuwe regel Ø\n Tweede  ".into();
        frame(&mut app, &ctx, Vec::new());
        let output = frame(&mut app, &ctx, Vec::new());
        let p = label(&output, "Opslaan");
        click(&mut app, &ctx, p);
        assert!(app.tiff_comment_editor.is_none());
        assert_eq!(
            notes::text(&app.geotiffs[0].metadata).unwrap(),
            "Nieuwe regel Ø\n Tweede"
        );
        assert!(app.dirty);
        assert_eq!(app.geotiffs.as_ptr(), pointer);
        assert_eq!(app.geotiffs[0].metadata["keep"], true);
        assert_eq!(
            app.geotiffs[0].metadata["template_reference_point"],
            json!({"x":1,"y":2})
        );
        app.dirty = false;
        app.open_tiff_comment(&source);
        app.save_tiff_comment();
        assert!(!app.dirty);
        app.open_tiff_comment(&source);
        app.tiff_comment_editor.as_mut().unwrap().text = "discard draft".into();
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(app.tiff_comment_editor.is_none());
        assert!(!app.dirty);
        assert_eq!(
            notes::text(&app.geotiffs[0].metadata).unwrap(),
            "Nieuwe regel Ø\n Tweede"
        );
        assert_eq!(app.project_name, "Retained project");
        assert_eq!(app.project_path, Some("project.json".into()));
    }
    #[test]
    fn actual_clear_comments_menu_clears_map_and_tiff_notes_but_keeps_sources_and_references() {
        let ctx = egui::Context::default();
        let mut app = app();
        let pointer = app.geotiffs.as_ptr();
        let output = frame(&mut app, &ctx, Vec::new());
        let p = label(&output, "Kaartacties");
        click(&mut app, &ctx, p);
        let output = frame(&mut app, &ctx, Vec::new());
        let p = label(&output, "Wis opmerkingen");
        click(&mut app, &ctx, p);
        assert!(app.comments.is_empty());
        assert_eq!(notes::text(&app.geotiffs[0].metadata).unwrap(), "");
        assert!(app.dirty);
        assert_eq!(app.message, "2 opmerking(en) gewist.");
        assert_eq!(app.geotiffs.as_ptr(), pointer);
        assert_eq!(
            app.geotiffs[0].metadata["template_reference_point"],
            json!({"x":1,"y":2})
        );
        app.dirty = false;
        app.clear_map_comments();
        assert!(!app.dirty);
        assert_eq!(app.message, "Geen opmerkingen om te wissen.");
    }
    #[test]
    fn clear_all_confirmation_preserves_saved_identity_source_files_settings_jobs_and_ignores_stale_work(
    ) {
        let ctx = egui::Context::default();
        let mut app = app();
        app.datasets.push(sleufbase_rust::parse_job_features(r#"{"features":[{"geometry":{"type":"Point","coordinates":[1,2]},"properties":{"idName":"water"}}]}"#,42,"PS42").unwrap());
        app.overlays.push(DxfOverlay {
            visible: true,
            ..Default::default()
        });
        app.virtual_trenches.push(
            VirtualTrench::new(
                "Virtual".into(),
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
        );
        app.bgt_layers.push(BgtLayer {
            kind: BgtKind::Wegdeel,
            bounds: app.geotiffs[0].bounds,
            visible: true,
            paths: vec![vec![(1., 2.), (3., 4.)]],
        });
        app.cadastral = Some(CadastralLayer {
            visible: true,
            bounds: app.geotiffs[0].bounds,
            linework: Vec::new(),
            labels: Vec::new(),
        });
        let mut order = sleufbase_rust::template_order::State::default();
        order.sync(&app.geotiffs).unwrap();
        app.template_state = Some(order);
        app.raster_points
            .rows
            .push(sleufbase_rust::raster_points::Candidate {
                raw: [1., 2.],
                display: [1., 2.],
                selected: true,
            });
        let dir = tempfile::tempdir().unwrap();
        let saved = dir.path().join("saved.json");
        assert!(app.save_project_to_path(saved.clone()));
        let original = std::fs::read(&saved).unwrap();
        let settings = serde_json::to_value(&app.migrated_settings).unwrap();
        app.live_jobs.push(Job {
            job_id: 42,
            title: "Retained job".into(),
            address: String::new(),
            municipality: String::new(),
            coordinates: String::new(),
            client_date: String::new(),
            status: 0,
            downloadable: true,
            prefix: String::new(),
        });
        app.dirty = true;
        app.request_project_action(ProjectAction::ClearLayers, &ctx);
        assert_eq!(app.geotiffs.len(), 1);
        app.finish_project_action(false, &ctx);
        assert_eq!(app.geotiffs.len(), 1);
        assert_eq!(app.comments.len(), 1);
        let old_generation = app.project_generation;
        let (tx, rx) = mpsc::channel();
        app.ahn_rx = Some(rx);
        app.ahn_busy = true;
        app.browser_ui.batch_mode = true;
        app.browser_ui.downloaded_json.push("old.json".into());
        app.browser_ui
            .queue
            .push_back((42, RemoteFileKind::JobFeatures));
        app.request_project_action(ProjectAction::ClearLayers, &ctx);
        app.finish_project_action(true, &ctx);
        assert!(
            app.geotiffs.is_empty()
                && app.comments.is_empty()
                && app.datasets.is_empty()
                && app.virtual_trenches.is_empty()
                && app.overlays.is_empty()
        );
        assert!(app.dirty);
        assert_eq!(app.project_generation, old_generation + 1);
        let original_contract: Value =
            serde_json::from_str(include_str!("../tests/fixtures/map-actions/reference.json"))
                .unwrap();
        let observed = json!({"tiff_layers":app.geotiffs,"dxf_overlays":app.overlays,"map_comments":app.comments,"template_export_slot_order":app.template_state.as_ref().map(|s|s.rows.as_slice()).unwrap_or(&[]),"maaiveld_markers":app.trench_markers.iter().map(Vec::len).collect::<Vec<_>>(),"maaiveld_marker_payloads":app.raster_points.rows,"selected_tiff_index":app.raster_points.selected,"selected_dxf_index":app.dxf_selection.as_ref().map(|d|d.overlay),"display_image":app.pdok_texture.as_ref().map(|_|true),"display_bounds":app.pdok_bounds,"display_size":app.raster_textures.first().map(|_|true)});
        for (key, value) in observed.as_object().unwrap() {
            assert_eq!(
                value, &original_contract["clear_all"]["state"][key],
                "Original workspace-reset field {key}"
            );
        }
        assert!(app.bgt_layers.is_empty() && app.cadastral.is_none());
        assert_eq!(app.project_name, "Retained project");
        assert_eq!(app.project_path, Some(saved.clone()));
        assert_eq!(app.live_jobs[0].job_id, 42);
        assert_eq!(
            serde_json::to_value(&app.migrated_settings).unwrap(),
            settings
        );
        assert!(tx.send(Ok(Some(99.))).is_err());
        app.poll_ahn();
        assert_eq!(app.message, "Alle lagen zijn gewist.");
        app.poll_kickthemap_browser(&ctx);
        assert!(!app.browser_ui.batch_mode);
        assert!(app.browser_ui.queue.is_empty() && app.browser_ui.downloaded_json.is_empty());
        app.live_download_generation = old_generation;
        app.apply_live_download_result(
            Ok((
                RemoteFileKind::JobFeatures,
                dir.path().join("old-download.json"),
            )),
            &ctx,
        );
        assert!(app.file_intent.is_none());
        assert_eq!(std::fs::read(&saved).unwrap(), original);
        let cleared = dir.path().join("cleared.json");
        assert!(app.save_project_to_path(cleared.clone()));
        assert!(project::read_project(&cleared).unwrap().rasters.is_empty());
        assert_eq!(std::fs::read(saved).unwrap(), original);
    }
    #[test]
    fn clear_all_waits_for_owned_worker_sources_and_cancellation_before_applying() {
        let ctx = egui::Context::default();
        let mut app = app();
        let (started, rx) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        app.exporter = Some(
            Exporter::with_processor(move |_, _, control| {
                started.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
                assert!(control.is_cancelled());
                Err("cancelled".into())
            })
            .unwrap(),
        );
        let request = sleufbase_rust::export_worker::Request {
            kind: sleufbase_rust::export_worker::Kind::StartPoints,
            path: PathBuf::new(),
            selection: sleufbase_rust::object_selection::Selection::all(0, 0),
            rules: Vec::new(),
            extra_layers: false,
            options: Default::default(),
            template: None,
            discipline_excel: false,
            maps: None,
            point_target: None,
            point_network: false,
        };
        assert!(app.start_export(request, false));
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        app.request_project_action(ProjectAction::ClearLayers, &ctx);
        assert!(app.pending_project_action.is_none());
        release.send(()).unwrap();
        let until = Instant::now() + Duration::from_secs(10);
        while app.export_intent.is_some() {
            app.poll_export_jobs(&ctx);
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(app.geotiffs.is_empty());
        assert!(app.comments.is_empty());
        assert!(app.dirty);
        assert_eq!(app.project_name, "Retained project");
        assert_eq!(app.message, "Alle lagen zijn gewist.");
    }
}
