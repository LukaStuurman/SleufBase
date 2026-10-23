//! Point modes use the exact current canvas transform, not a second fitted map.
use super::*;
use sleufbase_rust::{
    export_worker::{Exported, Kind, Request},
    raster_points::{self as points, Candidate},
};
use std::collections::HashSet;
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Mode {
    Reference,
    Start,
}
pub(super) struct State {
    pub online: bool,
    pub selected: Option<PathBuf>,
    pub mode: Option<Mode>,
    pub rows: Vec<Candidate>,
    pub pending_generation: Option<u64>,
    attempted: HashSet<u64>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            online: true,
            selected: None,
            mode: None,
            rows: Vec::new(),
            pending_generation: None,
            attempted: HashSet::new(),
        }
    }
}
pub(super) struct View {
    pub response:egui::Response,
    pub painter: egui::Painter,
    pub rect: egui::Rect,
    pub center: Pos2,
    pub origin: [f64; 2],
    pub scale: f64,
    pub clicked: Option<Pos2>,
}
impl View {
    pub fn project(&self, p: [f64; 2]) -> Pos2 {
        Pos2::new(
            self.center.x + ((p[0] - self.origin[0]) * self.scale) as f32,
            self.center.y - ((p[1] - self.origin[1]) * self.scale) as f32,
        )
    }
    pub fn world(&self, p: Pos2) -> [f64; 2] {
        [
            self.origin[0] + f64::from(p.x - self.center.x) / self.scale,
            self.origin[1] - f64::from(p.y - self.center.y) / self.scale,
        ]
    }
}
impl SleufBaseApp {
    pub(super) fn show_point_controls(&mut self, ui: &mut egui::Ui) {
        if self
            .raster_points
            .selected
            .as_ref()
            .is_none_or(|p| !self.geotiffs.iter().any(|r| r.path == *p))
        {
            self.raster_points.selected = self.geotiffs.first().map(|r| r.path.clone());
            self.raster_points.mode = None;
            self.raster_points.rows.clear();
        }
        ui.collapsing("Begin- en referentiepunten",|ui|{
            ui.checkbox(&mut self.raster_points.online,"Weg- en terreincontext online ophalen");
            ui.small("Selecteer hierboven een GeoTIFF. Punten worden bij het project bewaard en in sjablonen gebruikt.");
            let target=self.raster_points.selected.is_some();
            if ui.add_enabled(target,egui::Button::new(if self.raster_points.mode==Some(Mode::Reference){"Referentie sluiten"}else{"Referentiepunt plaatsen"})).clicked(){
                self.stop_raster_rotation();
                self.raster_points.mode=if self.raster_points.mode==Some(Mode::Reference){None}else{Some(Mode::Reference)};
                self.message="Klik op de kaart om het referentiepunt te plaatsen of te verplaatsen.".into();
            }
            if ui.add_enabled(target,egui::Button::new(if self.raster_points.mode==Some(Mode::Start){"Beginpunt sluiten"}else{"Beginpunt kiezen"})).clicked(){
                self.stop_raster_rotation();
                if self.raster_points.mode==Some(Mode::Start){self.raster_points.mode=None;}else if let Some(path)=self.raster_points.selected.clone(){self.start_point_task(Some(path));}
            }
            if ui.button("Wis alle referentiepunten").clicked(){self.clear_raster_references();}
            if ui.add_enabled(!self.geotiffs.is_empty()&&self.raster_points.pending_generation.is_none(),egui::Button::new("Laad alle beginpunten")).clicked(){
                self.raster_points.pending_generation=Some(self.project_generation);self.raster_points.attempted.clear();self.poll_point_downloads();
            }
            if self.raster_points.pending_generation.is_some(){
                ui.label("Objectgegevens voor beginpunten worden voorbereid…");
                if ui.button("Beginpunten annuleren").clicked(){self.raster_points.pending_generation=None;self.browser_ui.cancel.store(true,std::sync::atomic::Ordering::Relaxed);self.message="Beginpunten laden geannuleerd".into();}
            }
        });
    }
    pub(super) fn clear_raster_references(&mut self) {
        let mut removed = self
            .geotiffs
            .iter()
            .filter(|r| r.metadata.get(points::REFERENCE).is_some())
            .map(|r| r.path.clone())
            .collect::<HashSet<_>>();
        points::clear_reference(&mut self.geotiffs);
        for state in self
            .template_state
            .iter_mut()
            .chain(self.template_order_dialog.iter_mut().map(|d| &mut d.state))
        {
            for row in &mut state.rows {
                if row
                    .metadata
                    .as_object_mut()
                    .and_then(|m| m.remove(points::REFERENCE))
                    .is_some()
                {
                    if let Some(path) = &row.raster {
                        removed.insert(path.clone());
                    }
                }
            }
        }
        self.dirty |= !removed.is_empty();
        self.message = if removed.is_empty() {
            "Geen referentiepunten om te wissen.".into()
        } else {
            format!("{} referentiepunt(en) gewist.", removed.len())
        };
    }
    fn start_point_task(&mut self, target: Option<PathBuf>) -> bool {
        self.raster_points.mode = None;
        self.raster_points.rows.clear();
        let kind = if target.is_some() {
            Kind::StartCandidates
        } else {
            Kind::StartPoints
        };
        let request = Request {
            kind,
            path: PathBuf::new(),
            selection: sleufbase_rust::object_selection::Selection::all(0, 0),
            rules: self.migrated_settings.object_rules.clone(),
            extra_layers: false,
            options: Default::default(),
            template: None,
            discipline_excel: false,
            maps: None,
            point_target: target,
            point_network: self.raster_points.online,
        };
        self.start_export(request, false)
    }
    pub(super) fn poll_point_downloads(&mut self) {
        let Some(generation) = self.raster_points.pending_generation else {
            return;
        };
        if generation != self.project_generation
            || self
                .browser_ui
                .cancel
                .load(std::sync::atomic::Ordering::Relaxed)
                && !self.raster_points.attempted.is_empty()
        {
            self.raster_points.pending_generation = None;
            return;
        }
        if self.live_busy
            || self.pending_project_action.is_some()
            || self.file_intent.is_some()
            || self.export_intent.is_some()
            || self.template_job.is_some()
            || self.browser_ui.batch_mode
            || !self.browser_ui.downloaded_json.is_empty()
            || !self.browser_ui.downloaded_tiffs.is_empty()
        {
            return;
        }
        let missing = self
            .geotiffs
            .iter()
            .filter(|r| points::local_dataset_path(r).is_none())
            .filter_map(|r| r.kickthemap_job_id)
            .filter(|id| {
                !self.datasets.iter().any(|d| d.job_id == *id)
                    && !self.raster_points.attempted.contains(id)
                    && self.live_jobs.iter().any(|j| j.job_id == *id)
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .take(500)
            .collect::<Vec<_>>();
        if !missing.is_empty() && self.live_session.is_some() {
            self.raster_points.attempted.extend(missing.iter().copied());
            self.queue_kickthemap_jobs(&missing, false, true);
            return;
        }
        self.raster_points.pending_generation = None;
        self.start_point_task(None);
    }
    pub(super) fn finish_point_task(&mut self, kind: Kind, exported: Exported) {
        if kind == Kind::StartCandidates {
            if let Some((path, rows)) = exported.point_candidates {
                self.raster_points.selected = Some(path);
                self.raster_points.mode = (rows.len() >= 2).then_some(Mode::Start);
                self.raster_points.rows = rows;
                self.message = if self.raster_points.mode.is_some() {
                    "Klik op een eindpuntmarker om het beginpunt van het dwarsprofiel te kiezen."
                        .into()
                } else {
                    "Geen beginpuntkandidaten beschikbaar voor deze proefsleuf.".into()
                };
                for warning in exported.warnings.iter().take(15) {
                    self.message.push('\n');
                    self.message.push_str(warning);
                }
            }
        } else {
            let count = exported.point_updates.len();
            match points::apply(&mut self.geotiffs, &exported.point_updates) {
                Ok(changed) => {
                    self.dirty |= changed > 0;
                    self.message = if count == 0 {
                        "Er konden geen beginpunten worden geladen.".into()
                    } else {
                        format!(
                            "Beginpunten geladen voor {count} KickTheMap-proefsleuf/proefsleuven."
                        )
                    };
                }
                Err(e) => self.message = format!("Beginpunten niet gewijzigd: {e}"),
            }
            for warning in exported.warnings.iter().take(15) {
                self.message.push('\n');
                self.message.push_str(warning);
            }
        }
    }
    pub(super) fn handle_raster_points(&mut self, view: &View) {
        let Some(mode) = self.raster_points.mode else {
            return;
        };
        let Some(index) = self
            .raster_points
            .selected
            .as_ref()
            .and_then(|p| self.geotiffs.iter().position(|r| r.path == *p))
        else {
            return;
        };
        if mode == Mode::Reference {
            if let Some(raw) = points::stored(&self.geotiffs[index], points::REFERENCE) {
                if let Ok(p) = points::display(&self.geotiffs[index], raw) {
                    let p = view.project(p);
                    if view.rect.contains(p) {
                        view.painter.circle_filled(p, 9., Color32::WHITE);
                        view.painter
                            .circle_stroke(p, 9., Stroke::new(2.0_f32, Color32::BLACK));
                    }
                }
            }
        } else {
            for row in &self.raster_points.rows {
                let p = view.project(row.display);
                if view.rect.contains(p) {
                    view.painter.circle_filled(
                        p,
                        10.,
                        if row.selected { ORANGE } else { Color32::WHITE },
                    );
                    view.painter
                        .circle_stroke(p, 10., Stroke::new(2.0_f32, DARK));
                }
            }
        }
        if let Some(click) = view.clicked {
            let world = view.world(click);
            let result = if mode == Mode::Reference {
                points::set(&mut self.geotiffs[index], points::REFERENCE, world, true)
            } else {
                let mpp = 1. / view.scale;
                if let Some(i) = points::pick(
                    &self.raster_points.rows,
                    world,
                    mpp,
                    (mpp * 6.).clamp(0.001, 3.),
                ) {
                    let raw = self.raster_points.rows[i].raw;
                    for row in &mut self.raster_points.rows {
                        row.selected = (row.raw[0] - raw[0]).abs() <= 1e-6
                            && (row.raw[1] - raw[1]).abs() <= 1e-6;
                    }
                    points::set(&mut self.geotiffs[index], points::START, raw, false)
                } else {
                    self.message = "Klik op een eindpuntmarker om het beginpunt te kiezen.".into();
                    return;
                }
            };
            match result {
                Ok(changed) => {
                    self.dirty |= changed;
                    view.painter.ctx().request_repaint();
                    self.message = if mode == Mode::Reference {
                        "Referentiepunt bijgewerkt.".into()
                    } else {
                        "Beginpunt bijgewerkt.".into()
                    };
                }
                Err(e) => self.message = e,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    fn app() -> SleufBaseApp {
        let data: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/raster-points/reference.json"
        ))
        .unwrap();
        let c = &data["cases"][1];
        let transform =
            serde_json::from_value::<sleufbase_rust::geo::GeoTransform>(c["transform"].clone())
                .unwrap()
                .rotate_degrees(37.5, 20, 20)
                .unwrap();
        let r = GeoTiffInfo {
            path: "PS-2.tif".into(),
            width: 20,
            height: 20,
            transform,
            bounds: transform.bounds(20, 20).unwrap(),
            epsg: Some(28992),
            kickthemap_job_id: Some(2),
            kickthemap_job_title: "PS2".into(),
            kickthemap_job_sidecar: None,
            metadata: c["metadata"].clone(),
        };
        let mut app = SleufBaseApp {
            geotiffs: vec![r],
            datasets: vec![serde_json::from_value(c["dataset"].clone()).unwrap()],
            ..Default::default()
        };
        app.raster_points.selected = Some("PS-2.tif".into());
        app.raster_points.online = false;
        app
    }
    fn frame(app: &mut SleufBaseApp, ctx: &egui::Context, events: Vec<egui::Event>) -> View {
        let mut result = None;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(900., 600.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let view = canvas(
                        ui,
                        &[],
                        &app.geotiffs,
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                        None,
                        &[],
                        None,
                        None,
                        false,
                        &mut app.zoom,
                        &mut app.pan,
                        &app.migrated_settings.object_rules,
                        Some(&mut app.navigation),
                    )
                    .unwrap();
                    app.handle_raster_points(&view);
                    result = Some(view);
                });
            },
        );
        result.unwrap()
    }
    fn click(app: &mut SleufBaseApp, ctx: &egui::Context, p: Pos2) {
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(p),
                    egui::Event::PointerButton {
                        pos: p,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }
    #[test]
    fn real_canvas_pointer_click_stores_unrotated_reference_after_zoom_and_pan() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.zoom = 1.7;
        app.pan = egui::vec2(23., -17.);
        app.raster_points.mode = Some(Mode::Reference);
        let c: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/raster-points/reference.json"
        ))
        .unwrap();
        let c = &c["cases"][1];
        let target: [f64; 2] = serde_json::from_value(c["reference_click"].clone()).unwrap();
        let view = frame(&mut app, &ctx, Vec::new());
        let p = view.project(target);
        assert!(view.rect.contains(p));
        click(&mut app, &ctx, p);
        let stored = points::stored(&app.geotiffs[0], points::REFERENCE).unwrap();
        let expected = points::point(&c["after_reference"][points::REFERENCE]).unwrap();
        assert!((stored[0] - expected[0]).hypot(stored[1] - expected[1]) < 1e-5);
        assert!(app.dirty);
        assert_eq!(app.geotiffs[0].metadata["notes"], c["metadata"]["notes"]);
    }
    #[test]
    fn actual_candidate_worker_returns_sources_and_canvas_selection_updates_raw_start() {
        let ctx = egui::Context::default();
        let mut app = app();
        let source_pointer = app.geotiffs.as_ptr();
        let feature_pointer = app.datasets[0].features.as_ptr();
        assert!(app.start_point_task(Some("PS-2.tif".into())));
        assert!(app.geotiffs.is_empty());
        let until = Instant::now() + Duration::from_secs(10);
        while app.export_intent.is_some() {
            app.poll_export_jobs(&ctx);
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(app.geotiffs.as_ptr(), source_pointer);
        assert_eq!(app.datasets[0].features.as_ptr(), feature_pointer);
        assert!(!app.dirty);
        assert!(app.raster_points.mode == Some(Mode::Start));
        let row = app.raster_points.rows.last().unwrap().clone();
        let view = frame(&mut app, &ctx, Vec::new());
        click(&mut app, &ctx, view.project(row.display));
        assert_eq!(
            points::stored(&app.geotiffs[0], points::START),
            Some(row.raw)
        );
        assert!(app.dirty);
    }
    #[test]
    fn clear_all_invalidates_older_slot_snapshots_and_no_op_does_not_dirty() {
        let mut app = app();
        app.clear_raster_references();
        assert!(!app.dirty);
        assert_eq!(app.message, "Geen referentiepunten om te wissen.");
        app.geotiffs[0].metadata[points::REFERENCE] = json!({"x":1,"y":2});
        let mut state = sleufbase_rust::template_order::State::default();
        state.sync(&app.geotiffs).unwrap();
        state.rows[0].metadata =
            json!({"template_reference_point":{"x":1,"y":2},"keep":"old slot option"});
        app.template_state = Some(state);
        app.clear_raster_references();
        assert_eq!(app.message, "1 referentiepunt(en) gewist.");
        assert!(app.dirty);
        assert!(points::stored(&app.geotiffs[0], points::REFERENCE).is_none());
        assert_eq!(
            app.template_state.as_ref().unwrap().rows[0].metadata["keep"],
            "old slot option"
        );
        assert!(app.template_state.as_ref().unwrap().rows[0]
            .metadata
            .get(points::REFERENCE)
            .is_none());
        app.dirty = false;
        app.clear_raster_references();
        assert!(!app.dirty);
    }
    #[test]
    fn pending_bulk_points_wait_for_download_loading_and_cancel_on_project_switch() {
        let mut app = app();
        let (started, rx) = std::sync::mpsc::channel();
        let (release, gate) = std::sync::mpsc::channel();
        app.exporter = Some(
            sleufbase_rust::export_worker::Exporter::with_processor(move |_, request, control| {
                assert_eq!(request.kind, Kind::StartPoints);
                started.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
                assert!(control.is_cancelled());
                Ok(Exported {
                    point_updates: Vec::new(),
                    point_candidates: None,
                    skipped: Vec::new(),
                    companion: None,
                    warnings: Vec::new(),
                })
            })
            .unwrap(),
        );
        app.raster_points.pending_generation = Some(app.project_generation);
        app.browser_ui.batch_mode = true;
        app.poll_point_downloads();
        assert!(app.export_intent.is_none());
        assert!(app.raster_points.pending_generation.is_some());
        app.browser_ui.batch_mode = false;
        app.poll_point_downloads();
        assert!(app.export_intent.is_some());
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let ctx = egui::Context::default();
        app.request_project_action(ProjectAction::New, &ctx);
        release.send(()).unwrap();
        let until = Instant::now() + Duration::from_secs(10);
        while app.export_intent.is_some() {
            app.poll_export_jobs(&ctx);
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(app.geotiffs.is_empty());
        assert!(app.raster_points.pending_generation.is_none());
        assert!(app.raster_points.mode.is_none());
    }
}
