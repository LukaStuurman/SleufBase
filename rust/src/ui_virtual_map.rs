//! Original two-press placement and confirmed whole-layer moves on the real map.
use super::*;
use sleufbase_rust::virtual_map as map;
type XY = [f64; 2];
#[derive(Clone)]
struct Target {
    index: usize,
    label: String,
    start: Vertex,
    end: Vertex,
    objects: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    fn app() -> SleufBaseApp {
        let data: Value =
            serde_json::from_str(include_str!("../tests/fixtures/virtual-map/reference.json"))
                .unwrap();
        let trench: VirtualTrench =
            serde_json::from_value(data["cases"][1]["source"].clone()).unwrap();
        let center = map::center(&trench);
        SleufBaseApp {
            virtual_trenches: vec![trench],
            navigation: location_ui::Navigation {
                focus: Some(location_ui::Focus {
                    center,
                    meters_per_pixel: 0.05,
                }),
                ..Default::default()
            },
            ..Default::default()
        }
    }
    fn frame(
        app: &mut SleufBaseApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> raster_point_ui::View {
        if app.trench_markers.len() != app.virtual_trenches.len() {
            app.trench_markers = app
                .virtual_trenches
                .iter()
                .map(sleufbase_rust::cross_section::markers)
                .collect();
            app.refresh_virtual_references();
        }
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
                    app.navigation.pointer_tool = app.virtual_map.captures();
                    let view = canvas(
                        ui,
                        &[],
                        &[],
                        &[],
                        &[],
                        &app.virtual_trenches,
                        &app.trench_markers,
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
                    app.handle_virtual_map(&view);
                    result = Some(view);
                });
            },
        );
        result.unwrap()
    }
    fn button(pos: Pos2, button: egui::PointerButton, pressed: bool) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: Default::default(),
            },
        ]
    }
    fn click(app: &mut SleufBaseApp, ctx: &egui::Context, pos: Pos2) {
        for pressed in [true, false] {
            frame(app, ctx, button(pos, egui::PointerButton::Primary, pressed));
        }
    }
    #[test]
    fn real_canvas_move_retains_camera_source_vectors_z_and_does_not_pan_primary() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.zoom = 1.7;
        app.pan = egui::vec2(24., -17.);
        let before = app.virtual_trenches[0].clone();
        let source = app.virtual_trenches[0].objects.as_ptr();
        let view = frame(&mut app, &ctx, Vec::new());
        let anchor = map::center(&before);
        let fixed = view.project(anchor);
        let target = [anchor[0] + 1.3, anchor[1] + 2.7];
        app.start_virtual_move(0, None);
        let at = view.project(target);
        click(&mut app, &ctx, at);
        let view = frame(&mut app, &ctx, Vec::new());
        assert!(view.project(anchor).distance(fixed) < 0.001);
        assert_eq!(app.virtual_trenches[0].objects.as_ptr(), source);
        assert!(app.dirty);
        assert!(app.virtual_map.mode.is_none());
        assert!(!app.virtual_map.capture);
        let after = &app.virtual_trenches[0];
        assert!((after.start.x - before.start.x - 1.3).abs() < 2e-5);
        assert!((after.start.y - before.start.y - 2.7).abs() < 2e-5);
        assert_eq!(after.start.z, before.start.z);
        assert_eq!(after.metadata, before.metadata);
        app.start_virtual_move(0, None);
        let current = serde_json::to_vec(&app.virtual_trenches).unwrap();
        let p = view.rect.center();
        frame(&mut app, &ctx, button(p, egui::PointerButton::Middle, true));
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(p + egui::vec2(12., 9.))],
        );
        frame(
            &mut app,
            &ctx,
            button(p + egui::vec2(12., 9.), egui::PointerButton::Middle, false),
        );
        assert_eq!(app.pan, egui::vec2(12., 9.));
        assert_eq!(serde_json::to_vec(&app.virtual_trenches).unwrap(), current);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert!(app.virtual_map.mode.is_none());
        assert_eq!(serde_json::to_vec(&app.virtual_trenches).unwrap(), current);
    }
    #[test]
    fn empty_project_two_presses_enforce_original_minimum_and_open_staged_editor() {
        let ctx = egui::Context::default();
        let mut app = SleufBaseApp::default();
        app.start_virtual_creation();
        let view = frame(&mut app, &ctx, Vec::new());
        let p = view.rect.center();
        click(&mut app, &ctx, p);
        assert!(matches!(
            app.virtual_map.mode,
            Some(Mode::Create { start: Some(_), .. })
        ));
        assert!(!app.dirty);
        click(&mut app, &ctx, p + egui::vec2(1., 1.));
        assert!(app.virtual_trenches.is_empty());
        assert!(app.virtual_map.mode.is_some());
        click(&mut app, &ctx, p + egui::vec2(20., -10.));
        assert_eq!(app.virtual_trenches.len(), 1);
        assert!(app.trench_editor.is_some());
        assert!(app.dirty);
        assert!(app.virtual_map.mode.is_none());
        assert!(app.virtual_trenches[0].start.z.is_none());
        assert!((app.virtual_trenches[0].length() - 5_f64.sqrt()).abs() < 2e-5);
    }
    #[test]
    fn actual_reference_click_roundtrips_rotation_escape_and_global_clear() {
        let ctx = egui::Context::default();
        let mut app = app();
        let view = frame(&mut app, &ctx, Vec::new());
        let p = view.rect.center() + egui::vec2(25., -10.);
        let world = view.world(p);
        app.start_virtual_reference(0);
        click(&mut app, &ctx, p);
        let reference = map::reference(&app.virtual_trenches[0]).unwrap().unwrap();
        assert!((reference[0] - world[0]).abs() < 2e-5);
        assert!((reference[1] - world[1]).abs() < 2e-5);
        assert!(app.dirty);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert!(app.virtual_map.mode.is_none());
        assert_eq!(
            map::reference(&app.virtual_trenches[0]).unwrap().unwrap(),
            reference
        );
        app.clear_raster_references();
        assert!(map::reference(&app.virtual_trenches[0]).unwrap().is_none());
        app.dirty = false;
        app.clear_raster_references();
        assert!(!app.dirty);
    }
    #[test]
    fn project_open_and_prepared_marxact_import_keep_reference_markers() {
        let ctx = egui::Context::default();
        let mut app = app();
        let point = map::raster_center(&app.virtual_trenches[0]);
        map::set_reference(&mut app.virtual_trenches[0], point).unwrap();
        let document = NativeProject {
            virtual_trenches: app.virtual_trenches.clone(),
            ..Default::default()
        };
        let markers = document
            .virtual_trenches
            .iter()
            .map(sleufbase_rust::cross_section::markers)
            .collect();
        app.apply_project(document, None, false, markers, &ctx);
        assert_eq!(app.virtual_map.references.len(), 1);
        let before = app.virtual_map.references[0];
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        app.import_marxact(vec![root.join("marxact-cad/nested-leaders.dxf")]);
        let deadline = Instant::now() + Duration::from_secs(15);
        while app.file_intent.is_some() {
            assert!(Instant::now() < deadline, "MarXact import timeout");
            app.poll_file_jobs(&ctx);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(app.virtual_trenches.len() > 1);
        assert_eq!(app.trench_markers.len(), app.virtual_trenches.len());
        assert_eq!(app.virtual_map.references, vec![before]);
    }
    #[test]
    fn real_point_selection_axis_snap_end_transform_and_cancel(){
        let ctx=egui::Context::default();let mut app=app();
        app.virtual_trenches[0].objects.truncate(1);
        let center=map::center(&app.virtual_trenches[0]);app.virtual_trenches[0].objects[0].position.x=center[0];app.virtual_trenches[0].objects[0].position.y=center[1];
        let before=app.virtual_trenches[0].clone();let view=frame(&mut app,&ctx,Vec::new());
        app.start_virtual_points(0,false);click(&mut app,&ctx,view.project(center));assert!(matches!(app.virtual_map.mode,Some(Mode::MovePoint{point:map::MapPoint::Object(0),..})));
        let target=[center[0]+0.7,center[1]+0.8];let(x,y,_,_)=before.project(Vertex{x:target[0],y:target[1],z:None});click(&mut app,&ctx,view.project(target));
        assert!((app.virtual_trenches[0].objects[0].position.x-x).abs()<2e-5);assert!((app.virtual_trenches[0].objects[0].position.y-y).abs()<2e-5);assert_eq!(app.virtual_trenches[0].objects[0].position.z,before.objects[0].position.z);
        let view=frame(&mut app,&ctx,Vec::new());let end=app.virtual_trenches[0].end;app.start_virtual_points(0,false);click(&mut app,&ctx,view.project([end.x,end.y]));assert!(matches!(app.virtual_map.mode,Some(Mode::MovePoint{point:map::MapPoint::End,..})));
        let target=[end.x+2.,end.y+1.];click(&mut app,&ctx,view.project(target));assert!((app.virtual_trenches[0].end.x-target[0]).abs()<2e-5);assert_eq!(app.virtual_trenches[0].end.z,end.z);
        app.start_virtual_points(0,false);let bytes=serde_json::to_vec(&app.virtual_trenches).unwrap();frame(&mut app,&ctx,button(view.rect.center(),egui::PointerButton::Secondary,true));frame(&mut app,&ctx,button(view.rect.center(),egui::PointerButton::Secondary,false));assert!(app.virtual_map.mode.is_none());assert_eq!(serde_json::to_vec(&app.virtual_trenches).unwrap(),bytes);
    }
    #[test]
    fn real_copy_direction_dekband_and_pending_depth_survive_project(){
        let ctx=egui::Context::default();let mut app=app();let view=frame(&mut app,&ctx,Vec::new());let center=map::center(&app.virtual_trenches[0]);let length=app.virtual_trenches[0].length();
        app.start_virtual_copy(0);let start=[center[0]+4.,center[1]+3.];click(&mut app,&ctx,view.project(start));click(&mut app,&ctx,view.project([start[0],start[1]+2.]));assert_eq!(app.virtual_trenches.len(),2);assert!((app.virtual_trenches[1].length()-length).abs()<2e-5);assert!(app.trench_editor.is_some());
        app.trench_editor=None;
        // Copy placement preserves the old camera. The 19.5 m copy runs north
        // beyond that viewport; centre it as a user would before picking its
        // far endpoint. An off-canvas pointer must never place a deck band.
        app.navigation.focus=Some(location_ui::Focus{center:map::center(&app.virtual_trenches[1]),meters_per_pixel:0.05});
        let view=frame(&mut app,&ctx,Vec::new());
        app.start_virtual_dekband(1);
        let t=&app.virtual_trenches[1];let a=[t.start.x,t.start.y+1.];let b=[t.end.x,t.end.y-1.];
        assert!(view.rect.contains(view.project(a))&&view.rect.contains(view.project(b)),"both deck-band picks must be visible");
        click(&mut app,&ctx,view.project(a));
        assert!(matches!(app.virtual_map.mode,Some(Mode::Dekband{start:Some(_),..})),"first pick starts a deck band");
        click(&mut app,&ctx,view.project(b));
        assert_eq!(app.virtual_trenches[1].dekband_rows.len(),1);
        assert_eq!(app.virtual_trenches[1].dekband_rows[0].source_name,"Dekband");
        app.virtual_trenches[1].start.z=None;app.virtual_trenches[1].start_metadata=serde_json::Value::Null;app.virtual_map.point_name="Nieuwe LS".into();app.virtual_map.point_source="LS".into();app.virtual_map.point_depth=0.7;let center=map::center(&app.virtual_trenches[1]);app.start_virtual_points(1,true);click(&mut app,&ctx,view.project(center));
        let object=app.virtual_trenches[1].objects.iter().find(|p|p.object_name=="Nieuwe LS").unwrap();assert!(object.position.z.is_none());assert_eq!(object.metadata["sleufbase_depth_meters"],0.7);
        map::apply_surface_height(&mut app.virtual_trenches[1],map::MapPoint::Start,Some(3.2)).unwrap();let object=app.virtual_trenches[1].objects.iter().find(|p|p.object_name=="Nieuwe LS").unwrap();assert_eq!(object.position.z,Some(2.5));assert!(app.dirty);
    }
    #[test]
    fn source_changes_and_project_switches_discard_stale_modes() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.start_virtual_move(0, None);
        app.virtual_trenches[0].start.x += 1.;
        let _ = frame(&mut app, &ctx, Vec::new());
        assert!(app.virtual_map.mode.is_none());
        assert!(!app.dirty);
        app.start_virtual_reference(0);
        app.apply_project(NativeProject::default(), None, false, Vec::new(), &ctx);
        assert!(!app.virtual_map.captures());
        assert!(app.virtual_map.selected.is_none());
        assert!(app.virtual_map.references.is_empty());
    }
}
impl Target {
    fn new(index: usize, t: &VirtualTrench) -> Self {
        Self {
            index,
            label: t.label.clone(),
            start: t.start,
            end: t.end,
            objects: t.objects.len(),
        }
    }
    fn valid(&self, trenches: &[VirtualTrench]) -> bool {
        trenches.get(self.index).is_some_and(|t| {
            t.label == self.label
                && t.start == self.start
                && t.end == self.end
                && t.objects.len() == self.objects
        })
    }
}
enum Mode {
    Create {
        label: String,
        width: f64,
        start: Option<XY>,
    },
    Move {
        target: Target,
        anchor: XY,
    },
    Reference(Target),
    SelectPoint(Target),
    MovePoint {target:Target,point:map::MapPoint,original:Vertex},
    AddPoint(Target),
    Copy {target:Target,label:String,start:Option<XY>},
    Dekband {target:Target,start:Option<XY>},
}
#[derive(Default)]
pub(super) struct State {
    pub selected: Option<usize>,
    mode: Option<Mode>,
    generation: u64,
    capture: bool,
    pointer: Option<XY>,
    context_anchor: Option<XY>,
    references: Vec<(usize, XY)>,
    point_name:String,
    point_source:String,
    point_depth:f64,
    band_name:String,
    band_rgb:[u8;3],
    band_start_depth:f64,
    band_end_depth:f64,
}
impl State {
    pub fn captures(&self) -> bool {
        self.mode.is_some() || self.capture
    }
}
impl SleufBaseApp {
    pub(super) fn cancel_virtual_map(&mut self) {
        self.virtual_map.mode = None;
        self.virtual_map.pointer = None;
    }
    pub(super) fn refresh_virtual_references(&mut self) {
        self.virtual_map.references = self
            .virtual_trenches
            .iter()
            .enumerate()
            .filter_map(|(i, t)| map::reference(t).ok().flatten().map(|p| (i, p)))
            .collect();
        if self
            .virtual_map
            .selected
            .is_some_and(|i| i >= self.virtual_trenches.len())
        {
            self.virtual_map.selected = None;
        }
    }
    fn begin_virtual_map(&mut self) -> bool {
        if self.file_intent.is_some()
            || self.export_intent.is_some()
            || self.template_job.is_some()
            || self.trench_editor.is_some()
        {
            self.message = "Sluit de sleufeditor of rond de lopende taak eerst af.".into();
            return false;
        }
        self.stop_raster_rotation();
        self.raster_points.mode = None;
        self.raster_points.rows.clear();
        self.cancel_virtual_map();
        self.virtual_map.generation = self.project_generation;
        if self.navigation.focus.is_none()
            && self.virtual_trenches.is_empty()
            && self.geotiffs.is_empty()
            && self.datasets.is_empty()
            && self.overlays.is_empty()
        {
            self.navigation.focus = Some(location_ui::Focus {
                center: [155000., 463000.],
                meters_per_pixel: 0.1,
            });
            self.zoom = 1.;
            self.pan = egui::Vec2::ZERO;
        }
        true
    }
    fn start_virtual_creation(&mut self) {
        if !self.begin_virtual_map() {
            return;
        }
        let names = self
            .virtual_trenches
            .iter()
            .map(|t| std::borrow::Cow::Borrowed(t.label.as_str()))
            .chain(
                self.geotiffs
                    .iter()
                    .filter_map(|r| r.path.file_stem().map(|n| n.to_string_lossy())),
            );
        let name = map::next_name(names);
        self.virtual_map.mode = Some(Mode::Create {
            label: name,
            width: self.trench_form.width,
            start: None,
        });
        self.message =
            "Klik voor het beginpunt en daarna voor het eindpunt. Escape of rechtsklik annuleert."
                .into();
    }
    fn start_virtual_move(&mut self, index: usize, anchor: Option<XY>) {
        if !self.begin_virtual_map() {
            return;
        }
        let Some(t) = self.virtual_trenches.get(index) else {
            return;
        };
        let anchor = anchor.unwrap_or_else(|| map::center(t));
        self.virtual_map.selected = Some(index);
        self.virtual_map.mode = Some(Mode::Move {
            target: Target::new(index, t),
            anchor,
        });
        self.message =
            "Beweeg naar de nieuwe locatie en klik om de hele proefsleuf te verplaatsen.".into();
    }
    fn start_virtual_reference(&mut self, index: usize) {
        if !self.begin_virtual_map() {
            return;
        }
        let Some(t) = self.virtual_trenches.get(index) else {
            return;
        };
        self.virtual_map.selected = Some(index);
        self.virtual_map.mode = Some(Mode::Reference(Target::new(index, t)));
        self.message =
            "Klik om het referentiepunt van deze proefsleuf te plaatsen of te verplaatsen.".into();
    }
    fn start_virtual_points(&mut self,index:usize,adding:bool){
        if !self.begin_virtual_map(){return;}
        let Some(t)=self.virtual_trenches.get(index) else{return};
        self.virtual_map.selected=Some(index);
        self.virtual_map.mode=Some(if adding{Mode::AddPoint(Target::new(index,t))}else{Mode::SelectPoint(Target::new(index,t))});
        self.message=if adding{"Klik voor een meetobject; het punt wordt op de meetas vastgezet."}else{"Klik op het beginpunt, eindpunt of meetobject en daarna op de nieuwe locatie."}.into();
    }
    fn start_virtual_copy(&mut self,index:usize){
        if !self.begin_virtual_map(){return;}
        let Some(t)=self.virtual_trenches.get(index) else{return};
        let label=map::next_name(self.virtual_trenches.iter().map(|t|std::borrow::Cow::Borrowed(t.label.as_str())).chain(self.geotiffs.iter().filter_map(|r|r.path.file_stem().map(|n|n.to_string_lossy()))));
        self.virtual_map.mode=Some(Mode::Copy{target:Target::new(index,t),label,start:None});
        self.message="Prik het nieuwe beginpunt en daarna de richting. De gekopieerde sleuf behoudt zijn lengte.".into();
    }
    fn start_virtual_dekband(&mut self,index:usize){
        if !self.begin_virtual_map(){return;}
        let Some(t)=self.virtual_trenches.get(index) else{return};
        if self.virtual_map.band_name.is_empty(){self.virtual_map.band_name="Dekband".into();self.virtual_map.band_rgb=[17,24,39];}
        self.virtual_map.mode=Some(Mode::Dekband{target:Target::new(index,t),start:None});
        self.message="Prik begin en einde van de dekband op de meetas.".into();
    }
    pub(super) fn show_virtual_map_controls(&mut self, ui: &mut egui::Ui) {
        if ui.button("Nieuwe sleuf op kaart tekenen").clicked() {
            self.start_virtual_creation();
        }
        if let Some(i) = self
            .virtual_map
            .selected
            .filter(|&i| i < self.virtual_trenches.len())
        {
            ui.small(format!("Geselecteerd: {}", self.virtual_trenches[i].label));
            ui.horizontal_wrapped(|ui| {
                if ui.button("Meetpunt verplaatsen").clicked(){self.start_virtual_points(i,false);}
                if ui.button("Meetobject prikken").clicked(){self.start_virtual_points(i,true);}
                if ui.button("Kopie met nieuwe richting").clicked(){self.start_virtual_copy(i);}
                if ui.button("Dekband prikken").clicked(){self.start_virtual_dekband(i);}
                if ui.button("Verplaatsen op kaart").clicked() {
                    self.start_virtual_move(i, None);
                }
                if ui.button("Virtueel referentiepunt plaatsen").clicked() {
                    self.start_virtual_reference(i);
                }
                if ui.button("Virtueel referentiepunt wissen").clicked() {
                    let changed = map::clear_reference(&mut self.virtual_trenches[i]);
                    self.clear_virtual_editor_metadata(
                        sleufbase_rust::raster_points::REFERENCE,
                        Some(i),
                    );
                    self.dirty |= changed;
                    if changed {
                        self.trench_markers.clear();
                    }
                    self.message = if changed {
                        "Referentiepunt gewist."
                    } else {
                        "Geen referentiepunt om te wissen."
                    }
                    .into();
                }
            });
            if matches!(self.virtual_map.mode,Some(Mode::AddPoint(_))){
                ui.horizontal_wrapped(|ui|{ui.label("Object");ui.text_edit_singleline(&mut self.virtual_map.point_name);ui.label("Bron");ui.text_edit_singleline(&mut self.virtual_map.point_source);ui.label("Diepte onder beginmaaiveld (m)");ui.add(egui::DragValue::new(&mut self.virtual_map.point_depth).range(0.0..=1000.).speed(0.05));});
            }
            if matches!(self.virtual_map.mode,Some(Mode::Dekband{..})){
                ui.horizontal_wrapped(|ui|{ui.label("Dekband");ui.text_edit_singleline(&mut self.virtual_map.band_name);ui.color_edit_button_srgb(&mut self.virtual_map.band_rgb);ui.label("Begindiepte (m)");ui.add(egui::DragValue::new(&mut self.virtual_map.band_start_depth).range(0.0..=1000.).speed(0.05));ui.label("Einddiepte (m)");ui.add(egui::DragValue::new(&mut self.virtual_map.band_end_depth).range(0.0..=1000.).speed(0.05));});
            }
        }
        if self.virtual_map.mode.is_some() && ui.button("Kaartbewerking sluiten").clicked() {
            self.cancel_virtual_map();
        }
    }
    fn hold_virtual_camera(&mut self, view: &raster_point_ui::View) {
        self.navigation.focus = Some(location_ui::Focus {
            center: view.world(view.rect.center()),
            meters_per_pixel: 1. / view.scale,
        });
        self.zoom = 1.;
        self.pan = egui::Vec2::ZERO;
    }
    pub(super) fn handle_virtual_map(&mut self, view: &raster_point_ui::View) {
        let ctx = view.painter.ctx();
        let released = ctx.input(|i| i.pointer.button_released(egui::PointerButton::Primary));
        let consumed = self.virtual_map.capture;
        if released {
            self.virtual_map.capture = false;
        }
        if self.virtual_map.generation != self.project_generation && self.virtual_map.mode.is_some()
        {
            self.cancel_virtual_map();
        }
        let valid = match &self.virtual_map.mode {
            Some(Mode::Move { target, .. } | Mode::Reference(target) | Mode::SelectPoint(target) | Mode::AddPoint(target) | Mode::Copy{target,..} | Mode::Dekband{target,..}) => {
                target.valid(&self.virtual_trenches)
            }
            Some(Mode::MovePoint{target,point,original})=>target.valid(&self.virtual_trenches)&&map::position(&self.virtual_trenches[target.index],*point)==Some(*original),
            _ => true,
        };
        if !valid {
            self.cancel_virtual_map();
            self.message = "De gekozen proefsleuf is gewijzigd; kies de kaartactie opnieuw.".into();
        }
        if self.virtual_map.mode.is_some()
            && (ctx.input(|i| i.key_pressed(egui::Key::Escape))
                || view.response.secondary_clicked())
        {
            self.cancel_virtual_map();
            self.message = "Kaartbewerking geannuleerd.".into();
            return;
        }
        let pointer = ctx
            .input(|i| i.pointer.interact_pos())
            .filter(|p| view.rect.contains(*p));
        if let Some(p) = pointer {
            self.virtual_map.pointer = Some(view.world(p));
        }
        // Point/rotation tools own their clicks. Normal virtual selection must
        // never steal an active physical-raster tool's event.
        if self.raster_rotation.active() || self.raster_points.mode.is_some() {
            return;
        }
        if let Some(Mode::Reference(target)) = &self.virtual_map.mode {
            if let Some(p) = view.clicked {
                let i = target.index;
                match map::set_reference(&mut self.virtual_trenches[i], view.world(p)) {
                    Ok(changed) => {
                        self.dirty |= changed;
                        if changed {
                            self.trench_markers.clear();
                            self.refresh_virtual_references();
                        }
                        self.message = "Virtueel referentiepunt bijgewerkt.".into();
                    }
                    Err(e) => self.message = e,
                }
            }
        } else if self.virtual_map.mode.is_some()
            && view.response.hovered()
            && ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
        {
            if let Some(p) = pointer {
                self.virtual_map.capture = true;
                let world = view.world(p);
                match self.virtual_map.mode.take().unwrap() {
                    Mode::Create {
                        label,
                        width,
                        start: None,
                    } => {
                        self.virtual_map.mode = Some(Mode::Create {
                            label,
                            width,
                            start: Some(world),
                        });
                        self.message =
                            "Klik nu voor het eindpunt, op minstens 0,5 m afstand.".into();
                    }
                    Mode::Create {
                        label,
                        width,
                        start: Some(start),
                    } => match map::create(label.clone(), start, world, width) {
                        Ok(t) => {
                            self.hold_virtual_camera(view);
                            self.virtual_trenches.push(t);
                            let i = self.virtual_trenches.len() - 1;
                            self.virtual_map.selected = Some(i);
                            self.trench_markers.clear();
                            self.dirty = true;
                            self.message="Virtuele proefsleuf geplaatst. Vul de metingen en hoogtes in de editor in.".into();
                            self.open_virtual_editor(i);
                        }
                        Err(e) => {
                            self.message = e;
                            self.virtual_map.mode = Some(Mode::Create {
                                label,
                                width,
                                start: Some(start),
                            });
                        }
                    },
                    Mode::Move { target, anchor } => {
                        match map::translate(
                            &mut self.virtual_trenches[target.index],
                            world[0] - anchor[0],
                            world[1] - anchor[1],
                        ) {
                            Ok(changed) => {
                                self.hold_virtual_camera(view);
                                self.dirty |= changed;
                                self.trench_markers.clear();
                                self.message="Hele proefsleuf verplaatst; hoogtes en bronbestand zijn behouden.".into();
                            }
                            Err(e) => {
                                self.message = e;
                                self.virtual_map.mode = Some(Mode::Move { target, anchor });
                            }
                        }
                    }
                    Mode::SelectPoint(target)=>{
                        if let Some(point)=map::pick_point(&self.virtual_trenches[target.index],world,1./view.scale,(6./view.scale).clamp(0.001,3.)){
                            let original=map::position(&self.virtual_trenches[target.index],point).unwrap();
                            self.virtual_map.mode=Some(Mode::MovePoint{target,point,original});self.message="Klik op de nieuwe plek. Meetobjecten klikken vast op de meetas.".into();
                        }else{self.message="Klik dichter bij een meetpunt.".into();self.virtual_map.mode=Some(Mode::SelectPoint(target));}
                    }
                    Mode::MovePoint{target,point,original}=>{
                        match map::move_point(&mut self.virtual_trenches[target.index],point,world){
                            Ok(changed)=>{self.hold_virtual_camera(view);self.dirty|=changed;self.trench_markers.clear();self.message="Meetpunt verplaatst; meetwaarden behouden.".into();}
                            Err(e)=>{self.message=e;self.virtual_map.mode=Some(Mode::MovePoint{target,point,original});}
                        }
                    }
                    Mode::AddPoint(target)=>{
                        let trench=&self.virtual_trenches[target.index];let (x,y,_,_)=trench.project(Vertex{x:world[0],y:world[1],z:None});
                        let z=trench.start.z.map(|z|z-self.virtual_map.point_depth);
                        let object=sleufbase_rust::virtual_trench::TrenchPoint{metadata:serde_json::json!({"surface_mode":"start","surface_status":"start","surface_z":trench.start.z,"sleufbase_depth_meters":self.virtual_map.point_depth}),role:"object".into(),object_name:if self.virtual_map.point_name.trim().is_empty(){format!("Punt {}",trench.objects.len()+1)}else{self.virtual_map.point_name.clone()},source_name:self.virtual_map.point_source.clone(),position:Vertex{x,y,z},attributes:Default::default()};
                        match sleufbase_rust::virtual_trench_editor::replace_object(&mut self.virtual_trenches[target.index],None,object){
                            Ok(())=>{self.hold_virtual_camera(view);self.dirty=true;self.trench_markers.clear();self.message="Meetobject geplaatst op de meetas.".into();self.open_virtual_editor(target.index);}
                            Err(e)=>{self.message=e;self.virtual_map.mode=Some(Mode::AddPoint(target));}
                        }
                    }
                    Mode::Copy{target,label,start:None}=>{self.virtual_map.mode=Some(Mode::Copy{target,label,start:Some(world)});self.message="Klik nu de richting van de kopie.".into();}
                    Mode::Copy{target,label,start:Some(start)}=>{
                        match map::copy_in_direction(&self.virtual_trenches[target.index],label.clone(),start,world){
                            Ok(copy)=>{self.hold_virtual_camera(view);self.virtual_trenches.push(copy);let i=self.virtual_trenches.len()-1;self.virtual_map.selected=Some(i);self.trench_markers.clear();self.dirty=true;self.open_virtual_editor(i);}
                            Err(e)=>{self.message=e;self.virtual_map.mode=Some(Mode::Copy{target,label,start:Some(start)});}
                        }
                    }
                    Mode::Dekband{target,start:None}=>{self.virtual_map.mode=Some(Mode::Dekband{target,start:Some(world)});self.message="Prik nu het andere einde van de dekband.".into();}
                    Mode::Dekband{target,start:Some(start)}=>{
                        match map::add_dekband_with_depth(&mut self.virtual_trenches[target.index],start,world,self.virtual_map.band_name.clone(),Some(self.virtual_map.band_rgb),self.virtual_map.band_start_depth,self.virtual_map.band_end_depth){
                            Ok(changed)=>{self.dirty|=changed;self.trench_markers.clear();self.message="Dekband geplaatst; omschrijving en kleur worden opgeslagen en geëxporteerd.".into();}
                            Err(e)=>{self.message=e;self.virtual_map.mode=Some(Mode::Dekband{target,start:Some(start)});}
                        }
                    }
                    Mode::Reference(_) => unreachable!(),
                }
            }
        } else if self.virtual_map.mode.is_none() && !consumed {
            if let Some(p) = view.clicked {
                self.virtual_map.selected = map::pick(
                    &self.virtual_trenches,
                    view.world(p),
                    1. / view.scale,
                    (6. / view.scale).clamp(0.001, 3.),
                );
            }
            if view.response.secondary_clicked() {
                if let Some(p) = view.response.interact_pointer_pos() {
                    self.virtual_map.context_anchor = Some(view.world(p));
                    self.virtual_map.selected = map::pick(
                        &self.virtual_trenches,
                        view.world(p),
                        1. / view.scale,
                        (6. / view.scale).clamp(0.001, 3.),
                    );
                }
            }
            if let Some(i) = self
                .virtual_map
                .selected
                .filter(|&i| i < self.virtual_trenches.len())
            {
                view.response.context_menu(|ui| {
                    if ui.button("Proefsleuf bewerken").clicked() {
                        self.open_virtual_editor(i);
                        ui.close_menu();
                    }
                    if ui.button("Hele proefsleuf verplaatsen").clicked() {
                        self.start_virtual_move(i, self.virtual_map.context_anchor);
                        ui.close_menu();
                    }
                    if ui.button("Meetpunt verplaatsen").clicked(){self.start_virtual_points(i,false);ui.close_menu();}
                    if ui.button("Meetobject prikken").clicked(){self.start_virtual_points(i,true);ui.close_menu();}
                    if ui.button("Kopie met nieuwe richting").clicked(){self.start_virtual_copy(i);ui.close_menu();}
                    if ui.button("Dekband prikken").clicked(){self.start_virtual_dekband(i);ui.close_menu();}
                    if ui.button("Referentiepunt plaatsen").clicked() {
                        self.start_virtual_reference(i);
                        ui.close_menu();
                    }
                });
            }
        }
        let painter = &view.painter;
        for (i, p) in &self.virtual_map.references {
            let p = view.project(*p);
            painter.circle_stroke(p, 7., Stroke::new(2.0_f32, Color32::from_rgb(37, 99, 235)));
            if self.virtual_map.selected == Some(*i) {
                painter.text(
                    p + egui::vec2(9., -9.),
                    egui::Align2::LEFT_BOTTOM,
                    "Referentie",
                    egui::FontId::proportional(12.),
                    INK,
                );
            }
        }
        if let Some(pointer) = self.virtual_map.pointer {
            match &self.virtual_map.mode {
                Some(Mode::Create {
                    start: Some(start), ..
                }) => {
                    let a = view.project(*start);
                    let b = view.project(pointer);
                    painter.line_segment([a, b], Stroke::new(2.0_f32, ORANGE));
                    painter.circle_filled(a, 5., ORANGE);
                    painter.text(
                        b + egui::vec2(10., -10.),
                        egui::Align2::LEFT_BOTTOM,
                        format!(
                            "{:.2} m",
                            (pointer[0] - start[0]).hypot(pointer[1] - start[1])
                        ),
                        egui::FontId::proportional(13.),
                        INK,
                    );
                }
                Some(Mode::Move { target, anchor }) => {
                    let delta = [pointer[0] - anchor[0], pointer[1] - anchor[1]];
                    let points = self.virtual_trenches[target.index]
                        .measured_boundary()
                        .into_iter()
                        .map(|(x, y)| view.project([x + delta[0], y + delta[1]]))
                        .collect();
                    painter.add(egui::Shape::line(points, Stroke::new(2.0_f32, ORANGE)));
                }
                Some(Mode::MovePoint{target,point,..})=>{
                    let t=&self.virtual_trenches[target.index];let p=if matches!(point,map::MapPoint::Object(_)){let(x,y,_,_)=t.project(Vertex{x:pointer[0],y:pointer[1],z:None});[x,y]}else{pointer};
                    painter.circle_stroke(view.project(p),7.,Stroke::new(2.0_f32,ORANGE));
                }
                Some(Mode::AddPoint(target))|Some(Mode::Dekband{target,..})=>{
                    let t=&self.virtual_trenches[target.index];let(x,y,along,_)=t.project(Vertex{x:pointer[0],y:pointer[1],z:None});let pos=view.project([x,y]);
                    painter.circle_stroke(pos,7.,Stroke::new(2.0_f32,ORANGE));painter.text(pos+egui::vec2(9.,-9.),egui::Align2::LEFT_BOTTOM,format!("{along:.2} m"),egui::FontId::proportional(12.),INK);
                    if let Some(Mode::Dekband{start:Some(a),..})=&self.virtual_map.mode{let(x,y,_,_)=t.project(Vertex{x:a[0],y:a[1],z:None});painter.line_segment([view.project([x,y]),pos],Stroke::new(4_f32,ORANGE));}
                }
                Some(Mode::Copy{target,start:Some(start),..})=>{if let Ok(end)=map::copy_target_end(&self.virtual_trenches[target.index],*start,pointer){painter.line_segment([view.project(*start),view.project(end)],Stroke::new(2.0_f32,ORANGE));}}
                _ => {}
            }
        }
    }
}
