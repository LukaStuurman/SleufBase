//! TIFF rotation reuses image textures and holds a fixed camera during dragging.
use super::*;
use sleufbase_rust::geotiff_rotation as rotation;
#[derive(Clone, Copy)]
struct Camera {
    center: [f64; 2],
    meters_per_pixel: f64,
    size: [f64; 2],
}

struct Drag {
    start: f64,
    initial: f64,
}
#[derive(Default)]
pub(super) struct State {
    target: Option<PathBuf>,
    generation: u64,
    fitted: bool,
    drag: Option<Drag>,
    last: Option<Camera>,
    draft_path: Option<PathBuf>,
    draft: String,
}
impl State {
    pub fn active(&self) -> bool {
        self.target.is_some()
    }
}
impl SleufBaseApp {
    pub(super) fn stop_raster_rotation(&mut self) {
        self.raster_rotation.target = None;
        self.raster_rotation.drag = None;
        self.navigation.pointer_tool = false;
    }
    fn hold_rotation_camera(&mut self) {
        if let Some(camera) = self.raster_rotation.last {
            self.navigation.focus = Some(location_ui::Focus {
                center: camera.center,
                meters_per_pixel: camera.meters_per_pixel,
            });
            self.zoom = 1.;
            self.pan = egui::Vec2::ZERO;
        }
    }
    fn rotate_raster(&mut self, path: &std::path::Path, angle: f64, announce: bool) -> bool {
        let matching = self.geotiffs.iter().filter(|r| r.path == path).count();
        if matching != 1 {
            self.message = "Kies één unieke TIFF-bron".into();
            return false;
        }
        let raster = self.geotiffs.iter_mut().find(|r| r.path == path).unwrap();
        match rotation::set(raster, angle) {
            Ok(changed) => {
                self.dirty |= changed;
                self.raster_points.mode = None;
                self.raster_points.rows.clear();
                let angle = rotation::angle(raster);
                self.raster_rotation.draft = angle.to_string();
                if announce {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    self.message = if angle.abs() < 1e-9 {
                        format!("Rotatie gereset voor {name}.")
                    } else {
                        format!("{name} gedraaid naar {angle:.1}° om het middelpunt.")
                    };
                }
                true
            }
            Err(error) => {
                self.message = error;
                false
            }
        }
    }
    fn apply_raster_rotation(&mut self, path: &std::path::Path, angle: f64) {
        self.stop_raster_rotation();
        self.hold_rotation_camera();
        self.rotate_raster(path, angle, true);
    }
    fn start_raster_rotation(&mut self, path: PathBuf) {
        self.cancel_virtual_map();
        if self.geotiffs.iter().filter(|r| r.path == path).count() != 1 {
            self.message = "Kies één unieke TIFF-bron".into();
            return;
        }
        self.raster_points.mode = None;
        self.raster_points.rows.clear();
        self.raster_rotation.target = Some(path);
        self.raster_rotation.generation = self.project_generation;
        self.raster_rotation.fitted = false;
        self.raster_rotation.drag = None;
        self.message =
            "Sleep aan de rotatiecirkel. Rechtsklik of Escape sluit de draaibediening.".into();
    }
    pub(super) fn show_raster_rotation_controls(&mut self, ui: &mut egui::Ui) {
        let Some(path) = self.raster_points.selected.clone() else {
            return;
        };
        let Some(raster) = self.geotiffs.iter().find(|r| r.path == path) else {
            return;
        };
        let current = rotation::angle(raster);
        if self.raster_rotation.draft_path.as_ref() != Some(&path) {
            self.stop_raster_rotation();
            self.raster_rotation.draft_path = Some(path.clone());
            self.raster_rotation.draft = current.to_string();
        }
        ui.collapsing("TIFF draaien", |ui| {
            ui.small("Positief is linksom, negatief is rechtsom. Rasterpixels blijven behouden.");
            ui.horizontal_wrapped(|ui| {
                if ui.button("90° linksom").clicked() {
                    self.apply_raster_rotation(&path, current + 90.);
                }
                if ui.button("90° rechtsom").clicked() {
                    self.apply_raster_rotation(&path, current - 90.);
                }
                if ui.button("Reset rotatie").clicked() {
                    self.apply_raster_rotation(&path, 0.);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Hoek (graden)");
                ui.add(
                    egui::TextEdit::singleline(&mut self.raster_rotation.draft)
                        .desired_width(75.)
                        .char_limit(128),
                );
                if ui.button("Draai").clicked() {
                    match rotation::parse(&self.raster_rotation.draft) {
                        Ok(angle) => self.apply_raster_rotation(&path, angle),
                        Err(error) => self.message = error,
                    }
                }
            });
            if ui
                .button(if self.raster_rotation.active() {
                    "Draaien sluiten"
                } else {
                    "Interactief draaien"
                })
                .clicked()
            {
                if self.raster_rotation.active() {
                    self.stop_raster_rotation();
                } else {
                    self.start_raster_rotation(path.clone());
                }
            }
        });
    }
    pub(super) fn handle_raster_rotation(&mut self, view: &raster_point_ui::View) {
        let camera = Camera {
            center: view.world(view.rect.center()),
            meters_per_pixel: 1. / view.scale,
            size: [view.rect.width() as f64, view.rect.height() as f64],
        };
        self.raster_rotation.last = Some(camera);
        let Some(path) = self.raster_rotation.target.clone() else {
            return;
        };
        if self.raster_rotation.generation != self.project_generation
            || self.raster_points.selected.as_ref() != Some(&path)
        {
            self.stop_raster_rotation();
            return;
        }
        let Some(index) = self.geotiffs.iter().position(|r| r.path == path) else {
            self.stop_raster_rotation();
            return;
        };
        let ctx = view.painter.ctx();
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) || view.response.secondary_clicked() {
            self.stop_raster_rotation();
            self.message = "Interactief draaien afgesloten.".into();
            return;
        }
        if !self.raster_rotation.fitted {
            let raster = &self.geotiffs[index];
            let center = sleufbase_rust::raster_points::center(raster);
            let mut radius = 0_f64;
            for (x, y) in [
                (0., 0.),
                (raster.width as f64, 0.),
                (raster.width as f64, raster.height as f64),
                (0., raster.height as f64),
            ] {
                let p = raster.transform.pixel_to_world(x, y);
                radius = radius.max((p.0 - center[0]).hypot(p.1 - center[1]));
            }
            let available = (camera.size[0].min(camera.size[1]) * 0.5 - 64.).max(14.);
            self.navigation.focus = Some(location_ui::Focus {
                center,
                meters_per_pixel: camera.meters_per_pixel.max(radius / available),
            });
            self.zoom = 1.;
            self.pan = egui::Vec2::ZERO;
            self.raster_rotation.fitted = true;
            ctx.request_repaint();
            return;
        }
        let project = |p| {
            let p = view.project(p);
            [p.x as f64, p.y as f64]
        };
        let overlay = match rotation::overlay(&self.geotiffs[index], project) {
            Ok(value) => value,
            Err(error) => {
                self.message = error;
                self.stop_raster_rotation();
                return;
            }
        };
        let pointer = ctx.input(|i| i.pointer.interact_pos());
        if ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
            && view.response.hovered()
        {
            if let Some(p) = pointer {
                if rotation::hit(&overlay, [p.x as f64, p.y as f64]) {
                    self.hold_rotation_camera();
                    self.raster_rotation.drag = Some(Drag {
                        start: rotation::screen_angle(overlay.center, [p.x as f64, p.y as f64]),
                        initial: rotation::angle(&self.geotiffs[index]),
                    });
                } else {
                    self.stop_raster_rotation();
                    return;
                }
            }
        }
        if let Some(drag) = &self.raster_rotation.drag {
            if let Some(p) = pointer {
                let angle = rotation::dragged(
                    drag.initial,
                    drag.start,
                    rotation::screen_angle(overlay.center, [p.x as f64, p.y as f64]),
                );
                match angle {
                    Ok(angle) => {
                        if !self.rotate_raster(&path, angle, false) {
                            self.stop_raster_rotation();
                            return;
                        }
                    }
                    Err(error) => self.message = error,
                }
            }
            if ctx.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
                self.raster_rotation.drag = None;
                self.message = format!(
                    "Rotatie ingesteld op {:.1}°.",
                    rotation::angle(&self.geotiffs[index])
                );
            }
            ctx.request_repaint();
        }
        if let Ok(overlay) = rotation::overlay(&self.geotiffs[index], project) {
            let pos = |p: [f64; 2]| Pos2::new(p[0] as f32, p[1] as f32);
            let painter = &view.painter;
            painter.circle_stroke(
                pos(overlay.center),
                overlay.radius as f32,
                Stroke::new(2.0_f32, ORANGE),
            );
            painter.circle_filled(pos(overlay.handle), 10., ORANGE);
            painter.circle_stroke(
                pos(overlay.handle),
                10.,
                Stroke::new(2.0_f32, Color32::WHITE),
            );
            painter.add(egui::Shape::closed_line(
                overlay.footprint.into_iter().map(pos).collect(),
                Stroke::new(1.0_f32, ORANGE),
            ));
            painter.text(
                pos(overlay.center) + egui::vec2(0., 14.),
                egui::Align2::CENTER_TOP,
                format!("{:.1}°", overlay.rotation),
                egui::FontId::proportional(13.),
                INK,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    fn app() -> SleufBaseApp {
        let data: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/raster-rotation/reference.json"
        ))
        .unwrap();
        let raster: GeoTiffInfo =
            serde_json::from_value(data["cases"][0]["input"].clone()).unwrap();
        let mut app = SleufBaseApp {
            geotiffs: vec![raster],
            migrated_settings: MigratedSettings::default(),
            message: String::new(),
            ..Default::default()
        };
        app.raster_points.selected = Some(app.geotiffs[0].path.clone());
        app.raster_points.online = false;
        app
    }
    fn frame(
        app: &mut SleufBaseApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> raster_point_ui::View {
        let mut result = None;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1000., 800.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    app.navigation.pointer_tool = app.raster_rotation.active();
                    let view = canvas(
                        ui,
                        &[],
                        &app.geotiffs,
                        &app.raster_textures,
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
                    app.handle_raster_rotation(&view);
                    result = Some(view);
                });
            },
        );
        result.unwrap()
    }
    #[test]
    fn discrete_rotations_hold_the_camera_reuse_texture_and_preserve_measured_points() {
        let ctx = egui::Context::default();
        let mut app = app();
        let path = app.geotiffs[0].path.clone();
        app.zoom = 1.7;
        app.pan = egui::vec2(24., -17.);
        app.raster_textures.push(Some(ctx.load_texture(
            "rotation pixels",
            egui::ColorImage::new([8, 6], Color32::from_rgb(10, 80, 170)),
            egui::TextureOptions::NEAREST,
        )));
        let texture = app.raster_textures[0].as_ref().unwrap().id();
        let pointer = app.geotiffs.as_ptr();
        let metadata = app.geotiffs[0].metadata.clone();
        let view = frame(&mut app, &ctx, Vec::new());
        let probe = sleufbase_rust::raster_points::center(&app.geotiffs[0]);
        let before = view.project(probe);
        app.apply_raster_rotation(&path, 37.5);
        let view = frame(&mut app, &ctx, Vec::new());
        assert!(view.project(probe).distance(before) < 0.001);
        assert!(app.dirty);
        assert_eq!(app.geotiffs.as_ptr(), pointer);
        assert_eq!(app.raster_textures[0].as_ref().unwrap().id(), texture);
        assert_eq!(
            app.geotiffs[0].metadata["template_reference_point"],
            metadata["template_reference_point"]
        );
        assert_eq!(
            app.geotiffs[0].metadata["template_cross_section_start_point"],
            metadata["template_cross_section_start_point"]
        );
        app.apply_raster_rotation(&path, 0.);
        app.dirty = false;
        app.apply_raster_rotation(&path, 0.);
        assert!(!app.dirty);
        assert!(app.message.starts_with("Rotatie gereset"));
    }
    #[test]
    fn actual_canvas_press_drag_release_uses_original_positive_direction_without_panning() {
        let ctx = egui::Context::default();
        let mut app = app();
        let path = app.geotiffs[0].path.clone();
        frame(&mut app, &ctx, Vec::new());
        app.start_raster_rotation(path);
        frame(&mut app, &ctx, Vec::new());
        let view = frame(&mut app, &ctx, Vec::new());
        let overlay = rotation::overlay(&app.geotiffs[0], |p| {
            let p = view.project(p);
            [p.x as f64, p.y as f64]
        })
        .unwrap();
        let press = Pos2::new(overlay.handle[0] as f32, overlay.handle[1] as f32);
        assert!(view.rect.contains(press));
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(press),
                egui::Event::PointerButton {
                    pos: press,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(app.raster_rotation.drag.is_some());
        let radians = 113_f64.to_radians();
        let target = Pos2::new(
            (overlay.center[0] + radians.cos() * overlay.radius) as f32,
            (overlay.center[1] - radians.sin() * overlay.radius) as f32,
        );
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(target)]);
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(target),
                egui::Event::PointerButton {
                    pos: target,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let original: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/raster-rotation/reference.json"
        ))
        .unwrap();
        let expected = original["cases"][0]["drag"]["moves"][0]["expected"]
            .as_f64()
            .unwrap();
        assert!((rotation::angle(&app.geotiffs[0]) - expected).abs() < 0.0001);
        assert_eq!(app.pan, egui::Vec2::ZERO);
        assert!(app.dirty);
        assert!(app.raster_rotation.drag.is_none());
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
        assert!(!app.raster_rotation.active());
        assert!(!app.navigation.pointer_tool);
        assert!((rotation::angle(&app.geotiffs[0]) - expected).abs() < 0.0001);
        app.start_raster_rotation(app.geotiffs[0].path.clone());
        frame(&mut app, &ctx, Vec::new());
        let view = frame(&mut app, &ctx, Vec::new());
        let p = view.rect.center();
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(p),
                egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Middle,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(p + egui::vec2(16., 11.))],
        );
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: p + egui::vec2(16., 11.),
                button: egui::PointerButton::Middle,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(app.pan, egui::vec2(16., 11.));
        assert!(app.raster_rotation.active());
        assert!((rotation::angle(&app.geotiffs[0]) - expected).abs() < 0.0001);
    }
    #[test]
    fn switching_or_clearing_projects_discards_rotation_state_and_stale_camera() {
        let ctx = egui::Context::default();
        let mut app = app();
        let path = app.geotiffs[0].path.clone();
        frame(&mut app, &ctx, Vec::new());
        app.start_raster_rotation(path);
        assert!(app.raster_rotation.active());
        app.request_project_action(ProjectAction::ClearLayers, &ctx);
        assert!(!app.raster_rotation.active());
        assert!(app.raster_rotation.last.is_none());
        assert!(!app.navigation.pointer_tool);
        app.apply_project(NativeProject::default(), None, false, Vec::new(), &ctx);
        assert!(!app.raster_rotation.active());
        assert!(app.raster_rotation.last.is_none());
    }
    #[test]
    fn actual_header_buttons_remain_inside_their_clip_at_supported_small_widths() {
        let expected = [
            "Nieuw project",
            "Open Rust-project",
            "Opslaan als...",
            "Import Python-project",
            "Opmerking plaatsen",
            "Kaartacties",
            "Export kaarten",
            "Kadaster WFS",
            "MarXact importeren",
            "Excel disciplines",
            "DXF objecten",
            "DXF-sjabloon",
            "Kaart terugzetten",
        ];
        for width in [900., 1200., 1500.] {
            let ctx = egui::Context::default();
            let mut app = SleufBaseApp {
                migrated_settings: MigratedSettings::default(),
                message: String::new(),
                ..Default::default()
            };
            let input = || egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(width, 800.),
                )),
                ..Default::default()
            };
            let _ = ctx.run(input(), |ctx| app.render(ctx));
            let out = ctx.run(input(), |ctx| app.render(ctx));
            for text in expected {
                let shape = out
                    .shapes
                    .iter()
                    .find(|s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==text))
                    .unwrap_or_else(|| panic!("Missing toolbar {text} at {width}"));
                assert!(
                    shape
                        .clip_rect
                        .expand(0.5)
                        .contains_rect(shape.shape.visual_bounding_rect()),
                    "Clipped toolbar {text} at {width}: {:?}",
                    shape.shape.visual_bounding_rect()
                );
            }
        }
    }
}
