//! GUI adapters for durable project views and the bounded Python export task.
use super::*;
use sleufbase_rust::project_view::{Camera,ProjectView};
/// Capture actual world coordinates at screen centre, including pan and zoom.
pub(super) fn capture_camera(view:&raster_point_ui::View)->Camera{
    Camera{center:view.world(view.rect.center()),meters_per_pixel:1./view.scale}
}
/// Apply once after loading; later frames retain normal interactive navigation.
pub(super) fn restore_camera(view:&ProjectView,navigation:&mut location_ui::Navigation,zoom:&mut f64,pan:&mut egui::Vec2){
    navigation.focus=view.camera.as_ref().map(|c|location_ui::Focus{center:c.center,meters_per_pixel:c.meters_per_pixel});
    navigation.viewport=None;*zoom=1.;*pan=egui::Vec2::ZERO;
}
pub(super) fn export_progress(ctx:&egui::Context,worker:&sleufbase_rust::project_exchange_worker::Worker){
    if !worker.is_busy(){return;}
    egui::Window::new("Project voor Python exporteren").collapsible(false).resizable(false).show(ctx,|ui|{
        ui.spinner();ui.label("Projectgegevens voorbereiden en opslaan…");
        if ui.button("Annuleren").clicked(){worker.cancel();}
    });ctx.request_repaint_after(std::time::Duration::from_millis(40));
}

