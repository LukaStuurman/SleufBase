//! Standalone map export uses the same source-owning worker and project guard.
use super::*;
use sleufbase_rust::export_worker::{Kind, Request};
impl SleufBaseApp {
    pub(super) fn open_map_export(&mut self) {
        if self.export_intent.is_some()
            || self.template_job.is_some()
            || self.file_intent.is_some()
            || self.pending_project_action.is_some()
        {
            self.message = "Rond de lopende taak eerst af".into();
            return;
        }
        if self.geotiffs.is_empty() {
            self.message = "Laad eerst minimaal een GeoTIFF-bestand.".into();
            return;
        }
        self.map_export_open = true;
    }
    pub(super) fn start_maps_export(&mut self, directory: PathBuf) -> bool {
        if self.geotiffs.is_empty() {
            self.message = "Laad eerst minimaal een GeoTIFF-bestand.".into();
            return false;
        }
        let settings = match self.project_export_settings(){Ok(settings)=>settings.map_export,Err(error)=>{self.message=error;return false;}};
        if let Err(error) = settings.validate() {
            self.message = error;
            return false;
        }
        let request = Request {
            kind: Kind::Maps,
            path: directory,
            selection: sleufbase_rust::object_selection::Selection::all(0, 0),
            rules: Vec::new(),
            extra_layers: false,
            options: Default::default(),
            template: None,
            discipline_excel: false,
            maps: Some(settings),point_target:None,point_network:false,
        };
        let started = self.start_export(request, false);
        if started {
            self.map_export_open = false;
        }
        started
    }
    pub(super) fn show_map_export(&mut self, ctx: &egui::Context) {
        if !self.map_export_open {
            return;
        }
        let mut open = true;
        let mut export = false;
        let mut save = false;
        let names = sleufbase_rust::map_batch::filenames(self.geotiffs.iter().map(|r| &r.path));
        egui::Window::new("Export kaarten").id(egui::Id::new("standalone-map-export"))
            .open(&mut open).default_width(520.).show(ctx,|ui|{
            ui.label(format!("{} GeoTIFF(s) als afzonderlijke A4-kaarten",names.len()));
            let settings=match &mut self.project_settings{Some(settings)=>&mut settings.map_export,None=>&mut self.migrated_settings.map_export};
            let previous=settings.clone();
            egui::ComboBox::from_label("Achtergrond").selected_text(match settings.background.as_str(){
                "pdok"=>"PDOK luchtfoto","osm"=>"OpenStreetMap","white"=>"Wit (zonder internet)",
                "cadastral"=>"Kadaster",_=>"Onbekend"
            }).show_ui(ui,|ui|{
                ui.selectable_value(&mut settings.background,"pdok".into(),"PDOK luchtfoto");
                ui.selectable_value(&mut settings.background,"osm".into(),"OpenStreetMap");
                ui.selectable_value(&mut settings.background,"cadastral".into(),"Kadaster");
                ui.selectable_value(&mut settings.background,"white".into(),"Wit (zonder internet)");
            });
            ui.checkbox(&mut settings.include_cad,"Zichtbare CAD-lagen meenemen");
            ui.horizontal(|ui|{ui.label("Resolutie (DPI)");ui.add(egui::DragValue::new(&mut settings.dpi).range(30..=600));});
            ui.horizontal(|ui|{ui.label("Schaal 1:");ui.add(egui::DragValue::new(&mut settings.scale).range(1.0..=100_000.0));});
            egui::ScrollArea::vertical().max_height(160.).show(ui,|ui|{for name in &names{ui.label(name);}});
            ui.small("Bestaande PNGs met deze namen worden vervangen nadat alle kaarten zijn voorbereid. Dubbele bronnamen krijgen een volgnummer.");
            ui.horizontal(|ui|{save=ui.button("Instellingen onthouden").clicked();export=ui.button("Kies exportmap…").clicked();});
            if *settings!=previous&&self.project_settings.is_some(){self.dirty=true;}
        });
        self.map_export_open = open;
        if save {
            if self.project_settings.is_some(){self.dirty=true;self.message="Kaartinstellingen in het project gewijzigd. Sla het project op om ze te bewaren.".into();}
            else{self.message = match settings_compat::save_rust_settings(
                &project::app_data_directory().join("settings.json"),
                &self.migrated_settings,
            ) {
                Ok(()) => "Kaartinstellingen opgeslagen".into(),
                Err(e) => format!("Kaartinstellingen niet opgeslagen: {e}"),
            };}
        }
        if export {
            if let Some(directory) = rfd::FileDialog::new()
                .set_title("Kies exportmap voor kaarten")
                .pick_folder()
            {
                self.start_maps_export(directory);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::mpsc,
        time::{Duration, Instant},
    };
    fn app() -> SleufBaseApp {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/map-batch");
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/map-batch/reference.json"))
                .unwrap();
        let case = &data["cases"][0];
        let mut app = SleufBaseApp {
            geotiffs: vec![GeoTiffInfo {
                path: root.join(case["path"].as_str().unwrap()),
                width: 53,
                height: 37,
                transform: serde_json::from_value(case["transform"].clone()).unwrap(),
                bounds: serde_json::from_value(case["bounds"].clone()).unwrap(),
                epsg: Some(28992),
                kickthemap_job_id: None,
                kickthemap_job_title: String::new(),
                kickthemap_job_sidecar: None,metadata:Default::default(),
            }],
            ..Default::default()
        };
        app.migrated_settings.map_export = sleufbase_rust::map_batch::Settings {
            background: "white".into(),
            dpi: 96,
            ..Default::default()
        };
        app
    }
    #[test]
    fn actual_gui_map_worker_restores_same_sources_and_does_not_dirty_the_project() {
        let ctx = egui::Context::default();
        let dir = tempfile::tempdir().unwrap();
        let mut app = app();
        let pointer = app.geotiffs.as_ptr();
        let before = fs::read(&app.geotiffs[0].path).unwrap();
        app.open_map_export();
        assert!(app.map_export_open);
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.show_map_export(ctx));
        assert!(app.start_maps_export(dir.path().to_owned()));
        assert!(!app.map_export_open);
        assert!(app.geotiffs.is_empty());
        let until = Instant::now() + Duration::from_secs(20);
        while app.export_intent.is_some() {
            app.poll_export_jobs(&ctx);
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(app.geotiffs.as_ptr(), pointer);
        assert!(!app.dirty);
        assert_eq!(fs::read(&app.geotiffs[0].path).unwrap(), before);
        assert!(dir.path().join("input-0_kaart.png").is_file());
        assert!(app.message.contains("1 kaart(en)"));
    }
    #[test]
    fn actual_kadaster_choice_reaches_the_existing_source_owning_export_worker() {
        use sleufbase_rust::export_worker::{Exported,Exporter};
        let ctx=egui::Context::default();let mut app=app();app.open_map_export();
        let frame=|app:&mut SleufBaseApp,events|ctx.run(egui::RawInput{
            screen_rect:Some(egui::Rect::from_min_size(Pos2::ZERO,egui::vec2(1024.,768.))),events,..Default::default()
        },|ctx|app.show_map_export(ctx));
        let _=frame(&mut app,Vec::new());let output=frame(&mut app,Vec::new());
        let text_pos=|output:&egui::FullOutput,text:&str|output.shapes.iter()
            .find(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==text))
            .unwrap_or_else(||panic!("Missing map choice {text}")).shape.visual_bounding_rect().center();
        let click=|app:&mut SleufBaseApp,pos|{
            for pressed in [true,false]{let _=frame(app,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
        };
        click(&mut app,text_pos(&output,"Wit (zonder internet)"));let popup=frame(&mut app,Vec::new());
        assert!(!popup.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text().contains("Cyclomedia"))));
        click(&mut app,text_pos(&popup,"Kadaster"));assert_eq!(app.migrated_settings.map_export.background,"cadastral");
        let (sent,received)=mpsc::channel();
        app.exporter=Some(Exporter::with_processor(move |_,request,_|{
            sent.send(request.maps.as_ref().unwrap().background.clone()).unwrap();
            Ok(Exported{point_updates:Vec::new(),point_candidates:None,skipped:Vec::new(),companion:None,warnings:Vec::new()})
        }).unwrap());
        let dir=tempfile::tempdir().unwrap();let sources=app.geotiffs.as_ptr();
        assert!(app.start_maps_export(dir.path().to_owned()));
        assert_eq!(received.recv_timeout(Duration::from_secs(5)).unwrap(),"cadastral");
        let until=Instant::now()+Duration::from_secs(5);
        while app.export_intent.is_some(){app.poll_export_jobs(&ctx);assert!(Instant::now()<until);std::thread::sleep(Duration::from_millis(2));}
        assert_eq!(app.geotiffs.as_ptr(),sources);
    }
    #[test]
    fn project_switch_cancels_map_work_before_replacing_source_ownership() {
        use sleufbase_rust::export_worker::{Exported, Exporter};
        let (started, rx) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let mut app = app();
        app.exporter = Some(
            Exporter::with_processor(move |_, request, control| {
                assert_eq!(request.kind, Kind::Maps);
                started.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
                assert!(control.is_cancelled());
                Ok(Exported {
                    point_updates:Vec::new(),
                    point_candidates:None,
                    skipped: Vec::new(),
                    companion: None,
                    warnings: Vec::new(),
                })
            })
            .unwrap(),
        );
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let generation = app.project_generation;
        assert!(app.start_maps_export(dir.path().to_owned()));
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        app.request_project_action(ProjectAction::New, &ctx);
        assert_eq!(app.project_generation, generation);
        assert!(app.geotiffs.is_empty());
        release.send(()).unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while app.export_intent.is_some() {
            app.poll_export_jobs(&ctx);
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(app.project_generation > generation);
        assert!(app.geotiffs.is_empty());
        assert!(!app.map_export_open);
    }
}
