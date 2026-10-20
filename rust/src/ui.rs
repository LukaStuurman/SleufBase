//! Native Rust desktop UI, no Python runtime or IPC dependency.
//! Native project format is deliberately separate from legacy Python projects.

use eframe::egui::{self, Color32, Pos2, Stroke};
use sleufbase_rust::{
    Bounds, Dataset, Geometry,
    Vertex
};
#[cfg(test)]
use sleufbase_rust::RULES;
use sleufbase_rust::dxf_read::DxfOverlay;
#[cfg(test)]
use sleufbase_rust::dxf_read::load_dxf;
use sleufbase_rust::virtual_trench::VirtualTrench;
use sleufbase_rust::marxact_alignment;
use sleufbase_rust::pdok_wms::{self, PdokRaster};
use sleufbase_rust::osm_tiles::{self, OsmRaster};
use sleufbase_rust::bgt::{self,BgtKind,BgtLayer};
use sleufbase_rust::cadastral::{self,CadastralLayer};
use sleufbase_rust::ahn;
use sleufbase_rust::jobs_cache::{read_cache,JobsCache,Job};
use sleufbase_rust::kickthemap_live::{KickTheMapLiveClient,RemoteFileKind};
use sleufbase_rust::settings_compat::{self,MigratedSettings};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use sleufbase_rust::geotiff::GeoTiffInfo;
use sleufbase_rust::loaded_jobs::kickthemap_cache_dir;
use sleufbase_rust::project::{
    self, AutosaveSettings, NativeProject, Comment, save_project
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[path="ui_file_loading.rs"]
mod file_loading;
#[path="ui_exporting.rs"]
mod exporting;
#[path="ui_template_export.rs"]
mod template_exporting;
#[path="ui_project_template.rs"]
mod project_template_ui;
#[path="ui_trench_editor.rs"]
mod trench_editing;
#[path="ui_kickthemap_browser.rs"]
mod kick_browser_ui;

const ORANGE: Color32 = Color32::from_rgb(249, 115, 22);
const DARK: Color32 = Color32::from_rgb(194, 65, 12);
const INK: Color32 = Color32::from_rgb(17, 24, 39);

#[derive(Default)]
struct CommentForm {
    x:f64,y:f64,text:String,
}
struct TrenchForm {
    label:String, start_x:f64, start_y:f64, end_x:f64, end_y:f64, width:f64,
}
impl Default for TrenchForm {
    fn default()->Self {
        Self {label:"Virtuele proefsleuf".into(),start_x:0.0,start_y:0.0,
            end_x:10.0,end_y:0.0,width:0.6}
    }
}
impl TrenchForm {
    fn make(&self)->Result<VirtualTrench,String> {
        if self.label.trim().is_empty() {return Err("Geef de proefsleuf een naam".into());}
        VirtualTrench::new(self.label.trim().into(),
            Vertex{x:self.start_x,y:self.start_y,z:None},
            Vertex{x:self.end_x,y:self.end_y,z:None},self.width)
    }
}


#[derive(Clone,Debug,PartialEq)]
enum ProjectAction { New, Open(PathBuf), Restore(PathBuf), ImportPython(PathBuf), Close }

struct DxfSelection {overlay:usize,feature:usize,details:Vec<(String,String)>}

pub struct SleufBaseApp {
    file_loader:Option<sleufbase_rust::file_loader::FileLoader>,
    exporter:Option<sleufbase_rust::export_worker::Exporter>,
    export_intent:Option<exporting::ExportIntent>,
    template_dialog:Option<template_exporting::TemplateDialog>,
    project_template_dialog:Option<project_template_ui::Dialog>,
    template_preferences:Option<sleufbase_rust::template_prefs::TemplatePreferences>,
    template_job:Option<template_exporting::TemplateJob>,
    file_intent:Option<file_loading::FileIntent>,
    datasets: Vec<Dataset>,
    geotiffs: Vec<GeoTiffInfo>,
    raster_textures: Vec<Option<egui::TextureHandle>>,
    raster_loader:Option<sleufbase_rust::raster_loader::RasterLoader>,
    raster_errors:Vec<Option<String>>,
    overlays: Vec<DxfOverlay>,
    dxf_selection:Option<DxfSelection>,
    virtual_trenches: Vec<VirtualTrench>,
    trench_markers:Vec<Vec<sleufbase_rust::cross_section::Marker>>,
    show_trench_dialog: bool,
    trench_form:TrenchForm,
    trench_editor:Option<trench_editing::Editor>,
    pdok_texture:Option<egui::TextureHandle>,
    pdok_bounds:Option<Bounds>,
    pdok_rx:Option<Receiver<Result<PdokRaster,String>>>,
    pdok_busy:bool,
    osm_texture:Option<egui::TextureHandle>,
    osm_bounds:Option<Bounds>,
    osm_rx:Option<Receiver<Result<OsmRaster,String>>>,
    osm_busy:bool,
    osm_visible:bool,
    bgt_layers:Vec<BgtLayer>,
    bgt_rx:Option<Receiver<Result<BgtLayer,String>>>,
    bgt_busy:bool,
    cadastral:Option<CadastralLayer>,
    cadastral_rx:Option<Receiver<Result<CadastralLayer,String>>>,
    cadastral_busy:bool,
    ahn_rx:Option<Receiver<Result<Option<f64>,String>>>,
    ahn_busy:bool,
    jobs_cache:Option<JobsCache>,
    live_email:String,
    live_password:String,
    live_session:Option<KickTheMapLiveClient>,
    browser_ui:kick_browser_ui::BrowserUi,
    live_jobs:Vec<Job>,
    live_rx:Option<Receiver<Result<(KickTheMapLiveClient,Vec<Job>),String>>>,
    live_download_rx:Option<Receiver<(KickTheMapLiveClient,Result<(RemoteFileKind,PathBuf),String>)>>,
    live_busy:bool,
    jobs_query:String,
    comments:Vec<Comment>,
    show_comment_dialog:bool,
    show_settings_dialog:bool,
    rule_editor:Option<crate::rule_editor::Editor>,
    export_dialog:Option<crate::export_dialog::Dialog>,
    template_order_dialog:Option<crate::template_dialog::Dialog>,
    template_state:Option<sleufbase_rust::template_order::State>,
    migrated_settings:MigratedSettings,
    comment_form:CommentForm,
    only_active_cached:bool,
    active_only: bool,
    selected: Option<usize>,
    query: String,
    message: String,
    project_path: Option<PathBuf>,
    legacy_archive:Option<sleufbase_rust::legacy_project::LegacyArchive>,
    project_name: String,
    dirty: bool,
    pending_project_action:Option<ProjectAction>,
    close_confirmed:bool,
    project_generation:u64,
    live_download_generation:u64,
    backup_settings: AutosaveSettings,
    last_backup: Instant,
    zoom: f64,
    pan: egui::Vec2,
}

impl Default for SleufBaseApp {
    fn default() -> Self {
        let (migrated_settings,message)=match settings_compat::read_rust_settings(&project::app_data_directory().join("settings.json")) {
            Ok(settings)=>(settings,String::new()),
            Err(error)=>(MigratedSettings::default(),format!("Instellingen niet geladen: {error}"))
        };
        Self {
            file_loader:None,
            exporter:None,
            export_intent:None,
            template_dialog:None,
            project_template_dialog:None,
            template_preferences:None,
            template_job:None,
            file_intent:None,
            datasets:Vec::new(),
            geotiffs:Vec::new(),
            raster_textures:Vec::new(),
            raster_loader:None,
            raster_errors:Vec::new(),
            overlays:Vec::new(),
            dxf_selection:None,
            virtual_trenches:Vec::new(),
            trench_markers:Vec::new(),
            show_trench_dialog:false,
            trench_form:TrenchForm::default(),
            trench_editor:None,
            pdok_texture:None,
            pdok_bounds:None,
            pdok_rx:None,
            pdok_busy:false,
            osm_texture:None,
            osm_bounds:None,
            osm_rx:None,
            osm_busy:false,
            osm_visible:false,
            bgt_layers:Vec::new(),
            bgt_rx:None,
            bgt_busy:false,
            cadastral:None,
            cadastral_rx:None,
            cadastral_busy:false,
            ahn_rx:None,
            ahn_busy:false,
            jobs_cache:None,
            live_email:String::new(),
            live_password:String::new(),
            live_session:None,
            browser_ui:kick_browser_ui::BrowserUi::default(),
            live_jobs:Vec::new(),
            live_rx:None,
            live_download_rx:None,
            live_busy:false,
            jobs_query:String::new(),
            comments:Vec::new(),
            show_comment_dialog:false,
            show_settings_dialog:false,
            rule_editor:None,
            export_dialog:None,
            template_order_dialog:None,
            template_state:None,
            migrated_settings,
            comment_form:CommentForm::default(),
            only_active_cached:false,
            active_only:false,
            selected:None,
            query:String::new(),
            message,
            project_path:None,
            legacy_archive:None,
            project_name:"Naamloos project".into(),
            dirty:false,
            pending_project_action:None,
            close_confirmed:false,
            project_generation:0,
            live_download_generation:0,
            backup_settings:project::read_autosave_settings(
                &project::app_data_directory().join("autosave-settings.json")),
            last_backup:Instant::now(),
            zoom:1.0,
            pan:egui::Vec2::ZERO,
        }
    }
}

impl SleufBaseApp {
    fn snapshot(&self) -> NativeProject {
        NativeProject {
            name:self.project_name.clone(),
            legacy_archive:self.legacy_archive.clone(),
            template_preferences:self.template_preferences.clone(),
            datasets:self.datasets.clone(),
            rasters:self.geotiffs.clone(),
            overlays:self.overlays.clone(),
            bgt_layers:self.bgt_layers.clone(),
            cadastral:self.cadastral.clone(),
            virtual_trenches:self.virtual_trenches.clone(),
            comments:self.comments.clone(),
            template_export:self.template_state.clone(),
            ..NativeProject::default()
        }
    }

    fn clear_project_requests(&mut self) {
        self.cancel_file_load();
        self.export_dialog=None;
        self.template_dialog=None;
        self.template_order_dialog=None;
        self.project_template_dialog=None;
        debug_assert!(self.template_job.is_none(), "active template exports must settle before switching projects");
        self.project_generation=self.project_generation.wrapping_add(1);
        self.pdok_rx=None; self.pdok_busy=false;
        self.osm_rx=None; self.osm_busy=false;
        self.osm_texture=None; self.osm_bounds=None; self.osm_visible=false;
        if let Some(loader)=&mut self.raster_loader {loader.replace(Vec::new(),Default::default());}
        self.bgt_rx=None; self.bgt_busy=false;
        self.cadastral_rx=None; self.cadastral_busy=false;
        self.ahn_rx=None; self.ahn_busy=false;
        self.pdok_texture=None; self.pdok_bounds=None;
        self.show_trench_dialog=false; self.show_comment_dialog=false;
        self.trench_editor=None;
        self.zoom=1.0; self.pan=egui::Vec2::ZERO;
        self.last_backup=Instant::now();
        // Keep an in-flight download receiver so its in-memory login session
        // can return. Its generation prevents importing into a different project.
    }

    fn apply_project(&mut self,doc:NativeProject,path:Option<PathBuf>,restored:bool,markers:Vec<Vec<sleufbase_rust::cross_section::Marker>>,_ctx:&egui::Context) {
        self.clear_project_requests();
        self.datasets=doc.datasets; self.geotiffs=doc.rasters;
        self.template_state=doc.template_export;
        self.raster_textures=vec![None;self.geotiffs.len()]; self.overlays=doc.overlays;
        self.dxf_selection=None;
        self.bgt_layers=doc.bgt_layers; self.cadastral=doc.cadastral;
        self.virtual_trenches=doc.virtual_trenches; self.comments=doc.comments;
        self.trench_markers=markers;
        self.project_name=doc.name; self.project_path=path;
        self.legacy_archive=doc.legacy_archive;
        self.template_preferences=doc.template_preferences;
        self.selected=(!self.datasets.is_empty()).then_some(0);
        self.dirty=restored;
        self.schedule_raster_previews();
    }

    fn request_project_action(&mut self,action:ProjectAction,ctx:&egui::Context) {
        if self.defer_action_for_export(&action)||self.defer_action_for_template(&action){return;}
        if self.pending_project_action.is_some() {return;}
        self.cancel_file_load();
        if self.dirty {self.pending_project_action=Some(action);}
        else {self.execute_project_action(action,ctx);}
    }

    fn finish_project_action(&mut self,proceed:bool,ctx:&egui::Context) {
        if let Some(action)=self.pending_project_action.take() {
            if proceed {self.execute_project_action(action,ctx);}
        }
    }

    fn execute_project_action(&mut self,action:ProjectAction,ctx:&egui::Context) {
        if self.defer_action_for_export(&action)||self.defer_action_for_template(&action){return;}
        match action {
            ProjectAction::New=>{
                self.apply_project(NativeProject::default(),None,false,Vec::new(),ctx);
                self.message="Nieuw leeg Rust-project".into();
            }
            ProjectAction::Close=>{
                self.cancel_file_load();self.close_confirmed=true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            other=>self.start_project_load(other),
        }
    }

    fn restore_backup(&mut self,ctx:&egui::Context) {
        let dir=project::backup_directory();
        if !dir.is_dir() {self.message="Geen SleufBase Rust-back-ups gevonden".into();return;}
        if let Some(path)=rfd::FileDialog::new()
            .set_directory(&dir).add_filter("SleufBase backups",&["json"]).pick_file()
        {
            let allowed=path.canonicalize().ok().zip(dir.canonicalize().ok())
                .is_some_and(|(file,dir)| file.starts_with(dir));
            if !allowed {self.message="Back-up ligt niet in de Rust-back-upmap".into();return;}
            self.request_project_action(ProjectAction::Restore(path),ctx);
        }
    }

    fn show_unsaved_project_dialog(&mut self,ctx:&egui::Context) {
        if self.pending_project_action.is_none() {return;}
        let mut choice=None;
        let response=egui::Modal::new(egui::Id::new("unsaved_project")).show(ctx,|ui|{
            ui.heading("Projectwijzigingen opslaan?");
            ui.label("Dit project bevat onopgeslagen wijzigingen.");
            ui.horizontal(|ui|{
                if ui.button("Opslaan en doorgaan").clicked() {choice=Some(0);}
                if ui.button("Zonder opslaan").clicked() {choice=Some(1);}
                if ui.button("Annuleren").clicked() {choice=Some(2);}
            });
            if !self.message.is_empty() {ui.label(&self.message);}
        });
        match choice {
            Some(0)=>{if self.save_project_file(false) {self.finish_project_action(true,ctx);}},
            Some(1)=>self.finish_project_action(true,ctx),
            Some(2)=>self.finish_project_action(false,ctx),
            _=>{if response.should_close() {self.finish_project_action(false,ctx);}},
        }
    }

    fn save_project_file(&mut self, save_as: bool) -> bool {
        if self.export_intent.is_some() || self.template_job.is_some(){self.message="Wacht tot de export is afgerond".into();return false;}
        let path = if !save_as {
            self.project_path.clone()
        } else { None };
        let path=path.or_else(||rfd::FileDialog::new()
            .add_filter("SleufBase Rust Project", &["json"])
            .set_file_name("Project.sleufbase-rust.json")
            .save_file());
        path.is_some_and(|path|self.save_project_to_path(path))
    }

    fn save_project_to_path(&mut self,path:PathBuf) -> bool {
        if self.export_intent.is_some() || self.template_job.is_some(){self.message="Wacht tot de export is afgerond".into();return false;}
        match save_project(&path,&self.snapshot()) {
            Ok(())=>{
                self.project_path=Some(path.clone()); self.dirty=false;
                self.message=format!("Rust-project opgeslagen: {}",path.display());
                true
            }
            Err(err)=>{self.message=format!("Project opslaan mislukt: {err}");false}
        }
    }

    fn try_autosave(&mut self) {
        if self.export_intent.is_some() || self.template_job.is_some(){return;}
        if self.file_intent.is_some(){return;}
        if !self.backup_settings.enabled ||
            self.last_backup.elapsed() <
                Duration::from_secs(u64::from(self.backup_settings.interval_minutes)*60)
        {
            return;
        }
        // Reset first even on error to avoid repeatedly writing each repaint.
        self.last_backup=Instant::now();
        if !self.dirty {return;}
        match project::create_backup(
            &self.snapshot(),
            &project::backup_directory(),
            self.backup_settings.max_backups
        ) {
            Ok(path)=>self.message=format!("Automatische Rust-back-up: {}",path.display()),
            Err(err)=>self.message=format!("Automatische back-up mislukt: {err}"),
        }
    }

    fn schedule_raster_previews(&mut self) {
        self.raster_errors=vec![None;self.geotiffs.len()];
        if self.geotiffs.is_empty(){return;}
        if self.raster_loader.is_none() {
            match sleufbase_rust::raster_loader::RasterLoader::new() {
                Ok(loader)=>self.raster_loader=Some(loader),
                Err(err)=>{self.raster_errors.fill(Some(err.clone()));self.message=err;return;}
            }
        }
        let paths=self.geotiffs.iter().enumerate().map(|(id,r)|(id,r.path.clone())).collect();
        self.raster_loader.as_mut().unwrap().replace(paths,
            sleufbase_rust::raster_loader::preview_options_for_layers(self.geotiffs.len()));
    }

    fn select_dxf(&mut self,overlay:usize,feature:usize) {
        self.dxf_selection=self.overlays.get(overlay).and_then(|o|o.inspect_details(feature).map(|details|
            DxfSelection{overlay,feature,details:details.into_iter().collect()}
        ));
    }

    fn poll_raster_previews(&mut self,ctx:&egui::Context) {
        let Some(loader)=&mut self.raster_loader else{return;};
        for ready in loader.poll() {
            if self.geotiffs.get(ready.id).is_none_or(|r|r.path!=ready.path){continue;}
            match ready.result {
                Ok(preview)=>{
                    let image=egui::ColorImage::from_rgba_unmultiplied(
                        [preview.width as usize,preview.height as usize],&preview.rgba);
                    self.raster_textures[ready.id]=Some(ctx.load_texture(
                        ready.path.to_string_lossy(),image,egui::TextureOptions::LINEAR));
                    self.raster_errors[ready.id]=None;
                }
                Err(err)=>{self.raster_textures[ready.id]=None;self.raster_errors[ready.id]=Some(err);}
            }
        }
        if loader.remaining()>0 {ctx.request_repaint_after(Duration::from_millis(50));}
    }

    fn current_bounds(&self)->Option<Bounds> {
        let mut bounds=Bounds::from_datasets(&self.datasets);
        for raster in &self.geotiffs {bounds=union_bounds(bounds,Some(raster.bounds));}
        for overlay in self.overlays.iter().filter(|o|o.visible) {
            bounds=union_bounds(bounds,overlay.bounds());
        }
        for t in &self.virtual_trenches {
            for (x,y) in t.measured_boundary() {
                bounds=union_bounds(bounds,Some(Bounds{
                    min_x:x-0.01,min_y:y-0.01,max_x:x+0.01,max_y:y+0.01
                }));
            }
        }
        bounds
    }



    fn request_ahn(&mut self) {
        if self.ahn_busy {return;}
        let Some(bounds)=self.current_bounds() else {
            self.message="Laad eerst een RD-kaartgebied om AHN te raadplegen".into();
            return;
        };
        let x=(bounds.min_x+bounds.max_x)*0.5;
        let y=(bounds.min_y+bounds.max_y)*0.5;
        if self.geotiffs.iter().any(|r|r.epsg.is_some_and(|e|e!=28992)) {
            self.message="AHN ondersteunt hier alleen RD EPSG:28992".into();
            return;
        }
        let (tx,rx)=mpsc::channel();
        self.ahn_rx=Some(rx);
        self.ahn_busy=true;
        self.message=format!("AHN maaiveldhoogte rond RD {:.2} / {:.2} ophalen...",x,y);
        std::thread::spawn(move||{
            let _=tx.send(ahn::ground_level(x,y));
        });
    }
    fn poll_ahn(&mut self) {
        let event=self.ahn_rx.as_ref().map(|rx|rx.try_recv());
        match event {
            Some(Ok(Ok(Some(value))))=>{
                self.message=format!("AHN maaiveldhoogte rond midden kaart: {value:.3} m NAP");
                self.ahn_busy=false;
                self.ahn_rx=None;
            }
            Some(Ok(Ok(None)))=>{
                self.message="Geen geldige AHN-hoogte op deze locatie".into();
                self.ahn_busy=false;
                self.ahn_rx=None;
            }
            Some(Ok(Err(err)))=>{
                self.message=format!("AHN-service: {err}");
                self.ahn_busy=false;
                self.ahn_rx=None;
            }
            Some(Err(TryRecvError::Disconnected))=>{
                self.message="AHN-verzoek afgebroken".into();
                self.ahn_busy=false;
                self.ahn_rx=None;
            }
            _=>{}
        }
    }


    fn start_cadastral(&mut self) {
        if self.cadastral_busy {return;}
        if self.geotiffs.iter().any(|r|r.epsg.is_some_and(|epsg|epsg!=28992)) {
            self.message="Kadaster WFS vereist RD-coördinaten EPSG:28992".into();
            return;
        }
        let Some(bounds)=self.current_bounds() else {
            self.message="Laad een RD-gebied om kadastrale gegevens op te halen".into();
            return;
        };
        if let Err(err)=cadastral::tiles(bounds) {
            self.message=format!("Kadaster WFS: {err}");return;
        }
        let (tx,rx)=mpsc::channel();
        self.cadastral_rx=Some(rx);
        self.cadastral_busy=true;
        self.message="Kadastrale PDOK-lijnen en labels ophalen...".into();
        std::thread::spawn(move||{
            let _=tx.send(cadastral::fetch_cadastral(bounds));
        });
    }
    fn poll_cadastral(&mut self){
        let event=self.cadastral_rx.as_ref().map(|rx|rx.try_recv());
        match event{
            Some(Ok(Ok(data)))=>{
                let count=data.linework.iter().map(|g|g.paths.len()).sum::<usize>();
                self.message=format!("Kadastrale gegevens: {count} lijnen en {} labels",data.labels.len());
                self.cadastral=Some(data);
                self.cadastral_busy=false;
                self.cadastral_rx=None;
                self.dirty=true;
            }
            Some(Ok(Err(err)))=>{
                self.message=format!("Kadaster WFS: {err}");
                self.cadastral_busy=false;
                self.cadastral_rx=None;
            }
            Some(Err(TryRecvError::Disconnected))=>{
                self.message="Kadaster WFS-verzoek onderbroken".into();
                self.cadastral_busy=false;
                self.cadastral_rx=None;
            }
            _=>{}
        }
    }

    fn start_bgt(&mut self,kind:BgtKind) {
        if self.bgt_busy {return;}
        if self.geotiffs.iter().any(|r|r.epsg.is_some_and(|epsg|epsg!=28992)) {
            self.message="BGT vereist RD EPSG:28992, transformeer andere projecties eerst".into();
            return;
        }
        let Some(bounds)=self.current_bounds() else {
            self.message="Laad eerst een RD-kaartlaag".into();
            return;
        };
        if let Err(err)=bgt::build_url(kind,bounds) {
            self.message=format!("BGT: {err}");
            return;
        }
        let (tx,rx)=mpsc::channel();
        self.bgt_rx=Some(rx);
        self.bgt_busy=true;
        self.message=format!("{} worden geladen...",kind.label());
        std::thread::spawn(move||{
            let result=bgt::request_bgt(kind,bounds);
            let _=tx.send(result);
        });
    }

    fn poll_bgt(&mut self) {
        let event=self.bgt_rx.as_ref().map(|rx|rx.try_recv());
        match event {
            Some(Ok(Ok(layer)))=>{
                self.bgt_layers.retain(|l|l.kind!=layer.kind);
                self.message=format!("{}: {} paden geladen",layer.kind.label(),layer.paths.len());
                self.bgt_layers.push(layer);
                self.bgt_rx=None;
                self.bgt_busy=false;
                self.dirty=true;
            }
            Some(Ok(Err(err)))=>{
                self.message=format!("BGT ophalen mislukt: {err}");
                self.bgt_rx=None;
                self.bgt_busy=false;
            }
            Some(Err(TryRecvError::Disconnected))=>{
                self.message="BGT-verzoek afgebroken".into();
                self.bgt_rx=None;
                self.bgt_busy=false;
            }
            _=>{}
        }
    }

    fn start_pdok(&mut self) {
        if self.pdok_busy {return;}
        if self.geotiffs.iter().any(|i|i.epsg.is_some_and(|epsg|epsg!=28992)) {
            self.message="PDOK luchtfoto is RD (EPSG:28992). Transformeer afwijkende GeoTIFF eerst.".into();
            return;
        }
        let Some(bounds)=self.current_bounds() else {
            self.message="Laad eerst een RD-kaartlaag om de PDOK-luchtfoto op te halen".into();
            return;
        };
        let (sender,receiver)=mpsc::channel();
        self.pdok_rx=Some(receiver);
        self.pdok_busy=true;
        self.message="PDOK-luchtfoto wordt rechtstreeks vanuit Rust geladen".into();
        std::thread::spawn(move||{
            let result=pdok_wms::request_map(bounds,1200,900);
            let _=sender.send(result);
        });
    }

    fn poll_pdok(&mut self,ctx:&egui::Context) {
        let state=self.pdok_rx.as_ref().map(|rx|rx.try_recv());
        match state {
            Some(Ok(Ok(raster)))=>{
                let image=egui::ColorImage::from_rgba_unmultiplied(
                    [raster.width as usize,raster.height as usize],&raster.rgba
                );
                self.pdok_texture=Some(ctx.load_texture(
                    "PDOK Actueel_orthoHR",image,egui::TextureOptions::LINEAR));
                self.pdok_bounds=Some(raster.bounds);
                self.pdok_busy=false;
                self.pdok_rx=None;
                self.osm_visible=false;
                self.message="PDOK-luchtfoto geladen".into();
            }
            Some(Ok(Err(err)))=>{
                self.pdok_busy=false;
                self.pdok_rx=None;
                self.message=format!("PDOK-luchtfoto mislukt: {err}");
            }
            Some(Err(TryRecvError::Disconnected))=>{
                self.pdok_busy=false;
                self.pdok_rx=None;
                self.message="PDOK-verzoek onverwacht afgebroken".into();
            }
            _=>{}
        }
    }



    fn start_osm(&mut self) {
        if self.osm_busy {return;}
        if self.geotiffs.iter().any(|i| i.epsg.is_some_and(|epsg|epsg!=28992)) {
            self.message="OSM-kaart is RD (EPSG:28992). Transformeer afwijkende GeoTIFF eerst.".into();
            return;
        }
        let Some(bounds)=self.current_bounds() else {
            self.message="Laad eerst een RD-kaartlaag om OpenStreetMap op te halen".into();
            return;
        };
        let (sender,receiver)=mpsc::channel();
        self.osm_rx=Some(receiver);
        self.osm_busy=true;
        self.message="OpenStreetMap-tegels worden geladen (lokaal 7 dagen gecachet)".into();
        let cache=project::app_data_directory().join("osm-tiles");
        std::thread::spawn(move||{
            let result=osm_tiles::request_map(bounds,1200,900,cache);
            let _=sender.send(result);
        });
    }

    fn poll_osm(&mut self,ctx:&egui::Context) {
        let state=self.osm_rx.as_ref().map(|rx|rx.try_recv());
        match state {
            Some(Ok(Ok(raster)))=>{
                let image=egui::ColorImage::from_rgba_unmultiplied(
                    [raster.width as usize,raster.height as usize],&raster.rgba
                );
                self.osm_texture=Some(ctx.load_texture(
                    "OpenStreetMap",image,egui::TextureOptions::LINEAR));
                self.osm_bounds=Some(raster.bounds);
                self.osm_visible=true;
                self.osm_busy=false;
                self.osm_rx=None;
                self.message=if raster.stale_cache {
                    "OSM-kaart geladen uit verouderde cache (netwerk niet beschikbaar)".into()
                } else {"OpenStreetMap-kaart geladen".into()};
            }
            Some(Ok(Err(err)))=>{
                self.osm_busy=false;
                self.osm_rx=None;
                self.message=format!("OpenStreetMap mislukt: {err}");
            }
            Some(Err(TryRecvError::Disconnected))=>{
                self.osm_busy=false;
                self.osm_rx=None;
                self.message="OSM-verzoek onverwacht afgebroken".into();
            }
            _=>{}
        }
    }

    fn import_old_settings(&mut self) {
        if let Some(path)=rfd::FileDialog::new()
            .add_filter("Instellingen JSON",&["json"]).pick_file()
        {
            match settings_compat::import_legacy_file(&path) {
                Ok(config)=>{
                    let output=project::app_data_directory().join("settings.json");
                    match settings_compat::save_rust_settings(&output,&config) {
                        Ok(())=>{
                            self.migrated_settings=config;
                            self.rule_editor=None;
                            self.message="Instellingenopties overgenomen. Wachtwoorden/tokens zijn NIET geïmporteerd.".into();
                        }
                        Err(err)=>self.message=format!("Rust-instellingen niet opgeslagen: {err}")
                    }
                }
                Err(err)=>self.message=format!("Instellingen-import mislukt: {err}")
            }
        }
    }


    fn login_kickthemap(&mut self) {
        if self.live_busy {return;}
        let email=self.live_email.trim().to_owned();
        let password=std::mem::take(&mut self.live_password);
        if email.is_empty() || password.is_empty() {
            self.message="Vul KickTheMap e-mailadres en wachtwoord in".into();
            return;
        }
        // Never log, serialize, or retain credentials in the application state.
        let (tx,rx)=mpsc::channel();
        self.live_rx=Some(rx);
        self.live_busy=true;
        self.message="KickTheMap-login en live jobs ophalen...".into();
        std::thread::spawn(move || {
            let result=KickTheMapLiveClient::login(&email,&password)
                .and_then(|mut session|session.fetch_jobs().map(|jobs|(session,jobs)));
            let _=tx.send(result);
        });
    }

    fn refresh_live_jobs(&mut self) {
        if self.live_busy {return;}
        let Some(mut session)=self.live_session.take() else {
            self.message="Log eerst in bij KickTheMap".into();return;
        };
        let (tx,rx)=mpsc::channel();
        self.live_rx=Some(rx);
        self.live_busy=true;
        self.message="KickTheMap jobs vernieuwen...".into();
        std::thread::spawn(move || {
            let result=session.fetch_jobs();
            let _=tx.send(result.map(|jobs|(session,jobs)));
        });
    }


    fn download_live_job(&mut self,id:u64,kind:RemoteFileKind) {
        if self.live_busy {return;}
        let Some(job)=self.live_jobs.iter().find(|j|j.job_id==id).cloned() else {
            self.message="De KickTheMap-job is niet meer beschikbaar".into();
            return;
        };
        let Some(mut client)=self.live_session.take() else {
            self.message="Log opnieuw in voor het downloaden".into();
            return;
        };
        self.live_busy=true;
        self.message=format!("KickTheMap-job {} ophalen...",job.job_id);
        let (tx,rx)=mpsc::channel();
        self.live_download_generation=self.project_generation;
        self.live_download_rx=Some(rx);
        if !self.browser_ui.batch_mode {
            self.browser_ui.cancel.store(false,std::sync::atomic::Ordering::SeqCst);
        }
        let cancel=std::sync::Arc::clone(&self.browser_ui.cancel);
        let browser=self.browser_ui.browser.as_ref().map(|b|b.channel());
        std::thread::spawn(move || {
            let root=kickthemap_cache_dir().join("Rust");
            let result=client.download_job_with_browser(
                &job,kind,&root,&cancel,browser.as_ref()
            ).map(|p|(kind,p));
            let _=tx.send((client,result));
        });
    }

    fn apply_live_download_result(&mut self,result:Result<(RemoteFileKind,PathBuf),String>,ctx:&egui::Context) {
                if self.live_download_generation!=self.project_generation {
                    self.message=match result {
                        Ok((_,path))=>format!("Download opgeslagen: {}. Open het bestand om het aan dit project toe te voegen.",path.display()),
                        Err(err)=>format!("Download voor vorig project mislukt: {err}"),
                    };
                    return;
                }
                match result {
                    Ok((RemoteFileKind::GeoTiff,path))=>{
                        self.load_geotiffs(vec![path],ctx);
                    }
                    Ok((RemoteFileKind::JobFeatures,path))=>{
                        self.load_files(vec![path]);
                    }
                    Err(err)=>self.message=format!("KickTheMap-download: {err}")
                }
    }

    fn poll_live_download(&mut self,ctx:&egui::Context) {
        let event=self.live_download_rx.as_ref().map(|rx|rx.try_recv());
        match event {
            Some(Ok((client,result)))=>{
                self.live_session=Some(client);
                self.live_download_rx=None;
                self.live_busy=false;
                if self.browser_ui.batch_mode {
                    self.browser_ui.completed+=1;
                    match result {
                        Ok((RemoteFileKind::GeoTiff,path)) =>
                            self.browser_ui.downloaded_tiffs.push(path),
                        Ok((RemoteFileKind::JobFeatures,path)) =>
                            self.browser_ui.downloaded_json.push(path),
                        Err(err)=>self.browser_ui.failures.push(err),
                    }
                } else {
                    self.apply_live_download_result(result,ctx);
                }
            }
            Some(Err(TryRecvError::Disconnected))=>{
                self.live_busy=false;
                self.live_download_rx=None;
                self.message="KickTheMap-download afgebroken; log opnieuw in".into();
            }
            _=>{}
        }
    }

    fn poll_live_jobs(&mut self) {
        let event=self.live_rx.as_ref().map(|rx|rx.try_recv());
        match event {
            Some(Ok(Ok((session,jobs))))=>{
                self.live_session=Some(session);
                self.live_jobs=jobs;
                self.live_busy=false;
                self.live_rx=None;
                self.message=format!("{} KickTheMap jobs live opgehaald",self.live_jobs.len());
                self.save_live_cache();
            }
            Some(Ok(Err(err)))=>{
                self.live_busy=false;
                self.live_rx=None;
                self.live_session=None;
                self.message=format!("KickTheMap live: {err}");
            }
            Some(Err(TryRecvError::Disconnected))=>{
                self.live_busy=false;
                self.live_rx=None;
                self.live_session=None;
                self.message="KickTheMap-verbinding afgebroken".into();
            }
            _=>{}
        }
    }

    fn logout_live_jobs(&mut self) {
        if self.live_busy || self.browser_ui.capturing.is_some() {
            self.message="Rond het lopende browser- of downloadverzoek af voordat je uitlogt".into();
            return;
        }
        self.live_session=None;
        self.live_jobs.clear();
        self.live_password.clear();
        self.browser_ui.browser=None;
        self.browser_ui.selected.clear();
        self.browser_ui.queue.clear();
        self.browser_ui.cancel.store(true,std::sync::atomic::Ordering::SeqCst);
        self.message="KickTheMap-sessie uit het geheugen verwijderd".into();
    }

    fn open_cached_jobs(&mut self) {
        let dir=kickthemap_cache_dir().join("jobs_cache");
        if let Some(path)=rfd::FileDialog::new().set_directory(&dir)
            .add_filter("KickTheMap jobcache",&["json"]).pick_file()
        {
            match read_cache(&path) {
                Ok(cache)=>{
                    self.message=format!("Offline KickTheMap-cache: {} jobs, {} overgeslagen. Niet live.",
                        cache.jobs.len(),cache.skipped);
                    self.jobs_cache=Some(cache);
                }
                Err(err)=>self.message=format!("Jobcache ongeldig: {err}")
            }
        }
    }

    fn active_job_ids(&self) -> std::collections::BTreeSet<u64> {
        // An active Rust job is one actually loaded in this desktop app.
        // Never confuse a historical download or another Python instance's
        // heartbeat with a currently loaded Rust raster/dataset.
        let mut ids=sleufbase_rust::loaded_jobs::active_rust_job_ids(&self.datasets,&self.geotiffs);
        ids.extend(self.virtual_trenches.iter().map(|t|t.job_id).filter(|id|*id>0));
        ids
    }

    fn save_excel(&mut self) {
        if self.export_intent.is_some(){self.message="Er loopt al een export".into();return;}
        if self.datasets.is_empty() && self.virtual_trenches.is_empty() {
            self.message="Laad eerst jobFeatures.json of maak een virtuele proefsleuf".into();
            return;
        }
        if let Some(path)=rfd::FileDialog::new()
            .add_filter("Excel",&["xlsx"])
            .set_file_name("SleufBase-Disciplines-Rust.xlsx").save_file()
        {
            self.start_excel_export(path);
        }
    }

    fn save_dxf(&mut self) {
        if self.export_intent.is_some(){self.message="Er loopt al een export".into();return;}
        self.export_dialog=Some(crate::export_dialog::Dialog::new(self.datasets.len(),self.virtual_trenches.len(),self.migrated_settings.dxf_label_gap,self.project_generation));
    }

}


fn union_bounds(a:Option<Bounds>,b:Option<Bounds>)->Option<Bounds> {
    match (a,b) {
        (None,other)|(other,None)=>other,
        (Some(x),Some(y))=>Some(Bounds {
            min_x:x.min_x.min(y.min_x),min_y:x.min_y.min(y.min_y),
            max_x:x.max_x.max(y.max_x),max_y:x.max_y.max(y.max_y),
        }),
    }
}

fn aci_color(color:i32)->Color32 {
    let [r,g,b]=sleufbase_rust::cad::display_rgb(color.unsigned_abs().min(i32::MAX as u32)as i32,None,None);
    Color32::from_rgb(r,g,b)
}

fn cad_preview(value:&str)->&str {
    let end=value.char_indices().nth(600).map_or(value.len(),|(index,_)|index);
    &value[..end]
}

fn canvas(
    ui:&mut egui::Ui,
    datasets:&[Dataset],
    rasters:&[GeoTiffInfo],
    textures:&[Option<egui::TextureHandle>],
    overlays:&[DxfOverlay],
    virtual_trenches:&[VirtualTrench],
    trench_markers:&[Vec<sleufbase_rust::cross_section::Marker>],
    bgt_layers:&[BgtLayer],
    cadastral:Option<&CadastralLayer>,
    comments:&[Comment],
    pdok_texture:Option<&egui::TextureHandle>,
    pdok_bounds:Option<Bounds>,
    osm_visible:bool,
    zoom:&mut f64,
    pan:&mut egui::Vec2,
    rules:&[sleufbase_rust::Rule]
) {
    let desired=ui.available_size();
    let (rect,response)=ui.allocate_exact_size(desired,egui::Sense::click_and_drag());
    let painter=ui.painter_at(rect);
    painter.rect_filled(rect,5.0,Color32::from_rgb(250,251,253));

    let mut bounds=union_bounds(Bounds::from_datasets(datasets),pdok_bounds);
    for raster in rasters {
        bounds=union_bounds(bounds,Some(raster.bounds));
    }
    for layer in bgt_layers.iter().filter(|l|l.visible) {
        bounds=union_bounds(bounds,Some(layer.bounds));
    }
    if let Some(layer)=cadastral.filter(|l|l.visible) {
        bounds=union_bounds(bounds,Some(layer.bounds));
    }
    for overlay in overlays.iter().filter(|o|o.visible) {
        bounds=union_bounds(bounds,overlay.bounds());
    }
    for note in comments {
        bounds=union_bounds(bounds,Some(Bounds {
            min_x:note.x-0.1,min_y:note.y-0.1,
            max_x:note.x+0.1,max_y:note.y+0.1
        }));
    }
    for t in virtual_trenches {
        for (x,y) in t.measured_boundary() {
            bounds=union_bounds(bounds,Some(Bounds{
                min_x:x-0.01,min_y:y-0.01,max_x:x+0.01,max_y:y+0.01
            }));
        }
    }
    let Some(bounds)=bounds else {
        painter.text(rect.center(),egui::Align2::CENTER_CENTER,
            "Open een lokaal KickTheMap JSON-, GeoTIFF- of DXF-bestand",
            egui::FontId::proportional(17.0),INK);
        return;
    };

    if response.hovered() {
        let scroll=ui.input(|i|i.smooth_scroll_delta.y);
        if scroll!=0.0 {
            *zoom=(*zoom*(1.0+f64::from(scroll)*0.002).clamp(0.25,4.0))
                .clamp(0.05,300.0);
        }
    }
    if response.dragged() {*pan+=response.drag_delta();}

    let width=(bounds.max_x-bounds.min_x).max(1.0);
    let height=(bounds.max_y-bounds.min_y).max(1.0);
    let screen_width=(rect.width()-56.0).max(1.0) as f64;
    let screen_height=(rect.height()-56.0).max(1.0) as f64;
    let scale=(screen_width/width).min(screen_height/height) * *zoom;
    let cx=(bounds.min_x+bounds.max_x)*0.5;
    let cy=(bounds.min_y+bounds.max_y)*0.5;
    let center=rect.center()+*pan;
    let project=|x:f64,y:f64|Pos2::new(
        center.x+((x-cx)*scale) as f32,
        center.y-((y-cy)*scale) as f32
    );
    painter.add(egui::Shape::rect_filled(rect,0.0,Color32::TRANSPARENT));

    if let (Some(texture),Some(b))=(pdok_texture,pdok_bounds) {
        let mut mesh=egui::Mesh::with_texture(texture.id());
        for (x,y,u,v) in [
            (b.min_x,b.max_y,0.0f32,0.0f32),
            (b.max_x,b.max_y,1.0,0.0),
            (b.max_x,b.min_y,1.0,1.0),
            (b.min_x,b.min_y,0.0,1.0)
        ] {
            mesh.vertices.push(egui::epaint::Vertex {
                pos:project(x,y),uv:Pos2::new(u,v),color:Color32::WHITE
            });
        }
        mesh.indices.extend_from_slice(&[0,1,2,0,2,3]);
        painter.add(egui::Shape::mesh(mesh));
    }
    for (raster,texture) in rasters.iter().zip(textures.iter()) {
        if let Some(texture)=texture {
            let corners=[
                (0.0,0.0,0.0f32,0.0f32),
                (f64::from(raster.width),0.0,1.0,0.0),
                (f64::from(raster.width),f64::from(raster.height),1.0,1.0),
                (0.0,f64::from(raster.height),0.0,1.0)
            ];
            let mut mesh=egui::Mesh::with_texture(texture.id());
            for (col,row,u,v) in corners {
                let (x,y)=raster.transform.pixel_to_world(col,row);
                mesh.vertices.push(egui::epaint::Vertex {
                    pos:project(x,y),uv:Pos2::new(u,v),color:Color32::WHITE,
                });
            }
            mesh.indices.extend_from_slice(&[0,1,2,0,2,3]);
            painter.add(egui::Shape::mesh(mesh));
        }
    }
    for note in comments {
        let pos=project(note.x,note.y);
        painter.circle_filled(pos,5.0,DARK);
        painter.text(pos+egui::vec2(6.0,-6.0),egui::Align2::LEFT_BOTTOM,
            &note.text,egui::FontId::proportional(13.0),INK);
    }
    for (index,trench) in virtual_trenches.iter().enumerate() {
        let vertices=trench.measured_boundary();
        for pair in vertices.windows(2) {
            painter.line_segment([
                project(pair[0].0,pair[0].1),
                project(pair[1].0,pair[1].1)
            ],Stroke::new(1.5_f32,ORANGE));
        }
        let start=project(trench.start.x,trench.start.y);
        let end=project(trench.end.x,trench.end.y);
        painter.arrow(start,end-start,Stroke::new(2.0_f32,DARK));
        painter.circle_filled(start,3.5,DARK);
        painter.text(start+egui::vec2(5.0,-5.0),egui::Align2::LEFT_BOTTOM,
            &trench.label,egui::FontId::proportional(12.0),INK);
        if let Some(markers)=trench_markers.get(index) {
            for marker in markers {
                let object=&trench.objects[marker.object_index];
                let color=sleufbase_rust::matching_rule(&object.source_name,rules)
                    .map_or(Color32::from_rgb(37,99,235),|r|aci_color(i32::from(r.color_aci)));
                painter.line_segment([project(marker.start.x,marker.start.y),project(marker.end.x,marker.end.y)],Stroke::new(2.5_f32,color));
            }
        }
    }
    if let Some(kad)=cadastral.filter(|l|l.visible) {
        for layer in &kad.linework {
            let color=match layer.layer_name.as_str() {
                "KAD_GRENS"=>Color32::from_rgb(65,74,84),
                "KAD_BEBOUWING"=>Color32::from_rgb(150,75,54),
                _=>Color32::from_rgb(123,124,126)
            };
            for path in &layer.paths {
                for pair in path.windows(2) {
                    painter.line_segment([
                        project(pair[0].0,pair[0].1),
                        project(pair[1].0,pair[1].1)
                    ],Stroke::new(1.25_f32,color));
                }
            }
        }
        // The rotation is preserved in the model, but egui text rendering
        // here is not rotation-aware yet.
        for label in &kad.labels {
            painter.text(project(label.position.0,label.position.1),
                egui::Align2::CENTER_CENTER,&label.text,
                egui::FontId::proportional(11.0),INK);
        }
    }
    for layer in bgt_layers.iter().filter(|l|l.visible) {
        let color=match layer.kind {
            BgtKind::Wegdeel=>Color32::from_rgb(110,117,126),
            BgtKind::OnbegroeidTerreindeel=>Color32::from_rgb(104,139,89)
        };
        for path in &layer.paths {
            for pair in path.windows(2) {
                painter.line_segment([
                    project(pair[0].0,pair[0].1),
                    project(pair[1].0,pair[1].1)
                ],Stroke::new(1.0_f32,color));
            }
        }
    }
    for overlay in overlays.iter().filter(|o|o.visible) {
        for feature in overlay.rendered_features() {
            if !feature.visible{continue;}
            let [r,g,b]=feature.rgb;
            let stroke=Stroke::new(1.6_f32,Color32::from_rgb(r,g,b));
            if matches!(feature.source.entity_type.as_str(),"POINT"|"INSERT") {
                if let Some(point)=feature.points.first(){painter.circle_filled(project(point.x,point.y),2.5,stroke.color);}
            }
            for path in feature.paths() { for pair in path.windows(2) {
                painter.line_segment([
                    project(pair[0].x,pair[0].y),
                    project(pair[1].x,pair[1].y)
                ],stroke);
            }}
        }
    }
    for dataset in datasets {
        for feature in &dataset.features {
            let color=sleufbase_rust::matching_rule(&feature.source_name,rules)
                .map_or(Color32::GRAY,|r|aci_color(i32::from(r.color_aci)));
            match &feature.geometry {
                Geometry::Point(p)=>{painter.circle_filled(project(p.x,p.y),3.5,color);}
                Geometry::Polyline(vertices)=>{
                    for pair in vertices.windows(2) {
                        painter.line_segment([
                            project(pair[0].x,pair[0].y),
                            project(pair[1].x,pair[1].y)
                        ],Stroke::new(1.8_f32,color));
                    }
                }
            }
        }
    }
    if let Some(pos)=response.hover_pos() {
        let x=cx+f64::from(pos.x-center.x)/scale;
        let y=cy-f64::from(pos.y-center.y)/scale;
        painter.text(pos+egui::vec2(12.0,8.0),egui::Align2::LEFT_TOP,
            format!("X {:.3} / Y {:.3}",x,y),
            egui::FontId::monospace(12.0),INK);
    }
    painter.text(rect.left_bottom()+egui::vec2(12.0,-12.0),
        egui::Align2::LEFT_BOTTOM,
        format!("X: {:.2} - {:.2}, Y: {:.2} - {:.2}; zoom {:.1}x",
            bounds.min_x,bounds.max_x,bounds.min_y,bounds.max_y,zoom),
        egui::FontId::monospace(11.0),Color32::DARK_GRAY);
    if osm_visible {
        painter.text(rect.right_bottom()+egui::vec2(-10.0,-10.0),egui::Align2::RIGHT_BOTTOM,
            "© OpenStreetMap contributors",egui::FontId::proportional(12.0),INK);
    }
}

impl eframe::App for SleufBaseApp {
    fn update(&mut self,ctx:&egui::Context,_frame:&mut eframe::Frame) {self.render(ctx);}
}
impl SleufBaseApp {
    fn render(&mut self,ctx:&egui::Context) {
        if self.template_job.is_some() {
            if ctx.input(|i|i.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.request_project_action(ProjectAction::Close,ctx);
            }
            self.show_template_progress(ctx); return;
        }
        if self.export_intent.is_some(){
            if ctx.input(|i|i.viewport().close_requested()){
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.request_project_action(ProjectAction::Close,ctx);
            }
            self.show_export_progress(ctx);return;
        }
        if ctx.input(|i|i.viewport().close_requested()) && !self.close_confirmed {
            self.cancel_file_load();
            if self.dirty {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.request_project_action(ProjectAction::Close,ctx);
            }
        }
        ctx.set_visuals(egui::Visuals::light());
        if self.file_intent.is_some(){self.show_file_loading(ctx);return;}
        self.poll_file_jobs(ctx);
        self.try_autosave();
        self.poll_raster_previews(ctx);
        self.poll_pdok(ctx);self.poll_osm(ctx);
        self.poll_live_jobs();
        self.poll_live_download(ctx);
        self.poll_kickthemap_browser(ctx);
        self.poll_bgt();
        self.poll_cadastral();
        self.poll_ahn();
        ctx.request_repaint_after(Duration::from_secs(1));
        egui::TopBottomPanel::top("header").show(ctx,|ui|{
            ui.add_space(8.0);
            ui.horizontal(|ui|{
                ui.colored_label(DARK,egui::RichText::new("TECHBASE").strong().size(19.0));
                ui.separator();
                ui.label(egui::RichText::new("SleufBase Rust").strong().size(18.0));
                ui.label(egui::RichText::new("Migratie-preview").color(Color32::GRAY));
                if self.dirty {ui.colored_label(DARK,"● Niet opgeslagen");}
                ui.separator();
                if ui.button("Nieuw project").clicked() {self.request_project_action(ProjectAction::New,ctx);}
                if ui.button("Open Rust-project").clicked() {
                    if let Some(path)=rfd::FileDialog::new()
                        .add_filter("Rust-project JSON",&["json"]).pick_file()
                    {self.request_project_action(ProjectAction::Open(path),ctx);}
                }
                if ui.button("Opslaan").clicked() {self.save_project_file(false);}
                if ui.button("Opslaan als...").clicked() {self.save_project_file(true);}
                if ui.button("Instellingen").clicked() {self.show_settings_dialog=true;}
                if ui.button("Import oude voorkeuren").clicked() {self.import_old_settings();}
                if ui.button("Herstel back-up").clicked() {self.restore_backup(ctx);}
                if ui.button("Import Python-project").clicked() {
                    if let Some(path)=rfd::FileDialog::new().add_filter("Python-project",&["klicproject","json"]).pick_file() {
                        self.request_project_action(ProjectAction::ImportPython(path),ctx);
                    }
                }
            });
            ui.horizontal(|ui|{
                if ui.button("Open jobFeatures.json").clicked() {
                    if let Some(files)=rfd::FileDialog::new()
                        .add_filter("KickTheMap JSON",&["json"]).pick_files()
                    {self.load_files(files);}
                }
                if ui.button("Open GeoTIFF").clicked() {
                    if let Some(files)=rfd::FileDialog::new()
                        .add_filter("GeoTIFF",&["tif","tiff"]).pick_files()
                    {self.load_geotiffs(files,ctx);}
                }
                if ui.button("Virtuele proefsleuf").clicked() {self.show_trench_dialog=true;}
                if ui.button("Opmerking plaatsen").clicked() {self.show_comment_dialog=true;}
                if ui.button("Offline KickTheMap jobs").clicked() {self.open_cached_jobs();}
                if ui.add_enabled(!self.pdok_busy,egui::Button::new("PDOK luchtfoto")).clicked() {self.start_pdok();}
                if ui.add_enabled(!self.osm_busy,egui::Button::new("OpenStreetMap")).clicked() {self.start_osm();}
                if self.osm_texture.is_some() && ui.selectable_label(self.osm_visible,"OSM tonen").clicked() {self.osm_visible=true;}
                if self.pdok_texture.is_some() && ui.selectable_label(!self.osm_visible,"PDOK tonen").clicked() {self.osm_visible=false;}
                if ui.add_enabled(!self.bgt_busy,egui::Button::new("BGT wegdelen")).clicked() {
                    self.start_bgt(BgtKind::Wegdeel);
                }
                if ui.add_enabled(!self.bgt_busy,egui::Button::new("BGT terrein")).clicked() {
                    self.start_bgt(BgtKind::OnbegroeidTerreindeel);
                }
                if ui.add_enabled(!self.cadastral_busy,egui::Button::new("Kadaster WFS")).clicked() {
                    self.start_cadastral();
                }
                if ui.add_enabled(!self.ahn_busy,egui::Button::new("AHN hoogte")).clicked() {
                    self.request_ahn();
                }
                if ui.button("MarXact importeren").clicked() {
                    if let Some(files)=rfd::FileDialog::new()
                        .add_filter("MarXact DXF",&["dxf"]).pick_files()
                    {self.import_marxact(files);}
                }
                if ui.button("Open DXF").clicked() {
                    if let Some(files)=rfd::FileDialog::new()
                        .add_filter("AutoCAD DXF",&["dxf"]).pick_files()
                    {self.load_overlays(files);}
                }
                ui.separator();
                if ui.button("Excel disciplines").clicked() {self.save_excel();}
                if ui.button("DXF objecten").clicked() {self.save_dxf();}
                ui.menu_button("DXF-sjabloon", |ui| {
                    if ui.button("TIFF-bladen en PS-varianten…").clicked() {self.open_template_export();ui.close_menu();}
                    if ui.button("Jobs en virtuele proefsleuven…").clicked() {self.open_project_template_dialog();ui.close_menu();}
                    if ui.button("Opgeslagen exportinvoer…").clicked() {self.open_template_dialog();ui.close_menu();}
                });
                if ui.button("Kaart terugzetten").clicked() {self.zoom=1.0;self.pan=egui::Vec2::ZERO;}
            });
            if !self.message.is_empty() {
                ui.label(egui::RichText::new(&self.message).small().color(DARK));
            }
        });

        if self.file_intent.is_some(){self.show_file_loading(ctx);return;}
        if self.export_intent.is_some(){self.show_export_progress(ctx);return;}
        if self.template_dialog.is_some() {self.show_template_dialog(ctx);}
        if self.project_template_dialog.is_some() {self.show_project_template_dialog(ctx);}
        egui::SidePanel::left("sources")
            .default_width(300.0).min_width(230.0).show(ctx,|ui|{
            egui::ScrollArea::vertical().id_salt("source_controls").show(ui,|ui|{
            if let Some(archive)=&self.legacy_archive {
                egui::CollapsingHeader::new("Python-project: importmeldingen").default_open(true).show(ui,|ui|{
                    for notice in &archive.notices {ui.label(notice);}
                });
                ui.separator();
            }
            ui.heading("KickTheMap online");
            if ui.button("Open KickTheMap browser & jobbeheer").clicked(){self.open_kickthemap_workspace();}
            ui.horizontal(|ui|{
                ui.label("E-mail");
                ui.add(egui::TextEdit::singleline(&mut self.live_email)
                    .desired_width(170.0).hint_text("e-mailadres"));
            });
            ui.horizontal(|ui|{
                ui.label("Wachtwoord");
                ui.add(egui::TextEdit::singleline(&mut self.live_password)
                    .password(true).desired_width(150.0));
            });
            ui.horizontal(|ui|{
                if ui.add_enabled(!self.live_busy,egui::Button::new("Inloggen")).clicked() {
                    self.login_kickthemap();
                }
                if ui.add_enabled(!self.live_busy && self.live_session.is_some(),
                    egui::Button::new("Vernieuwen")).clicked() {
                    self.refresh_live_jobs();
                }
                if ui.button("Uitloggen").clicked() {
                    self.logout_live_jobs();
                }
            });
            if self.live_busy {ui.spinner();}
            if self.live_session.is_some() {
                ui.small(format!("{} live jobs (niet opgeslagen)",self.live_jobs.len()));
                let mut request_download=None;
                egui::ScrollArea::vertical().max_height(175.0).show(ui,|ui|{
                    for job in &self.live_jobs {
                        ui.label(format!("{} — {} ({})",
                            job.job_id,job.title,job.municipality));
                        ui.horizontal(|ui|{
                            if ui.add_enabled(!self.live_busy,
                                egui::Button::new("GeoTIFF")).clicked() {
                                request_download=Some((job.job_id,RemoteFileKind::GeoTiff));
                            }
                            if ui.add_enabled(!self.live_busy,
                                egui::Button::new("Objecten")).clicked() {
                                request_download=Some((job.job_id,RemoteFileKind::JobFeatures));
                            }
                        });
                    }
                });
                if let Some((id,kind))=request_download {
                    self.download_live_job(id,kind);
                }
            }
            ui.separator();
            ui.heading("Ingeladen proefsleuven");
            ui.checkbox(&mut self.active_only,"Alleen ingeladen in Rust");
            if self.active_only {
                ui.small("Filtert uitsluitend objecten en GeoTIFFs die nu in Rust zijn geladen.");
            }
            let active_ids=self.active_job_ids();
            ui.text_edit_singleline(&mut self.query);
            ui.small("Zoek op naam of job-ID");
            ui.separator();
            egui::ScrollArea::vertical().max_height(180.0).show(ui,|ui|{
                for (index,ds) in self.datasets.iter().enumerate() {
                    if self.active_only && !active_ids.contains(&ds.job_id) {continue;}
                    let search=self.query.trim().to_lowercase();
                    if !search.is_empty() && !ds.title.to_lowercase().contains(&search)
                        && !ds.job_id.to_string().contains(&search) {continue;}
                    let label=format!("{}  ({} objecten)",ds.title,ds.features.len());
                    if ui.selectable_label(self.selected==Some(index),label).clicked() {
                        self.selected=Some(index);
                    }
                }
            });
            ui.separator();
            ui.heading("GeoTIFF-bestanden");
            egui::ScrollArea::vertical().max_height(130.0).show(ui,|ui|{
                for (index,info) in self.geotiffs.iter().enumerate() {
                    ui.label(info.path.file_name().and_then(|s|s.to_str()).unwrap_or("TIFF"));
                    ui.small(format!("{}x{}, EPSG {}, job {}",
                        info.width,info.height,
                        info.epsg.map(|v|v.to_string()).unwrap_or_else(||"?".into()),
                        info.kickthemap_job_id.map(|v|v.to_string()).unwrap_or_else(||"?".into())));
                    if let Some(error)=self.raster_errors.get(index).and_then(|e|e.as_ref()) {
                        ui.colored_label(DARK,error);
                    } else if self.raster_textures.get(index).is_none_or(|t|t.is_none()) {
                        ui.horizontal(|ui|{ui.spinner();ui.small("Raster laden...");});
                    }
                }
            });
            ui.separator();

            if let Some(cache)=&self.jobs_cache {
                ui.separator();
                ui.heading("KickTheMap jobs (offline cache)");
                ui.small(format!("{} jobs, opgeslagen op {:.0} (Unix)",cache.jobs.len(),cache.saved_at));
                ui.checkbox(&mut self.only_active_cached,"Alleen actief geladen");
                ui.text_edit_singleline(&mut self.jobs_query);
                let needle=self.jobs_query.trim().to_lowercase();
                egui::ScrollArea::vertical().max_height(130.0).show(ui,|ui|{
                    for job in cache.jobs.iter().filter(|j|{
                        (!self.only_active_cached||active_ids.contains(&j.job_id)) &&
                        (needle.is_empty()||j.title.to_lowercase().contains(&needle)||
                            j.job_id.to_string().contains(&needle))
                    }) {
                        ui.label(format!("{}: {} ({})",job.job_id,job.title,job.municipality));
                    }
                });
            }
            ui.separator();
            ui.heading("Virtuele proefsleuven");
            let mut edit_virtual = None;
            for (index,trench) in self.virtual_trenches.iter_mut().enumerate() {
                ui.horizontal(|ui|{
                    ui.label(format!("{} ({:.2} m, {} objecten{})",
                        trench.label,trench.length(),trench.objects.len(),
                        if trench.boundary_3d.is_empty() {""} else {", MarXact 3D"}
                    ));
                    if ui.small_button("Bewerken").clicked(){edit_virtual=Some(index);}
                });
                if !trench.boundary_3d.is_empty() {
                    ui.push_id(index,|ui|{
                        let mut rotation=trench.alignment.as_ref()
                            .map_or(0.0,|a|a.rotation_degrees);
                        if ui.add(egui::Slider::new(&mut rotation,-90.0..=90.0)
                            .text("Rotatie °")).changed()
                        {
                            match marxact_alignment::apply_rotation(trench,rotation) {
                                Ok(())=>{self.dirty=true;self.trench_markers.clear();},
                                Err(err)=>self.message=format!("Uitlijning mislukt: {err}"),
                            }
                        }
                        if ui.small_button("Automatisch uitlijnen").clicked() {
                            match marxact_alignment::recalculate(trench) {
                                Ok(())=>{self.dirty=true;self.trench_markers.clear();},
                                Err(err)=>self.message=format!("Uitlijning mislukt: {err}"),
                            }
                        }
                    });
                }
            }
            if let Some(index)=edit_virtual {self.open_virtual_editor(index);}
            ui.separator();
            ui.heading("Kaartopmerkingen");
            let mut remove_comment=None;
            for (i,note) in self.comments.iter().enumerate() {
                ui.horizontal(|ui|{
                    ui.label(format!("{} ({:.1}, {:.1})",note.text,note.x,note.y));
                    if ui.small_button("×").clicked(){remove_comment=Some(i);}
                });
            }
            if let Some(i)=remove_comment {self.comments.remove(i);self.dirty=true;}
            ui.separator();
            ui.heading("Kadaster WFS");
            if let Some(layer)=&mut self.cadastral {
                let path_count=layer.linework.iter().map(|l|l.paths.len()).sum::<usize>();
                if ui.checkbox(&mut layer.visible,
                    format!("{path_count} lijnen, {} labels",layer.labels.len())
                ).changed(){self.dirty=true;}
            }
            ui.separator();
            ui.heading("PDOK BGT-lagen");
            for layer in &mut self.bgt_layers {
                if ui.checkbox(&mut layer.visible,
                    format!("{} ({} paden)",layer.kind.label(),layer.paths.len())
                ).changed(){self.dirty=true;}
            }
            ui.separator();
            ui.heading("DXF-lagen");
            let mut choose_dxf=None;
            let mut replace_dxf=None;
            let selected_dxf=self.dxf_selection.as_ref().map(|s|(s.overlay,s.feature));
            egui::ScrollArea::vertical().max_height(150.0).show(ui,|ui|{
                for (overlay_index,overlay) in self.overlays.iter_mut().enumerate() {
                    let label=overlay.path.file_name().and_then(|s|s.to_str()).unwrap_or("DXF");
                    let changed=ui.checkbox(&mut overlay.visible,label).changed();
                    if changed {self.dirty=true;}
                    ui.small(format!("{} bronentiteiten, {} blokonderdelen",overlay.features.len(),overlay.expanded.features.len()));
                    for warning in overlay.warnings.iter().chain(&overlay.block_warnings){ui.colored_label(DARK,cad_preview(warning));}
                    ui.push_id(overlay_index,|ui|{
                        ui.horizontal(|ui|{
                            if ui.small_button("Opnieuw laden").clicked(){replace_dxf=Some((overlay_index,overlay.path.clone()));}
                            if ui.small_button("Bron kiezen...").clicked(){
                                if let Some(path)=rfd::FileDialog::new().add_filter("AutoCAD DXF",&["dxf"]).pick_file(){replace_dxf=Some((overlay_index,path));}
                            }
                        });
                        let mut layers_changed=false;
                        ui.collapsing("CAD-lagen",|ui|{
                            for (name,layer) in &mut overlay.layers {
                                if ui.checkbox(&mut layer.visible,cad_preview(name)).changed(){self.dirty=true;layers_changed=true;}
                            }
                        });
                        if layers_changed{overlay.refresh_block_visibility();}
                        ui.collapsing("Objecten bekijken",|ui|{
                            egui::ScrollArea::vertical().id_salt("objects").max_height(140.0)
                                .show_rows(ui,20.0,overlay.features.len()+overlay.expanded.features.len(),|ui,rows|{
                                    for index in rows {
                                        let feature=overlay.inspect_feature(index).unwrap();
                                        let label=format!("{} {} [{}]",feature.source.entity_type,cad_preview(&feature.source.handle),cad_preview(feature.layer));
                                        if ui.selectable_label(selected_dxf==Some((overlay_index,index)),label).clicked(){choose_dxf=Some((overlay_index,index));}
                                    }
                                });
                        });
                    });
                }
            });
            if let Some((overlay,feature))=choose_dxf {self.select_dxf(overlay,feature);}
            if let Some((overlay,path))=replace_dxf {self.replace_dxf_source(overlay,path);}
            if let Some(selection)=&self.dxf_selection {
                ui.collapsing("CAD-objectgegevens",|ui|{
                    egui::Grid::new("cad-details").striped(true).show(ui,|ui|{
                        for (key,value) in &selection.details {
                            ui.label(key);
                            ui.vertical(|ui|{
                                ui.label(cad_preview(value));
                                if cad_preview(value).len()<value.len() {
                                    ui.small(format!("Verkort getoond, {} bytes",value.len()));
                                }
                                if ui.small_button("Kopiëren").clicked(){ui.ctx().copy_text(value.clone());}
                            });
                            ui.end_row();
                        }
                    });
                });
            }
            ui.separator();
            if let Some(ds)=self.selected.and_then(|i|self.datasets.get(i)) {
                ui.strong(&ds.title);
                let summary=sleufbase_rust::summarize(ds,&self.migrated_settings.object_rules);
                ui.label(format!("Job-ID {}, {} objecten, {} disciplines",
                    ds.job_id,ds.features.len(),summary.distinct_count()));
                for (rule,count) in self.migrated_settings.object_rules.iter().zip(summary.counts.iter()) {
                    if *count>0 {ui.label(format!("{}: {}",rule.display_label(),count));}
                }
            }
            });
        });

        if self.file_intent.is_some(){self.show_file_loading(ctx);return;}
        if self.trench_markers.len()!=self.virtual_trenches.len() {
            self.trench_markers=self.virtual_trenches.iter().map(sleufbase_rust::cross_section::markers).collect();
        }
        egui::CentralPanel::default().show(ctx,|ui|{
            ui.heading("Kaart");
            ui.label("Zoom met scrollwiel, sleep om te verschuiven. GeoTIFF, DXF, PDOK, OpenStreetMap, BGT en Kadaster (preview).");
            let selected:&[Dataset]=self.selected.and_then(|i|self.datasets.get(i))
                .map(std::slice::from_ref).unwrap_or(&[]);
            canvas(ui,selected,&self.geotiffs,&self.raster_textures,
                &self.overlays,&self.virtual_trenches,&self.trench_markers,&self.bgt_layers,self.cadastral.as_ref(),&self.comments,
                if self.osm_visible && self.osm_texture.is_some() {self.osm_texture.as_ref()} else {self.pdok_texture.as_ref()},
                if self.osm_visible && self.osm_texture.is_some() {self.osm_bounds} else {self.pdok_bounds},
                self.osm_visible && self.osm_texture.is_some(),
                &mut self.zoom,&mut self.pan,&self.migrated_settings.object_rules);
        });

        if self.show_trench_dialog {
            let mut open=self.show_trench_dialog;
            let mut create=false;
            egui::Window::new("Nieuwe virtuele proefsleuf")
                .open(&mut open).resizable(false).show(ctx,|ui|{
                ui.label("Naam");
                ui.text_edit_singleline(&mut self.trench_form.label);
                ui.horizontal(|ui|{
                    ui.label("Start X");ui.add(egui::DragValue::new(&mut self.trench_form.start_x).speed(0.1));
                    ui.label("Start Y");ui.add(egui::DragValue::new(&mut self.trench_form.start_y).speed(0.1));
                });
                ui.horizontal(|ui|{
                    ui.label("Eind X");ui.add(egui::DragValue::new(&mut self.trench_form.end_x).speed(0.1));
                    ui.label("Eind Y");ui.add(egui::DragValue::new(&mut self.trench_form.end_y).speed(0.1));
                });
                ui.horizontal(|ui|{
                    ui.label("Breedte (m)");ui.add(egui::DragValue::new(&mut self.trench_form.width).speed(0.05));
                });
                if ui.button("Proefsleuf toevoegen").clicked(){create=true;}
            });
            self.show_trench_dialog=open;
            if create {
                match self.trench_form.make() {
                    Ok(t)=>{
                        self.virtual_trenches.push(t);
                        self.dirty=true;
                        self.show_trench_dialog=false;
                        self.message="Virtuele proefsleuf opgeslagen in Rust-project".into();
                    }
                    Err(err)=>self.message=format!("Virtuele proefsleuf ongeldig: {err}"),
                }
            }
        }


        if self.trench_editor.is_some() {self.show_virtual_editor(ctx);}
        if self.browser_ui.open {self.show_kickthemap_workspace(ctx);}

        if self.show_comment_dialog {
            let mut opened=self.show_comment_dialog;
            let mut submit=false;
            egui::Window::new("Kaartopmerking").open(&mut opened)
                .resizable(false).show(ctx,|ui|{
                ui.horizontal(|ui|{
                    ui.label("RD X");
                    ui.add(egui::DragValue::new(&mut self.comment_form.x).speed(0.1));
                    ui.label("RD Y");
                    ui.add(egui::DragValue::new(&mut self.comment_form.y).speed(0.1));
                });
                ui.label("Opmerking");
                ui.text_edit_singleline(&mut self.comment_form.text);
                if ui.button("Toevoegen").clicked(){submit=true;}
            });
            self.show_comment_dialog=opened;
            if submit {
                if self.comment_form.x.is_finite()&&self.comment_form.y.is_finite()&&
                    !self.comment_form.text.trim().is_empty()
                {
                    self.comments.push(Comment {
                        x:self.comment_form.x,y:self.comment_form.y,
                        text:self.comment_form.text.trim().to_owned()
                    });
                    self.dirty=true;
                    self.show_comment_dialog=false;
                    self.comment_form.text.clear();
                } else {self.message="Geef geldige RD-coördinaten en tekst op".into();}
            }
        }


        if self.show_settings_dialog {
            let mut show=self.show_settings_dialog;
            let mut save=false;
            egui::Window::new("SleufBase Rust-instellingen")
                .open(&mut show).resizable(false).show(ctx,|ui|{
                ui.strong("SleufBase-instellingen");
                if ui.button("Objectregels en disciplinekleuren").clicked() {
                    self.rule_editor=Some(crate::rule_editor::Editor::new(&self.migrated_settings.object_rules));
                }
                ui.horizontal(|ui|{
                    ui.label("DXF-labelafstand (m)");
                    ui.add(egui::DragValue::new(&mut self.migrated_settings.dxf_label_gap).range(0.0..=100.).speed(0.1));
                });
                ui.checkbox(&mut self.migrated_settings.template_auto_fill_bgt_fysiek_voorkomen,
                    "BGT fysiek voorkomen automatisch aanvullen (voorbereid)");
                ui.checkbox(&mut self.migrated_settings.template_auto_export_discipline_excel,
                    "Automatisch discipline-Excel naast sjabloon-DXF");
                ui.small("Het overzicht volgt dezelfde PS-namen en volgorde. Bij vervangen blijft de vorige Excel-versie in de exportmap bewaard.");
                ui.separator();
                ui.checkbox(&mut self.backup_settings.enabled,"Automatische projectback-ups");
                ui.horizontal(|ui|{
                    ui.label("Interval (minuten)");
                    ui.add(egui::DragValue::new(&mut self.backup_settings.interval_minutes)
                        .range(1..=1440));
                });
                ui.horizontal(|ui|{
                    ui.label("Maximaal aantal back-ups");
                    ui.add(egui::DragValue::new(&mut self.backup_settings.max_backups)
                        .range(1..=200));
                });
                ui.label(format!("{} extra materiaalkeuzes",self.migrated_settings.kickthemap_material_choices.len()));
                if ui.button("Instellingen opslaan").clicked() {save=true;}
            });
            self.show_settings_dialog=show;
            if save {
                let a=settings_compat::save_rust_settings(
                    &project::app_data_directory().join("settings.json"),
                    &self.migrated_settings);
                let b=project::write_autosave_settings(
                    &project::app_data_directory().join("autosave-settings.json"),
                    &self.backup_settings);
                self.message=if a.is_ok()&&b.is_ok() {
                    "Rust-instellingen opgeslagen".into()
                } else {
                    format!("Instellingen niet volledig opgeslagen: {a:?}; {b:?}")
                };
                self.show_settings_dialog=false;
            }
        }

        if let Some(rules)=crate::rule_editor::show(ctx,&mut self.rule_editor) {
            let mut config=self.migrated_settings.clone();
            config.object_rules=rules;
            match settings_compat::save_rust_settings(&project::app_data_directory().join("settings.json"),&config) {
                Ok(())=>{
                    self.migrated_settings=config;
                    self.rule_editor=None;
                    self.message="Objectregels opgeslagen en toegepast op kaart, tellingen en exports".into();
                },
                Err(error)=>{if let Some(editor)=self.rule_editor.as_mut(){editor.error=error;}}
            }
        }
        if let Some(action)=crate::export_dialog::show(ctx,&mut self.export_dialog,&self.datasets,&self.virtual_trenches,self.project_generation) {
            if let Some(path)=rfd::FileDialog::new().add_filter("DXF",&["dxf"]).set_file_name("SleufBase-Objecten-Rust.dxf").save_file() {
                self.finish_dxf_export(path,action);
            }
        }
        self.show_unsaved_project_dialog(ctx);
        if let Some(state)=crate::template_dialog::show(ctx,&mut self.template_order_dialog,&self.geotiffs,&self.datasets,self.project_generation){
            if let Some(path)=rfd::FileDialog::new().add_filter("DXF",&["dxf"]).set_file_name(if state.reverse{"SleufBase-Proefsleuven-omgekeerd.dxf"}else{"SleufBase-Proefsleuven.dxf"}).save_file(){self.finish_template_export(path,state);}
        }

    }
}

#[cfg(test)]
mod project_lifecycle_tests {
    use super::*;
    fn finish_loading(app:&mut SleufBaseApp,ctx:&egui::Context){
        let deadline=Instant::now()+Duration::from_secs(15);
        while app.file_intent.is_some(){
            app.poll_file_jobs(ctx);
            assert!(Instant::now()<deadline,"file loading did not finish");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn edited_app()->SleufBaseApp {
        let mut app=SleufBaseApp::default();
        app.comments.push(Comment{x:168000.0,y:401000.0,text:"unsaved".into()});
        app.dirty=true;
        app
    }

    #[test]
    fn new_and_close_require_a_decision_and_cancel_keeps_work() {
        let ctx=egui::Context::default(); let mut app=edited_app();
        for action in [ProjectAction::New,ProjectAction::Close] {
            app.request_project_action(action.clone(),&ctx);
            assert_eq!(app.pending_project_action,Some(action));
            assert_eq!(app.comments.len(),1); assert!(app.dirty);
            app.finish_project_action(false,&ctx);
            assert!(app.pending_project_action.is_none());
            assert_eq!(app.comments[0].text,"unsaved"); assert!(!app.close_confirmed);
        }
        app.request_project_action(ProjectAction::New,&ctx);
        app.finish_project_action(true,&ctx);
        assert!(app.comments.is_empty()); assert!(!app.dirty);
    }

    #[test]
    fn failed_open_or_save_keeps_current_work_and_pending_action() {
        let ctx=egui::Context::default(); let mut app=edited_app();
        let dir=tempfile::tempdir().unwrap();
        app.request_project_action(ProjectAction::Open(dir.path().join("missing.json")),&ctx);
        assert!(!app.save_project_to_path(dir.path().to_owned()));
        assert!(app.pending_project_action.is_some()); assert!(app.dirty);
        assert_eq!(app.comments[0].text,"unsaved");
        app.finish_project_action(true,&ctx);
        assert_eq!(app.comments[0].text,"unsaved"); assert!(app.dirty);
        finish_loading(&mut app,&ctx);
        assert_eq!(app.comments[0].text,"unsaved");assert!(app.dirty);
        assert!(app.message.contains("huidige gegevens behouden"));
        assert!(app.project_path.is_none());
    }

    #[test]
    fn save_then_switch_and_restore_as_new_never_retargets_the_backup() {
        let ctx=egui::Context::default(); let mut app=edited_app();
        let dir=tempfile::tempdir().unwrap();let saved=dir.path().join("original.json");
        app.request_project_action(ProjectAction::New,&ctx);
        assert!(app.save_project_to_path(saved.clone()));
        app.finish_project_action(true,&ctx);
        assert!(app.comments.is_empty());
        app.request_project_action(ProjectAction::Restore(saved.clone()),&ctx);
        finish_loading(&mut app,&ctx);
        assert_eq!(app.comments[0].text,"unsaved"); assert!(app.dirty);
        assert!(app.project_path.is_none());
        let original=std::fs::read(&saved).unwrap();
        let copy=dir.path().join("recovered.json");
        assert!(app.save_project_to_path(copy.clone()));
        assert_eq!(app.project_path,Some(copy));
        assert_eq!(std::fs::read(saved).unwrap(),original);
    }

    #[test]
    fn project_switch_detaches_all_old_map_requests() {
        let ctx=egui::Context::default(); let mut app=SleufBaseApp::default();
        let (btx,brx)=mpsc::channel(); app.bgt_rx=Some(brx);app.bgt_busy=true;
        let (ctx_send,crx)=mpsc::channel();app.cadastral_rx=Some(crx);app.cadastral_busy=true;
        let (atx,arx)=mpsc::channel();app.ahn_rx=Some(arx);app.ahn_busy=true;
        let (ptx,prx)=mpsc::channel();app.pdok_rx=Some(prx);app.pdok_busy=true;
        let (otx,orx)=mpsc::channel();app.osm_rx=Some(orx);app.osm_busy=true;
        let old_generation=app.project_generation;
        app.execute_project_action(ProjectAction::New,&ctx);
        assert_ne!(app.project_generation,old_generation);
        assert!(!app.bgt_busy&&!app.cadastral_busy&&!app.ahn_busy&&!app.pdok_busy&&!app.osm_busy);
        assert!(btx.send(Err("late".into())).is_err());
        assert!(ctx_send.send(Err("late".into())).is_err());
        assert!(atx.send(Err("late".into())).is_err());
        assert!(ptx.send(Err("late".into())).is_err());
        assert!(otx.send(Err("late".into())).is_err());
        app.poll_bgt();app.poll_cadastral();app.poll_ahn();app.poll_pdok(&ctx);app.poll_osm(&ctx);
        assert!(app.bgt_layers.is_empty()); assert!(app.cadastral.is_none()); assert!(!app.dirty);
    }
    #[test]
    fn late_download_stays_on_disk_without_entering_a_new_project() {
        let ctx=egui::Context::default(); let mut app=SleufBaseApp::default();
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("PS1_123_jobFeatures.json");
        std::fs::write(&path,include_str!("../tests/fixtures/jobFeatures.json")).unwrap();
        app.live_download_generation=app.project_generation;
        app.execute_project_action(ProjectAction::New,&ctx);
        app.apply_live_download_result(Ok((RemoteFileKind::JobFeatures,path.clone())),&ctx);
        assert!(app.datasets.is_empty());assert!(!app.dirty);assert!(path.exists());
        app.live_download_generation=app.project_generation;
        app.apply_live_download_result(Ok((RemoteFileKind::JobFeatures,path)),&ctx);
        assert!(app.datasets.is_empty());finish_loading(&mut app,&ctx);
        assert_eq!(app.datasets.len(),1); assert!(app.dirty);
    }

    #[test]
    fn python_import_is_guarded_and_snapshot_retains_original_document() {
        let ctx=egui::Context::default();let mut app=edited_app();
        let dir=tempfile::tempdir().unwrap();let source=dir.path().join("PS42.klicproject");
        std::fs::write(&source,include_str!("../tests/fixtures/python-v1.klicproject")).unwrap();
        app.request_project_action(ProjectAction::ImportPython(source.clone()),&ctx);
        assert!(app.legacy_archive.is_none());assert_eq!(app.comments[0].text,"unsaved");
        app.finish_project_action(true,&ctx);
        assert!(app.dirty);assert!(app.project_path.is_none());
        assert!(app.legacy_archive.is_none());finish_loading(&mut app,&ctx);
        let doc=app.snapshot();
        assert_eq!(doc.legacy_archive.unwrap().original["project_name"],"PS42");
        assert!(app.active_job_ids().contains(&123));
        app.finish_project_action(false,&ctx);
        assert_eq!(std::fs::read_to_string(source).unwrap(),include_str!("../tests/fixtures/python-v1.klicproject"));
    }

    #[test]
    fn signed_dxf_colour_indices_never_overflow() {
        for colour in 1..=255 {assert_eq!(aci_color(colour),aci_color(-colour));}
        let _=aci_color(i32::MIN);
    }

    fn finish_exporting(app:&mut SleufBaseApp,ctx:&egui::Context) {
        let deadline=Instant::now()+Duration::from_secs(15);
        while app.export_intent.is_some(){app.poll_export_jobs(ctx);assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(2));}
    }

    #[test]
    fn selected_export_preserves_inputs_and_loads_the_actual_published_drawing_as_an_overlay() {
        let mut app=SleufBaseApp::default();
        let json=r#"{"features":[{"geometry":{"type":"Point","coordinates":[1,2]},"properties":{"idName":"water"}}]}"#;
        app.datasets=vec![sleufbase_rust::parse_job_features(json,1,"PS1").unwrap(),sleufbase_rust::parse_job_features(json,2,"PS2").unwrap()];
        let before=serde_json::to_value(&app.datasets).unwrap();
        app.save_dxf();app.export_dialog.as_mut().unwrap().selection.datasets[0]=false;
        let dir=tempfile::tempdir().unwrap();let output=dir.path().join("selected.dxf");
        app.finish_dxf_export(output.clone(),crate::export_dialog::Action{overlay:true});
        finish_exporting(&mut app,&egui::Context::default());
        assert!(app.export_dialog.is_none());assert!(output.exists());
        finish_loading(&mut app,&egui::Context::default());
        assert_eq!(app.overlays.len(),1);assert_eq!(app.overlays[0].path,output);
        let text=std::fs::read_to_string(&output).unwrap();
        assert!(text.contains("JobId: 2"));assert!(!text.contains("JobId: 1"));
        assert_eq!(serde_json::to_value(&app.datasets).unwrap(),before);
        let bytes=std::fs::read(&output).unwrap();
        app.save_dxf();app.finish_dxf_export(output.clone(),crate::export_dialog::Action{overlay:false});
        finish_exporting(&mut app,&egui::Context::default());
        assert!(app.export_dialog.as_ref().unwrap().error.contains("bestaat al"));
        assert_eq!(std::fs::read(output).unwrap(),bytes);
    }

    #[test]
    fn canvas_uses_configured_rule_colors_instead_of_default_discipline_labels() {
        let dataset=sleufbase_rust::parse_job_features(r#"{"features":[{"geometry":{"type":"Point","coordinates":[1,2]},"properties":{"idName":"water"}}]}"#,1,"PS1").unwrap();
        let custom=[sleufbase_rust::Rule::new(vec!["water".into()],"CUSTOM".into(),6,"Andere naam".into())];
        let ctx=egui::Context::default();let mut zoom=1.;let mut pan=egui::Vec2::ZERO;
        let mut render=|rules:&[sleufbase_rust::Rule]|ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(Pos2::ZERO,egui::vec2(800.,600.))),..Default::default()},|ctx|{
            egui::CentralPanel::default().show(ctx,|ui|{canvas(ui,std::slice::from_ref(&dataset),&[],&[],&[],&[],&[],&[],None,&[],None,None,false,&mut zoom,&mut pan,rules);});
        });
        let has_magenta=|output:egui::FullOutput|output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Circle(c)if c.fill==aci_color(6)));
        assert!(has_magenta(render(&custom)));
        assert!(!has_magenta(render(RULES)));
        assert!(!has_magenta(render(&[])));
    }

    #[test]
    fn cad_inspector_preserves_attributes_and_resets_on_project_switch() {
        let ctx=egui::Context::default();let mut app=SleufBaseApp::default();
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cad-properties/properties-binary.dxf");
        app.overlays.push(load_dxf(&path).unwrap());
        app.select_dxf(0,app.overlays[0].features.len()-1);
        let details=&app.dxf_selection.as_ref().unwrap().details;
        assert!(details.iter().any(|(key,value)|key=="Attribuut Code"&&value=="PS 42"));
        assert!(details.iter().any(|(key,value)|key=="XDATA MEASUREMENTS"&&value.contains("ID-42")));
        assert!(!app.dirty);
        app.execute_project_action(ProjectAction::New,&ctx);
        assert!(app.dxf_selection.is_none());
    }

    #[test]
    fn cad_source_refresh_hydrates_old_overlays_and_a_failed_relink_preserves_current_data(){
        let mut app=SleufBaseApp::default();
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cad-scene/scene-ascii.dxf");
        let mut old=load_dxf(&path).unwrap();old.blocks.clear();old.expanded=Default::default();old.visible=false;
        let mut root=old.layers.remove("ROOT").unwrap();root.visible=false;old.layers.insert("rOoT".into(),root);
        app.overlays.push(old);app.select_dxf(0,0);
        app.replace_dxf_source(0,path);
        finish_loading(&mut app,&egui::Context::default());
        assert!(!app.overlays[0].blocks.is_empty());assert_eq!(app.overlays[0].expanded.features.len(),82);
        assert!(!app.overlays[0].visible);assert!(!app.overlays[0].layers["ROOT"].visible);
        assert!(app.dirty);assert!(app.dxf_selection.is_none());
        let before=serde_json::to_value(&app.overlays[0]).unwrap();let cache=app.overlays[0].expanded.clone();
        app.dirty=false;app.replace_dxf_source(0,"does-not-exist.dxf".into());
        finish_loading(&mut app,&egui::Context::default());
        assert_eq!(serde_json::to_value(&app.overlays[0]).unwrap(),before);
        assert!(std::sync::Arc::ptr_eq(&cache,&app.overlays[0].expanded));assert!(!app.dirty);
        assert!(app.message.contains("huidige gegevens behouden"));
    }

    #[test]
    fn canvas_uses_true_color_and_honors_cad_layer_visibility() {
        let ctx=egui::Context::default();let mut zoom=1.0;let mut pan=egui::Vec2::ZERO;
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cad-properties/properties-binary.dxf");
        let mut overlay=load_dxf(&path).unwrap();
        // One entity in true-color layer True, so toggling that layer must
        // remove its actual painted stroke. Other appearance cases are covered
        // by the Python-oracle integration tests.
        overlay.features.retain(|f|f.layer=="True");
        overlay.rebuild_block_cache().unwrap();
        let mut render=|overlay:&DxfOverlay| {
            ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(Pos2::ZERO,egui::vec2(800.0,600.0))),..Default::default()},|ctx|{
                egui::CentralPanel::default().show(ctx,|ui|{
                    canvas(ui,&[],&[],&[],std::slice::from_ref(overlay),&[],&[],&[],None,&[],None,None,false,&mut zoom,&mut pan,RULES);
                });
            })
        };
        let has_color=|output:egui::FullOutput|output.shapes.iter().any(|clipped|
            matches!(&clipped.shape,egui::Shape::LineSegment{stroke,..}if stroke.color==Color32::from_rgb(0x12,0x34,0x56))
        );
        assert!(has_color(render(&overlay)));
        overlay.layers.get_mut("True").unwrap().visible=false;
        overlay.refresh_block_visibility();
        assert!(!has_color(render(&overlay)));
    }

    #[test]
    fn canvas_never_paints_a_bridge_between_disconnected_spline_paths() {
        let ctx=egui::Context::default();let mut zoom=1.;let mut pan=egui::Vec2::ZERO;
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/splines/splines-binary.dxf");
        let mut overlay=load_dxf(&path).unwrap();
        overlay.features.retain(|f|f.cad.as_ref().is_some_and(|p|p.xdata.get("SOURCE_ID").is_some_and(|tags|tags[0].value=="disconnected")));
        overlay.blocks.clear();overlay.rebuild_block_cache().unwrap();
        assert_eq!(overlay.features.len(),1);
        let output=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(Pos2::ZERO,egui::vec2(800.,600.))),..Default::default()},|ctx|{
            egui::CentralPanel::default().show(ctx,|ui|{
                canvas(ui,&[],&[],&[],std::slice::from_ref(&overlay),&[],&[],&[],None,&[],None,None,false,&mut zoom,&mut pan,RULES);
            });
        });
        let edges:Vec<_>=output.shapes.iter().filter_map(|s|match &s.shape {
            egui::Shape::LineSegment{points,stroke}if stroke.color==Color32::from_rgb(220,20,60)=>Some(points),_=>None
        }).collect();
        assert_eq!(edges.len(),overlay.features[0].points.len()-2);
        // The actual gap spans over half the fitted viewport. Every real
        // curve edge is short; the renderer must never paint that long bridge.
        assert!(edges.iter().all(|edge|(edge[1].x-edge[0].x).abs()<400.));
    }

    #[test]
    fn canvas_draws_only_the_two_visible_edges_of_a_partly_hidden_quad(){
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/meshes/meshes-binary.dxf");
        let mut overlay=load_dxf(&path).unwrap();
        overlay.features.retain(|f|f.cad.as_ref().is_some_and(|p|p.xdata.get("SOURCE_ID").is_some_and(|t|t[0].value=="quad-5")));
        overlay.blocks.clear();
        overlay.features[0].cad.as_mut().unwrap().true_color=Some(0x12ab34);
        overlay.rebuild_block_cache().unwrap();
        let ctx=egui::Context::default();let mut zoom=1.;let mut pan=egui::Vec2::ZERO;
        let output=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(Pos2::ZERO,egui::vec2(600.,400.))),..Default::default()},|ctx|{
            egui::CentralPanel::default().show(ctx,|ui|{canvas(ui,&[],&[],&[],std::slice::from_ref(&overlay),&[],&[],&[],None,&[],None,None,false,&mut zoom,&mut pan,RULES);});
        });
        let edges=output.shapes.iter().filter(|s|matches!(&s.shape,egui::Shape::LineSegment{stroke,..}if stroke.color==Color32::from_rgb(0x12,0xab,0x34))).count();
        assert_eq!(edges,2,"hidden edges and gaps must not be painted");
    }

    #[test]
    fn raster_pixels_load_asynchronously_and_stay_out_of_new_projects() {
        let ctx=egui::Context::default();let mut app=SleufBaseApp::default();
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters");
        app.load_geotiffs(vec![root.join("gray16.tiff")],&ctx);
        finish_loading(&mut app,&ctx);
        assert_eq!(app.geotiffs.len(),1);assert!(app.raster_textures[0].is_none());
        let deadline=Instant::now()+Duration::from_secs(10);
        while app.raster_textures[0].is_none() {
            app.poll_raster_previews(&ctx);
            assert!(app.raster_errors[0].is_none(),"{:?}",app.raster_errors[0]);
            assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(app.raster_textures[0].as_ref().unwrap().size(),[7,1]);
        app.load_geotiffs(vec![root.join("large-striped.tiff")],&ctx);
        app.execute_project_action(ProjectAction::New,&ctx);
        app.poll_raster_previews(&ctx);
        assert!(app.geotiffs.is_empty());assert!(app.raster_textures.is_empty());assert!(app.raster_errors.is_empty());
    }

}