impl SleufBaseApp {
    pub(super) fn request_selected_background(&mut self){
        use sleufbase_rust::project_view::BackgroundProvider as B;
        self.background_restore_pending=false;
        if !self.project_view.use_background{
            self.pdok_rx=None;self.pdok_busy=false;self.osm_rx=None;self.osm_busy=false;return;
        }
        match self.project_view.provider(){
            B::PdokAerial|B::Cadastral=>{
                let source=if self.project_view.provider()==B::Cadastral{pdok_wms::Source::Cadastral}else{pdok_wms::Source::Aerial};
                self.osm_rx=None;self.osm_busy=false;self.osm_visible=false;
                if self.pdok_source!=source{self.pdok_rx=None;self.pdok_busy=false;self.pdok_texture=None;self.pdok_bounds=None;}
                self.pdok_source=source;
                #[cfg(not(test))]
                self.start_pdok_source(source);
            }
            B::OpenStreetMap=>{
                self.pdok_rx=None;self.pdok_busy=false;self.pdok_texture=None;self.pdok_bounds=None;self.osm_visible=true;
                #[cfg(not(test))]
                self.start_osm();
            }
            B::Cyclomedia=>{self.project_view.normalize_removed_provider();self.request_selected_background();}
            B::Unknown=>{
                self.pdok_rx=None;self.pdok_busy=false;self.osm_rx=None;self.osm_busy=false;
                self.pdok_texture=None;self.pdok_bounds=None;self.osm_texture=None;self.osm_bounds=None;
                self.message=format!("Achtergrondbron '{}' wordt niet ondersteund; de voorkeur blijft in het project bewaard",self.project_view.background_source);
            }
        }
    }
    /// Resolve persistent identities without a project snapshot or vector copy.
    pub(super) fn apply_view_selection(&mut self){
        use sleufbase_rust::project_view::TiffSelection as S;
        self.virtual_map.selected=None;self.raster_points.selected=None;
        match &self.project_view.selected_tiff{
            Some(S::Virtual{job_id,label})=>{
                let mut hits=self.virtual_trenches.iter().enumerate().filter(|(_,t)|t.job_id==*job_id&&t.label==*label).map(|(i,_)|i);
                let first=hits.next();if hits.next().is_none(){self.virtual_map.selected=first;}
            }
            Some(S::Raster{path})=>{if self.geotiffs.iter().filter(|r|r.path==*path).count()==1{self.raster_points.selected=Some(path.clone());}}
            None=>{}
        }
        if self.dxf_selection.as_ref().is_some_and(|selected|self.overlays.get(selected.overlay).is_none_or(|overlay|Some(&overlay.path)!=self.project_view.selected_dxf.as_ref())){self.dxf_selection=None;}
    }
    pub(super) fn show_project_view_controls(&mut self,ui:&mut egui::Ui){
        use sleufbase_rust::project_view::{BackgroundProvider as B,TiffSelection as S};
        let previous=self.project_view.clone();
        ui.horizontal_wrapped(|ui|{
            ui.checkbox(&mut self.project_view.use_background,"Achtergrond tonen");
            egui::ComboBox::from_id_salt("project-background-source").selected_text(self.project_view.background_source.clone()).show_ui(ui,|ui|{
                for source in [B::PdokAerial,B::OpenStreetMap,B::Cadastral]{ui.selectable_value(&mut self.project_view.background_source,source.label().into(),source.label());}
                if self.project_view.provider()==B::Unknown{ui.label(format!("Niet ondersteund: {}",self.project_view.background_source));}
            });
            ui.add(egui::Slider::new(&mut self.project_view.opacity_percent,0. ..=100.).text("TIFF-dekking (%)"));
            if ui.add_enabled(self.project_view.use_background&&!self.pdok_busy&&!self.osm_busy,egui::Button::new("Achtergrond vernieuwen")).clicked(){self.request_selected_background();}
        });
        ui.horizontal_wrapped(|ui|{
            let label=match &self.project_view.selected_tiff{Some(S::Virtual{label,..})=>label.clone(),Some(S::Raster{path})=>path.file_name().unwrap_or_default().to_string_lossy().into_owned(),None=>"Geen geselecteerde proefsleuf".into()};
            egui::ComboBox::from_id_salt("project-tiff-selection").selected_text(label).show_ui(ui,|ui|{
                ui.selectable_value(&mut self.project_view.selected_tiff,None,"Geen geselecteerde proefsleuf");
                for trench in &self.virtual_trenches{ui.selectable_value(&mut self.project_view.selected_tiff,Some(S::Virtual{job_id:trench.job_id,label:trench.label.clone()}),format!("Sleuf: {}",trench.label));}
                for raster in &self.geotiffs{ui.selectable_value(&mut self.project_view.selected_tiff,Some(S::Raster{path:raster.path.clone()}),format!("TIFF: {}",raster.path.file_name().unwrap_or_default().to_string_lossy()));}
            });
            let label=self.project_view.selected_dxf.as_ref().map(|p|p.file_name().unwrap_or_default().to_string_lossy().into_owned()).unwrap_or_else(||"Geen geselecteerde DXF-laag".into());
            egui::ComboBox::from_id_salt("project-dxf-selection").selected_text(label).show_ui(ui,|ui|{
                ui.selectable_value(&mut self.project_view.selected_dxf,None,"Geen geselecteerde DXF-laag");
                for overlay in &self.overlays{ui.selectable_value(&mut self.project_view.selected_dxf,Some(overlay.path.clone()),overlay.path.file_name().unwrap_or_default().to_string_lossy());}
            });
        });
        if self.project_view!=previous{
            self.dirty=true;
            if previous.selected_tiff!=self.project_view.selected_tiff||previous.selected_dxf!=self.project_view.selected_dxf{self.apply_view_selection();}
            if previous.use_background!=self.project_view.use_background||previous.background_source!=self.project_view.background_source{self.request_selected_background();}
        }
    }
    pub(super) fn project_exchange_busy(&self)->bool{self.project_exchange.as_ref().is_some_and(|w|w.is_busy())}
    pub(super) fn defer_for_project_exchange(&mut self,action:&ProjectAction)->bool{
        if let Some(worker)=self.project_exchange.as_ref().filter(|w|w.is_busy()) {
            worker.cancel();self.exchange_deferred=Some(action.clone());return true;
        }false
    }
    pub(super) fn choose_python_project_export(&mut self){
        if self.project_sources_owned()||self.export_intent.is_some()||self.template_job.is_some()||self.file_intent.is_some()||self.pending_project_action.is_some()||self.trench_editor.is_some(){
            self.message="Rond de lopende taak of het sleufconcept eerst af".into();return;
        }
        if let Some(path)=rfd::FileDialog::new().set_title("Nieuw project voor Python exporteren").add_filter("SleufBase Python-project",&["klicproject"]).set_file_name("SleufBase-export.klicproject").save_file(){self.start_python_project_export(path);}
    }
    /// Move every large allocation to the worker. Texture caches, identity,
    /// dirty state and generation remain on the UI until ownership returns.
    pub(super) fn take_project_sources(&mut self)->NativeProject{
        NativeProject{
            name:self.project_name.clone(),view:Some(self.project_view.clone()),project_settings:self.project_settings.take(),
            datasets:std::mem::take(&mut self.datasets),rasters:std::mem::take(&mut self.geotiffs),overlays:std::mem::take(&mut self.overlays),
            bgt_layers:std::mem::take(&mut self.bgt_layers),cadastral:self.cadastral.take(),virtual_trenches:std::mem::take(&mut self.virtual_trenches),comments:std::mem::take(&mut self.comments),
            template_export:self.template_state.take(),template_preferences:self.template_preferences.take(),legacy_archive:self.legacy_archive.take(),
            ..Default::default()
        }
    }
    pub(super) fn restore_project_sources(&mut self,project:NativeProject){
        self.datasets=project.datasets;self.geotiffs=project.rasters;self.overlays=project.overlays;self.bgt_layers=project.bgt_layers;
        self.cadastral=project.cadastral;self.virtual_trenches=project.virtual_trenches;self.comments=project.comments;
        self.template_state=project.template_export;self.template_preferences=project.template_preferences;self.legacy_archive=project.legacy_archive;
        self.project_settings=project.project_settings;if let Some(view)=project.view{self.project_view=view;}
    }
    pub(super) fn start_python_project_export(&mut self,path:PathBuf){
        if self.project_sources_owned()||self.export_intent.is_some()||self.template_job.is_some()||self.file_intent.is_some()||self.pending_project_action.is_some()||self.trench_editor.is_some(){self.message="Rond de lopende taak of het sleufconcept eerst af".into();return;}
        if self.project_exchange.as_ref().is_none_or(|w|w.is_closed()) {
            match sleufbase_rust::project_exchange_worker::Worker::new(){Ok(worker)=>self.project_exchange=Some(worker),Err(error)=>{self.message=error;return;}}
        }
        let project=self.take_project_sources();
        match self.project_exchange.as_mut().unwrap().start(project,path){
            Ok(())=>{self.message="Project voor Python exporteren…".into();self.exchange_deferred=None;}
            Err(failure)=>{self.message=failure.message;self.restore_project_sources(failure.project);}
        }
    }
    pub(super) fn poll_project_exchange(&mut self,ctx:&egui::Context){
        let ready=self.project_exchange.as_mut().and_then(|w|w.poll());
        if let Some(ready)=ready {
            self.restore_project_sources(ready.project);
            self.message=match ready.result{Ok(())=>format!("Nieuw Python-project opgeslagen: {}",ready.path.display()),Err(error)=>format!("Python-export: {error}")};
            if let Some(action)=self.exchange_deferred.take(){self.request_project_action(action,ctx);}
        }
    }
    pub(super) fn update_view_selection(&mut self,old_virtual:Option<usize>,old_raster:Option<&std::path::Path>){
        use sleufbase_rust::project_view::TiffSelection;
        let previous=self.project_view.selected_tiff.clone();
        if self.virtual_map.selected!=old_virtual {
            self.project_view.selected_tiff=self.virtual_map.selected.and_then(|i|self.virtual_trenches.get(i)).map(|t|TiffSelection::Virtual{job_id:t.job_id,label:t.label.clone()});
        }else if self.raster_points.selected.as_deref()!=old_raster {
            self.project_view.selected_tiff=self.raster_points.selected.as_ref().map(|path|TiffSelection::Raster{path:path.clone()});
        }
        self.dirty|=previous!=self.project_view.selected_tiff;
    }
    pub(super) fn project_export_settings(&self)->Result<MigratedSettings,String>{
        if let Some(settings)=&self.project_settings{settings.validate()?;return Ok(settings.clone());}
        if let Some(value)=self.legacy_archive.as_ref().and_then(|a|a.original.get("template_export_settings")) {
            return settings_compat::with_project_overrides(&self.migrated_settings,value);
        }
        self.migrated_settings.validate()?;Ok(self.migrated_settings.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_export_restores_allocations_identity_camera_and_unsaved_state(){
        let mut app=SleufBaseApp::default();app.project_name="Bewaard".into();app.project_path=Some("original.json".into());app.dirty=true;
        app.comments.push(Comment{x:1.,y:2.,text:"Bron".into()});let pointer=app.comments.as_ptr();let generation=app.project_generation;
        app.project_view.camera=Some(Camera{center:[123.,456.],meters_per_pixel:0.1});
        let dir=tempfile::tempdir().unwrap();let target=dir.path().join("exists.klicproject");std::fs::write(&target,b"previous").unwrap();
        app.start_python_project_export(target.clone());assert!(app.project_exchange_busy());assert!(app.comments.is_empty());
        let ctx=egui::Context::default();let deadline=Instant::now()+Duration::from_secs(5);
        while app.project_exchange_busy(){assert!(Instant::now()<deadline);app.poll_project_exchange(&ctx);std::thread::sleep(Duration::from_millis(2));}
        assert_eq!(app.comments.as_ptr(),pointer);assert_eq!(app.project_generation,generation);assert!(app.dirty);assert_eq!(app.project_path,Some("original.json".into()));
        assert_eq!(app.project_view.camera.unwrap().center,[123.,456.]);assert_eq!(std::fs::read(target).unwrap(),b"previous");
    }
    #[test]
    fn camera_capture_and_restore_preserve_the_visible_world_center(){
        let view=ProjectView{camera:Some(Camera{center:[155012.5,463007.25],meters_per_pixel:0.08}),..Default::default()};
        let mut navigation=location_ui::Navigation::default();let mut zoom=9.;let mut pan=egui::vec2(7.,8.);
        restore_camera(&view,&mut navigation,&mut zoom,&mut pan);assert_eq!(zoom,1.);assert_eq!(pan,egui::Vec2::ZERO);let focus=navigation.focus.unwrap();assert_eq!(focus.center,[155012.5,463007.25]);assert_eq!(focus.meters_per_pixel,0.08);
    }
}

#[cfg(test)]
mod view_control_tests{
    use super::*;
    use sleufbase_rust::project_view::{BackgroundProvider as B,TiffSelection as S};
    #[test]
    fn scoped_preferences_reach_both_template_dialogs_and_keep_global_defaults(){
        let mut app=SleufBaseApp::default();app.migrated_settings.dxf_label_gap=9.;app.migrated_settings.export_preferences.template_include_cross_sections=false;
        let general=app.migrated_settings.clone();let mut scoped=general.clone();scoped.dxf_label_gap=2.;scoped.export_preferences.template_include_cross_sections=true;
        scoped.export_preferences.dxf_trench_export_mode="polygon".into();scoped.export_preferences.dxf_centerline_color="#123456".into();app.project_settings=Some(scoped);
        let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/gray16.tiff");app.geotiffs.push(sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap());
        app.virtual_trenches.push(VirtualTrench::new("PS1".into(),Vertex{x:155000.,y:463000.,z:None},Vertex{x:155010.,y:463000.,z:None},1.).unwrap());
        app.open_template_export();let state=&app.template_order_dialog.as_ref().unwrap().state;assert!(state.profiles);assert_eq!(state.label_gap,2.);assert_eq!(state.overview_settings.centerline_color,[0x12,0x34,0x56]);
        app.open_project_template_dialog();let opts=&app.project_template_dialog.as_ref().unwrap().options;assert!(opts.choices[0].profile);assert_eq!(opts.overview_settings.trench_mode,sleufbase_rust::cadastral_overview::TrenchMode::Polygon);
        app.save_dxf();assert_eq!(app.export_dialog.as_ref().unwrap().label_gap,2.);assert_eq!(app.migrated_settings,general);
        let restored=serde_json::from_value::<NativeProject>(serde_json::to_value(app.snapshot()).unwrap()).unwrap();assert_eq!(restored.project_settings.unwrap().dxf_label_gap,2.);
    }
    #[test]
    fn background_switches_preserve_saved_preferences_and_do_not_network_in_tests(){
        let mut app=SleufBaseApp::default();app.project_view.background_source=B::Cadastral.label().into();app.background_restore_pending=true;
        app.request_selected_background();assert_eq!(app.pdok_source,pdok_wms::Source::Cadastral);assert!(!app.background_restore_pending);assert!(!app.osm_visible);assert!(app.pdok_rx.is_none());
        app.project_view.background_source=B::OpenStreetMap.label().into();app.request_selected_background();assert!(app.osm_visible);assert!(app.osm_rx.is_none());
        app.project_view.use_background=false;app.request_selected_background();assert!(!app.pdok_busy);assert!(!app.osm_busy);assert!(!app.project_view.use_background);
        app.project_view.use_background=true;app.project_view.background_source="Future custom source".into();app.request_selected_background();assert!(app.message.contains("niet ondersteund"));assert_eq!(app.project_view.background_source,"Future custom source");
    }
    #[test]
    fn persistent_layer_identity_drives_real_selection_widgets_and_survives_reorder(){
        let mut app=SleufBaseApp::default();
        let start=Vertex{x:155000.,y:463000.,z:None};let end=Vertex{x:155004.,y:463000.,z:None};
        app.virtual_trenches=vec![VirtualTrench::new("PS1".into(),start,end,1.).unwrap(),VirtualTrench::new("PS2".into(),start,end,1.).unwrap()];
        app.project_view.selected_tiff=Some(S::Virtual{job_id:0,label:"PS2".into()});app.apply_view_selection();assert_eq!(app.virtual_map.selected,Some(1));
        app.virtual_trenches.swap(0,1);app.apply_view_selection();assert_eq!(app.virtual_map.selected,Some(0));
        let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/gray16.tiff");app.geotiffs.push(sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap());
        app.project_view.selected_tiff=Some(S::Raster{path:path.clone()});app.apply_view_selection();assert_eq!(app.raster_points.selected,Some(path));assert_eq!(app.virtual_map.selected,None);
        let ctx=egui::Context::default();let mut text=String::new();
        let output=ctx.run(egui::RawInput::default(),|ctx|{egui::CentralPanel::default().show(ctx,|ui|app.show_project_view_controls(ui));});
        for clipped in output.shapes{if let egui::Shape::Text(shape)=clipped.shape{text.push_str(&shape.galley.job.text);}}
        assert!(text.contains("gray16.tiff"));assert!(text.contains("TIFF-dekking"));
    }
    #[test]
    fn busy_python_export_defers_close_restores_data_and_cannot_repurpose_save(){
        let mut app=SleufBaseApp::default();app.project_path=Some("original.json".into());app.dirty=true;app.comments.push(Comment{x:1.,y:2.,text:"unsaved".into()});let pointer=app.comments.as_ptr();
        app.project_exchange=Some(sleufbase_rust::project_exchange_worker::Worker::with_processor(|_,_,control|loop{control.check()?;std::thread::sleep(Duration::from_millis(2));}).unwrap());
        let dir=tempfile::tempdir().unwrap();app.start_python_project_export(dir.path().join("export.klicproject"));
        assert!(app.project_exchange_busy());let unintended=dir.path().join("must-not-save.json");assert!(!app.save_project_to_path(unintended.clone()));assert!(!unintended.exists());
        let ctx=egui::Context::default();app.request_project_action(ProjectAction::Close,&ctx);assert_eq!(app.exchange_deferred,Some(ProjectAction::Close));assert!(!app.close_confirmed);
        let deadline=Instant::now()+Duration::from_secs(5);while app.project_exchange_busy(){assert!(Instant::now()<deadline);app.poll_project_exchange(&ctx);std::thread::sleep(Duration::from_millis(2));}
        assert_eq!(app.comments.as_ptr(),pointer);assert_eq!(app.project_path,Some("original.json".into()));assert!(app.dirty);assert_eq!(app.pending_project_action,Some(ProjectAction::Close));assert!(!app.close_confirmed);
        app.finish_project_action(false,&ctx);assert!(app.pending_project_action.is_none());assert_eq!(app.comments[0].text,"unsaved");assert!(!dir.path().join("export.klicproject").exists());
    }
}
