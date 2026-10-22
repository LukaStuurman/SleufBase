//! Address/RD/GPS lookup and temporary navigation, separate from project data.
use super::*;
use sleufbase_rust::location_search::{
    self,
    worker::{Ready, Worker},
    Location,
};

#[derive(Default)]
pub(super) struct Search {
    pub open: bool,
    query: String,
    results: Vec<Location>,
    selected: usize,
    result_focus: bool,
    result_ids: Vec<egui::Id>,
    deadline: Option<Instant>,
    worker: Option<Worker>,
    ticket: Option<u64>,
    error: Option<String>,
    focus_entry: bool,
    pub needs_aerial: bool,
}
impl Search {
    pub fn reset(&mut self) {
        if let Some(worker) = &self.worker {
            worker.cancel();
        }
        let worker = self.worker.take();
        *self = Self {
            worker,
            ..Default::default()
        };
    }
    fn cancel(&mut self) {
        if let Some(worker) = &self.worker {
            worker.cancel();
        }
        self.ticket = None;
        self.deadline = None;
        self.result_ids.clear();
    }
    fn accept(&mut self, ready: Ready) {
        if self.ticket != Some(ready.ticket) || !self.open {
            return;
        }
        self.ticket = None;
        match ready.result {
            Ok(mut values) => {
                values.truncate(6);
                self.results = values;
                self.selected = 0;
                self.result_focus = false;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
    fn start(&mut self) {
        self.cancel();
        self.error = None;
        self.results.clear();
        self.selected = 0;
        self.result_focus = false;
        if let Some(location) = location_search::parse_coordinate(&self.query) {
            self.results.push(location);
            return;
        }
        if self.query.trim().chars().count() < 3 {
            return;
        }
        let result = (|| {
            if self.worker.is_none() {
                self.worker = Some(Worker::new()?);
            }
            self.worker.as_ref().unwrap().search(self.query.clone())
        })();
        match result {
            Ok(ticket) => self.ticket = Some(ticket),
            Err(error) => self.error = Some(error),
        }
    }
}
#[derive(Clone, Copy)]
pub(super) struct Focus {
    pub center: [f64; 2],
    pub meters_per_pixel: f64,
}
#[derive(Default)]
pub(super) struct Navigation {
    pub focus: Option<Focus>,
    pub viewport: Option<Bounds>,
    pub marker: Option<([f64; 2], Instant)>,
}
impl Navigation {
    fn mark(&mut self, location: &Location, jump: bool) -> Result<(), String> {
        if !location.x.is_finite() || !location.y.is_finite() {
            return Err("Ongeldige locatie".into());
        }
        let center = [location.x, location.y];
        if jump {
            self.focus = Some(Focus {
                center,
                meters_per_pixel: location.meters_per_pixel(),
            });
            self.viewport = None;
        }
        self.marker = Some((center, Instant::now()));
        Ok(())
    }
    pub fn bounds(&self) -> Option<Bounds> {
        self.focus.map(|focus| {
            self.viewport.unwrap_or(Bounds {
                min_x: focus.center[0] - 600. * focus.meters_per_pixel,
                max_x: focus.center[0] + 600. * focus.meters_per_pixel,
                min_y: focus.center[1] - 450. * focus.meters_per_pixel,
                max_y: focus.center[1] + 450. * focus.meters_per_pixel,
            })
        })
    }
}
impl SleufBaseApp {
    pub(super) fn open_location(&mut self) {
        self.location.open = true;
        self.location.focus_entry = true;
    }
    pub(super) fn poll_location(&mut self, ctx: &egui::Context) {
        if let Some(ready) = self.location.worker.as_ref().and_then(Worker::poll) {
            self.location.accept(ready);
        }
        if self.location.open
            && self
                .location
                .deadline
                .is_some_and(|at| Instant::now() >= at)
        {
            self.location.start();
        }
        if self.location.ticket.is_some() || self.location.deadline.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if self.location.needs_aerial && !self.pdok_busy && self.navigation.viewport.is_some() {
            self.location.needs_aerial = false;
            self.start_pdok();
        }
    }
    fn choose_location(&mut self, location: Location, jump: bool) -> Result<(), String> {
        if self
            .geotiffs
            .iter()
            .any(|r| r.epsg.is_some_and(|e| e != 28992))
        {
            return Err("Locatiezoeken vereist RD (EPSG:28992) voor geladen kaartlagen".into());
        }
        self.navigation.mark(&location, jump)?;
        if jump {
            self.zoom = 1.;
            self.pan = egui::Vec2::ZERO;
            self.osm_visible = false;
            self.location.needs_aerial = true;
            self.location.open = false;
            self.location.cancel();
            self.message = format!("Gesprongen naar: {}", location.label);
        } else {
            self.message = format!("Locatie gemarkeerd: {}", location.label);
        }
        Ok(())
    }
    pub(super) fn show_location(&mut self, ctx: &egui::Context) {
        if !self.location.open {
            return;
        }
        let mut open = true;
        let mut close_key = false;
        let mut choice = None;
        let mut submit = false;
        egui::Window::new("Locatie zoeken").id(egui::Id::new("location-search"))
            .open(&mut open).default_width(440.).show(ctx,|ui|{
            ui.label("Adres, plaats, RD-coördinaten of GPS-locatie");
            let entry_id=egui::Id::new("location-query");
            let keyboard_active=ui.memory(|m|m.focused()).is_some_and(|id|id==entry_id||self.location.result_ids.contains(&id))||self.location.focus_entry;
            let down=keyboard_active&&ui.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::ArrowDown));
            let up=keyboard_active&&ui.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::ArrowUp));
            submit|=keyboard_active&&ui.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Enter));
            close_key|=keyboard_active&&ui.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Escape));
            let response=ui.add(egui::TextEdit::singleline(&mut self.location.query)
                .id(entry_id).char_limit(1024).desired_width(f32::INFINITY).hint_text("Bijv. Utrecht of 155000, 463000"));
            if self.location.focus_entry {response.request_focus();self.location.focus_entry=false;}
            if response.has_focus(){ui.memory_mut(|m|m.set_focus_lock_filter(entry_id,egui::EventFilter{horizontal_arrows:true,vertical_arrows:true,escape:true,..Default::default()}));}
            if response.changed(){self.location.cancel();self.location.results.clear();self.location.result_focus=false;self.location.error=None;self.location.deadline=Some(Instant::now()+Duration::from_millis(250));}
            if down&&!self.location.results.is_empty(){
                self.location.selected=if self.location.result_focus{(self.location.selected+1).min(self.location.results.len()-1)}else{0};self.location.result_focus=true;
            }
            if up&&self.location.result_focus{
                if self.location.selected==0{self.location.result_focus=false;}
                else{self.location.selected-=1;}
            }
            if self.location.ticket.is_some(){ui.horizontal(|ui|{ui.spinner();ui.label("Locaties zoeken…");});}
            egui::ScrollArea::vertical().max_height(190.).show(ui,|ui|{
                self.location.result_ids.clear();
                for (i,result) in self.location.results.iter().enumerate(){
                    let response=ui.selectable_label(self.location.selected==i,format!("{} ({})",result.label,result.location_type));
                    self.location.result_ids.push(response.id);
                    if response.has_focus(){ui.memory_mut(|m|m.set_focus_lock_filter(response.id,egui::EventFilter{vertical_arrows:true,escape:true,..Default::default()}));}
                    if response.clicked(){self.location.selected=i;self.location.result_focus=true;ui.memory_mut(|m|m.request_focus(entry_id));}
                    if response.double_clicked(){choice=Some((result.clone(),true));}
                }
            });
            ui.horizontal(|ui|{
                if ui.button("Zoeken").clicked(){self.location.start();}
                if ui.add_enabled(!self.location.results.is_empty(),egui::Button::new("Ga naar locatie")).clicked(){submit=true;}
                if ui.add_enabled(!self.location.results.is_empty(),egui::Button::new("Alleen markeren")).clicked(){choice=self.location.results.get(self.location.selected).cloned().map(|r|(r,false));}
            });
            if let Some(error)=&self.location.error {ui.colored_label(DARK,error);}
            ui.small("Zoeksuggesties: PDOK Locatieserver. GPS kan in beide volgordes voor Nederland.");
        });
        if submit {
            if let Some(location) = location_search::parse_coordinate(&self.location.query) {
                choice = Some((location, true));
            } else if let Some(location) = self.location.results.get(self.location.selected) {
                choice = Some((location.clone(), true));
            } else {
                self.location.start();
            }
        }
        if let Some((location, jump)) = choice {
            if let Err(error) = self.choose_location(location, jump) {
                self.location.error = Some(error);
            }
        }
        if !open || close_key {
            self.location.open = false;
            self.location.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn location() -> Location {
        Location {
            label: "Testlocatie".into(),
            x: 155000.,
            y: 463000.,
            location_type: "rd".into(),
        }
    }
    #[test]
    fn stale_or_closed_search_results_never_replace_current_suggestions() {
        let mut search = Search {
            open: true,
            ticket: Some(2),
            ..Default::default()
        };
        search.accept(Ready {
            ticket: 1,
            result: Ok(vec![location()]),
        });
        assert!(search.results.is_empty());
        search.accept(Ready {
            ticket: 2,
            result: Ok(vec![location()]),
        });
        assert_eq!(search.results.len(), 1);
        search.open = false;
        search.ticket = Some(3);
        search.accept(Ready {
            ticket: 3,
            result: Err("stale".into()),
        });
        assert!(search.error.is_none());
        search.reset();
        assert!(search.results.is_empty() && search.ticket.is_none() && !search.open);
    }
    #[test]
    fn a_location_on_an_empty_map_has_the_original_scale_pulse_and_panned_provider_bounds() {
        let ctx = egui::Context::default();
        let mut navigation = Navigation::default();
        navigation.mark(&location(), true).unwrap();
        let mut zoom = 1.;
        let mut pan = egui::vec2(30., -20.);
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(800., 600.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    canvas(
                        ui,
                        &[],
                        &[],
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
                        &mut zoom,
                        &mut pan,
                        RULES,
                        Some(&mut navigation),
                    );
                });
            },
        );
        assert!(output
            .shapes
            .iter()
            .any(|s| matches!(&s.shape,egui::Shape::Circle(c) if c.fill==ORANGE&&c.radius==4.)));
        let b = navigation.bounds().unwrap();
        assert!(((b.min_x + b.max_x) / 2. - (155000. - 30. * 0.2)).abs() < 1e-8);
        assert!(((b.min_y + b.max_y) / 2. - (463000. - 20. * 0.2)).abs() < 1e-8);
        assert!((b.max_x - b.min_x) < 160. && b.max_x > b.min_x);
    }
    #[test]
    fn navigation_is_not_a_project_edit_and_project_switch_cancels_and_clears_it() {
        let mut app = SleufBaseApp::default();
        let ctx = egui::Context::default();
        app.choose_location(location(), true).unwrap();
        assert!(!app.dirty && app.location.needs_aerial && app.navigation.focus.is_some());
        assert_eq!(app.snapshot().comments.len(), 0);
        app.apply_project(NativeProject::default(), None, false, Vec::new(), &ctx);
        assert!(
            app.navigation.focus.is_none()
                && app.navigation.marker.is_none()
                && !app.location.needs_aerial
        );
        app.choose_location(location(), false).unwrap();
        assert!(app.navigation.focus.is_none() && app.navigation.marker.is_some());
    }
    #[test]
    fn keyboard_enters_the_first_suggestion_then_moves_and_selects_without_a_network_call() {
        let ctx = egui::Context::default();
        let mut app = SleufBaseApp {
            location: Search {
                open: true,
                query: "testplaats".into(),
                results: vec![
                    location(),
                    Location {
                        label: "Tweede".into(),
                        x: 156000.,
                        ..location()
                    },
                ],
                focus_entry: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let frame = |key: Option<egui::Key>, app: &mut SleufBaseApp| {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(900., 650.),
                )),
                ..Default::default()
            };
            if let Some(key) = key {
                input.events.push(egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let _ = ctx.run(input, |ctx| app.show_location(ctx));
        };
        frame(None, &mut app);
        frame(Some(egui::Key::ArrowDown), &mut app);
        assert_eq!(app.location.selected, 0);
        frame(Some(egui::Key::ArrowDown), &mut app);
        assert_eq!(app.location.selected, 1);
        frame(Some(egui::Key::Enter), &mut app);
        assert!(!app.location.open && app.location.worker.is_none());
        assert_eq!(app.navigation.focus.unwrap().center, [156000., 463000.]);
    }
}
