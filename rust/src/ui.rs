//! Native Rust desktop UI, no Python runtime or IPC dependency.
//! Native project format is deliberately separate from legacy Python projects.

use eframe::egui::{self, Color32, Pos2, Stroke};
use sleufbase_rust::{
    load_job_features, write_dxf, write_xlsx, Bounds, Dataset, Geometry,
    Vertex, RULES
};
use sleufbase_rust::dxf_read::{load_dxf, DxfOverlay};
use sleufbase_rust::virtual_trench::VirtualTrench;
use sleufbase_rust::pdok_wms::{self, PdokRaster};
use sleufbase_rust::bgt::{self,BgtKind,BgtLayer};
use sleufbase_rust::ahn;
use sleufbase_rust::jobs_cache::{read_cache,JobsCache,Job};
use sleufbase_rust::kickthemap_live::{KickTheMapLiveClient,RemoteFileKind};
use sleufbase_rust::settings_compat::{self,MigratedSettings};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use sleufbase_rust::geotiff::{read_geotiff_metadata, GeoTiffInfo};
use sleufbase_rust::loaded_jobs::{kickthemap_cache_dir, read_active_loaded_job_ids};
use sleufbase_rust::project::{
    self, AutosaveSettings, NativeProject, Comment, read_project, save_project
};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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


pub struct SleufBaseApp {
    datasets: Vec<Dataset>,
    geotiffs: Vec<GeoTiffInfo>,
    raster_textures: Vec<Option<egui::TextureHandle>>,
    overlays: Vec<DxfOverlay>,
    virtual_trenches: Vec<VirtualTrench>,
    show_trench_dialog: bool,
    trench_form:TrenchForm,
    pdok_texture:Option<egui::TextureHandle>,
    pdok_bounds:Option<Bounds>,
    pdok_rx:Option<Receiver<Result<PdokRaster,String>>>,
    pdok_busy:bool,
    bgt_layers:Vec<BgtLayer>,
    bgt_rx:Option<Receiver<Result<BgtLayer,String>>>,
    bgt_busy:bool,
    ahn_rx:Option<Receiver<Result<Option<f64>,String>>>,
    ahn_busy:bool,
    jobs_cache:Option<JobsCache>,
    live_email:String,
    live_password:String,
    live_session:Option<KickTheMapLiveClient>,
    live_jobs:Vec<Job>,
    live_rx:Option<Receiver<Result<(KickTheMapLiveClient,Vec<Job>),String>>>,
    live_download_rx:Option<Receiver<(KickTheMapLiveClient,Result<(RemoteFileKind,PathBuf),String>)>>,
    live_busy:bool,
    jobs_query:String,
    comments:Vec<Comment>,
    show_comment_dialog:bool,
    show_settings_dialog:bool,
    migrated_settings:MigratedSettings,
    comment_form:CommentForm,
    only_active_cached:bool,
    active_only: bool,
    selected: Option<usize>,
    query: String,
    message: String,
    project_path: Option<PathBuf>,
    project_name: String,
    dirty: bool,
    backup_settings: AutosaveSettings,
    last_backup: Instant,
    zoom: f64,
    pan: egui::Vec2,
}

impl Default for SleufBaseApp {
    fn default() -> Self {
        Self {
            datasets:Vec::new(),
            geotiffs:Vec::new(),
            raster_textures:Vec::new(),
            overlays:Vec::new(),
            virtual_trenches:Vec::new(),
            show_trench_dialog:false,
            trench_form:TrenchForm::default(),
            pdok_texture:None,
            pdok_bounds:None,
            pdok_rx:None,
            pdok_busy:false,
            bgt_layers:Vec::new(),
            bgt_rx:None,
            bgt_busy:false,
            ahn_rx:None,
            ahn_busy:false,
            jobs_cache:None,
            live_email:String::new(),
            live_password:String::new(),
            live_session:None,
            live_jobs:Vec::new(),
            live_rx:None,
            live_download_rx:None,
            live_busy:false,
            jobs_query:String::new(),
            comments:Vec::new(),
            show_comment_dialog:false,
            show_settings_dialog:false,
            migrated_settings:settings_compat::load_rust_settings(
                &project::app_data_directory().join("settings.json")),
            comment_form:CommentForm::default(),
            only_active_cached:false,
            active_only:false,
            selected:None,
            query:String::new(),
            message:String::new(),
            project_path:None,
            project_name:"Naamloos project".into(),
            dirty:false,
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
            datasets:self.datasets.clone(),
            rasters:self.geotiffs.clone(),
            overlays:self.overlays.clone(),
            bgt_layers:self.bgt_layers.clone(),
            virtual_trenches:self.virtual_trenches.clone(),
            comments:self.comments.clone(),
            ..NativeProject::default()
        }
    }

    fn new_project(&mut self) {
        self.datasets.clear();
        self.geotiffs.clear();
        self.raster_textures.clear();
        self.overlays.clear();
        self.bgt_layers.clear();
        self.bgt_rx=None;
        self.bgt_busy=false;
        self.virtual_trenches.clear();
        self.comments.clear();
        self.pdok_texture=None;
        self.pdok_bounds=None;
        self.pdok_rx=None;
        self.pdok_busy=false;
        self.selected=None;
        self.project_path=None;
        self.project_name="Naamloos project".into();
        self.dirty=false;
        self.zoom=1.0;
        self.pan=egui::Vec2::ZERO;
        self.message="Nieuw leeg Rust-project. De vorige gegevens zijn uit het huidige venster verwijderd.".into();
    }

    fn load_project_file(&mut self, path: PathBuf, ctx: &egui::Context) {
        match read_project(&path) {
            Ok(doc) => {
                let textures=doc.rasters.iter().map(|r|load_texture(&r.path,ctx)).collect();
                self.datasets=doc.datasets;
                self.geotiffs=doc.rasters;
                self.raster_textures=textures;
                self.overlays=doc.overlays;
                self.bgt_layers=doc.bgt_layers;
                self.virtual_trenches=doc.virtual_trenches;
                self.comments=doc.comments;
                self.pdok_texture=None;
                self.pdok_bounds=None;
                self.pdok_rx=None;
                self.pdok_busy=false;
                self.project_name=doc.name;
                self.selected=(!self.datasets.is_empty()).then_some(0);
                self.project_path=Some(path.clone());
                self.zoom=1.0;
                self.pan=egui::Vec2::ZERO;
                self.last_backup=Instant::now();
                self.dirty=false;
                self.message=format!("Rust-project geopend: {}",path.display());
            }
            Err(err) => self.message=format!("Project niet geladen (origineel ongewijzigd): {err}"),
        }
    }


    fn restore_backup(&mut self,ctx:&egui::Context) {
        if self.dirty {
            self.message="Sla eerst je onopgeslagen projectwijzigingen op voordat je een back-up herstelt".into();
            return;
        }
        let dir=project::backup_directory();
        if !dir.is_dir() {
            self.message="Geen SleufBase Rust-back-ups gevonden".into();return;
        }
        if let Some(path)=rfd::FileDialog::new()
            .set_directory(&dir).add_filter("SleufBase backups",&["json"]).pick_file()
        {
            let allowed=path.canonicalize().ok().zip(dir.canonicalize().ok())
                .is_some_and(|(file,dir)| file.starts_with(dir));
            if !allowed {
                self.message="Back-up ligt niet in de beveiligde Rust-back-upmap".into();
                return;
            }
            match read_project(&path) {
                Ok(project_data)=>{
                    let rasters=project_data.rasters.iter()
                        .map(|r|load_texture(&r.path,ctx)).collect();
                    self.datasets=project_data.datasets;
                    self.geotiffs=project_data.rasters;
                    self.overlays=project_data.overlays;
                    self.bgt_layers=project_data.bgt_layers;
                    self.virtual_trenches=project_data.virtual_trenches;
                    self.comments=project_data.comments;
                    self.project_name=project_data.name;
                    self.raster_textures=rasters;
                    self.selected=(!self.datasets.is_empty()).then_some(0);
                    // Never overwrite the source backup when pressing Save.
                    self.project_path=None;
                    self.dirty=true;
                    self.last_backup=Instant::now();
                    self.message=format!(
                        "Back-up hersteld uit {}. Kies Opslaan als om het als nieuw project te bewaren.",
                        path.display());
                }
                Err(err)=>self.message=format!("Back-up ongeldig: {err}")
            }
        }
    }

    fn save_project_file(&mut self, save_as: bool) {
        let path = if !save_as {
            self.project_path.clone()
        } else { None };
        let path=path.or_else(||rfd::FileDialog::new()
            .add_filter("SleufBase Rust Project", &["json"])
            .set_file_name("Project.sleufbase-rust.json")
            .save_file());
        if let Some(path)=path {
            match save_project(&path,&self.snapshot()) {
                Ok(())=>{
                    self.project_path=Some(path.clone());
                    self.dirty=false;
                    self.message=format!("Rust-project opgeslagen: {}",path.display());
                }
                Err(err)=>self.message=format!("Project opslaan mislukt: {err}"),
            }
        }
    }

    fn try_autosave(&mut self) {
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

    fn load_files(&mut self, paths:Vec<PathBuf>) {
        let mut added=0;
        let mut failures=Vec::new();
        for path in paths {
            if self.datasets.iter().any(|ds| ds.source_path==path) {continue;}
            match load_job_features(&path) {
                Ok(dataset)=>{
                    self.datasets.push(dataset);
                    self.selected=Some(self.datasets.len()-1);
                    added+=1;
                }
                Err(err)=>failures.push(format!("{}: {err}",path.display()))
            }
        }
        if added>0 {self.dirty=true;}
        self.message = format!("{added} KickTheMap-job(s) geladen. {}",failures.join("; "));
    }

    fn load_geotiffs(&mut self, paths:Vec<PathBuf>, ctx:&egui::Context) {
        let mut added=0;
        let mut warnings=Vec::new();
        for path in paths {
            if self.geotiffs.iter().any(|info|info.path==path) {continue;}
            match read_geotiff_metadata(&path) {
                Ok(info)=>{
                    let texture=load_texture(&path,ctx);
                    if texture.is_none() {
                        warnings.push(format!("{}: geen 8-bit rasterpreview",path.display()));
                    }
                    self.geotiffs.push(info);
                    self.raster_textures.push(texture);
                    added+=1;
                }
                Err(err)=>warnings.push(format!("{}: {err}",path.display()))
            }
        }
        if added>0 {self.dirty=true;}
        self.message=format!("{added} GeoTIFF-bestanden geladen. {}",warnings.join("; "));
    }

    fn load_overlays(&mut self, paths:Vec<PathBuf>) {
        let mut added=0;
        let mut failures=Vec::new();
        for path in paths {
            if self.overlays.iter().any(|overlay|overlay.path==path) {continue;}
            match load_dxf(&path) {
                Ok(overlay)=>{self.overlays.push(overlay);added+=1;}
                Err(err)=>failures.push(format!("{}: {err}",path.display()))
            }
        }
        if added>0 {self.dirty=true;}
        self.message=format!("{added} DXF-overlays geladen. {}",failures.join("; "));
    }


    fn current_bounds(&self)->Option<Bounds> {
        let mut bounds=Bounds::from_datasets(&self.datasets);
        for raster in &self.geotiffs {bounds=union_bounds(bounds,Some(raster.bounds));}
        for overlay in self.overlays.iter().filter(|o|o.visible) {
            bounds=union_bounds(bounds,overlay.bounds());
        }
        for t in &self.virtual_trenches {
            bounds=union_bounds(bounds,Some(Bounds{
                min_x:t.start.x.min(t.end.x)-t.width_meters,
                min_y:t.start.y.min(t.end.y)-t.width_meters,
                max_x:t.start.x.max(t.end.x)+t.width_meters,
                max_y:t.start.y.max(t.end.y)+t.width_meters
            }));
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
        self.live_download_rx=Some(rx);
        std::thread::spawn(move || {
            let root=kickthemap_cache_dir().join("Rust");
            let result=client.download_job(&job,kind,&root).map(|p|(kind,p));
            let _=tx.send((client,result));
        });
    }

    fn poll_live_download(&mut self,ctx:&egui::Context) {
        let event=self.live_download_rx.as_ref().map(|rx|rx.try_recv());
        match event {
            Some(Ok((client,result)))=>{
                self.live_session=Some(client);
                self.live_download_rx=None;
                self.live_busy=false;
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
        if self.live_busy {
            self.message="Rond het lopende verzoek af voordat je uitlogt".into();
            return;
        }
        self.live_session=None;
        self.live_jobs.clear();
        self.live_password.clear();
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
        let now=SystemTime::now().duration_since(UNIX_EPOCH)
            .map(|d|d.as_secs_f64()).unwrap_or(0.0);
        read_active_loaded_job_ids(
            &kickthemap_cache_dir().join("active_loaded_jobs.json"),now
        )
    }

    fn export_datasets(&self)->Vec<Dataset> {
        let mut all=self.datasets.clone();
        all.extend(self.virtual_trenches.iter().map(|t|t.to_dataset(0,true)));
        all
    }

    fn save_excel(&mut self) {
        if self.datasets.is_empty() && self.virtual_trenches.is_empty() {
            self.message="Laad eerst jobFeatures.json of maak een virtuele proefsleuf".into();
            return;
        }
        if let Some(path)=rfd::FileDialog::new()
            .add_filter("Excel",&["xlsx"])
            .set_file_name("SleufBase-Disciplines-Rust.xlsx").save_file()
        {
            self.message=match write_xlsx(&path,&self.export_datasets(),RULES) {
                Ok(())=>format!("Excel opgeslagen: {}",path.display()),
                Err(err)=>format!("Excel-export mislukt: {err}"),
            };
        }
    }

    fn save_dxf(&mut self) {
        if self.datasets.is_empty() && self.virtual_trenches.is_empty() {
            self.message="Laad eerst jobFeatures.json of maak een virtuele proefsleuf".into();
            return;
        }
        if let Some(path)=rfd::FileDialog::new()
            .add_filter("DXF",&["dxf"])
            .set_file_name("SleufBase-Objecten-Rust.dxf").save_file()
        {
            self.message=match write_dxf(&path,&self.export_datasets(),RULES) {
                Ok(())=>format!("DXF opgeslagen: {}",path.display()),
                Err(err)=>format!("DXF-export mislukt: {err}"),
            };
        }
    }
}

fn load_texture(path:&std::path::Path,ctx:&egui::Context) -> Option<egui::TextureHandle> {
    let Ok((w,h,rgba))=sleufbase_rust::raster::decode_tiff_preview(path) else{return None;};
    let image=egui::ColorImage::from_rgba_unmultiplied([w as usize,h as usize],&rgba);
    Some(ctx.load_texture(
        path.to_string_lossy(),image,egui::TextureOptions::LINEAR
    ))
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
    match color.abs() {
        1=>Color32::from_rgb(220,45,45),
        2=>Color32::from_rgb(225,190,20),
        3=>Color32::from_rgb(40,150,40),
        4=>Color32::from_rgb(40,190,195),
        5=>Color32::from_rgb(45,100,220),
        6=>Color32::from_rgb(190,70,200),
        7=>Color32::from_rgb(45,45,45),
        _=>Color32::from_rgb(175,70,40),
    }
}

fn canvas(
    ui:&mut egui::Ui,
    datasets:&[Dataset],
    rasters:&[GeoTiffInfo],
    textures:&[Option<egui::TextureHandle>],
    overlays:&[DxfOverlay],
    virtual_trenches:&[VirtualTrench],
    bgt_layers:&[BgtLayer],
    comments:&[Comment],
    pdok_texture:Option<&egui::TextureHandle>,
    pdok_bounds:Option<Bounds>,
    zoom:&mut f64,
    pan:&mut egui::Vec2
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
        bounds=union_bounds(bounds,Some(Bounds {
            min_x:t.start.x.min(t.end.x)-t.width_meters,
            min_y:t.start.y.min(t.end.y)-t.width_meters,
            max_x:t.start.x.max(t.end.x)+t.width_meters,
            max_y:t.start.y.max(t.end.y)+t.width_meters,
        }));
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
    for trench in virtual_trenches {
        let vertices=trench.polygon();
        for i in 0..4 {
            let a=vertices[i]; let b=vertices[(i+1)%4];
            painter.line_segment([project(a.0,a.1),project(b.0,b.1)],Stroke::new(1.5,ORANGE));
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
                ],Stroke::new(1.0,color));
            }
        }
    }
    for overlay in overlays.iter().filter(|o|o.visible) {
        for feature in &overlay.features {
            let stroke=Stroke::new(1.6,aci_color(feature.color_aci));
            for pair in feature.points.windows(2) {
                painter.line_segment([
                    project(pair[0].x,pair[0].y),
                    project(pair[1].x,pair[1].y)
                ],stroke);
            }
        }
    }
    for dataset in datasets {
        for feature in &dataset.features {
            let color=match sleufbase_rust::matching_rule(&feature.source_name,RULES) {
                Some(rule) if rule.label=="Waterleiding"=>Color32::from_rgb(40,105,235),
                Some(rule) if rule.label=="Laagspanning"=>Color32::from_rgb(236,174,20),
                Some(rule) if rule.label=="Middenspanning"=>Color32::from_rgb(235,40,45),
                Some(_)=>ORANGE,
                None=>Color32::GRAY,
            };
            match &feature.geometry {
                Geometry::Point(p)=>{painter.circle_filled(project(p.x,p.y),3.5,color);}
                Geometry::Polyline(vertices)=>{
                    for pair in vertices.windows(2) {
                        painter.line_segment([
                            project(pair[0].x,pair[0].y),
                            project(pair[1].x,pair[1].y)
                        ],Stroke::new(1.8,color));
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
}

impl eframe::App for SleufBaseApp {
    fn update(&mut self,ctx:&egui::Context,_frame:&mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::light());
        self.try_autosave();
        self.poll_pdok(ctx);
        self.poll_live_jobs();
        self.poll_live_download(ctx);
        self.poll_bgt();
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
                if ui.button("Nieuw project").clicked() {self.new_project();}
                if ui.button("Open Rust-project").clicked() {
                    if let Some(path)=rfd::FileDialog::new()
                        .add_filter("Rust-project JSON",&["json"]).pick_file()
                    {self.load_project_file(path,ctx);}
                }
                if ui.button("Opslaan").clicked() {self.save_project_file(false);}
                if ui.button("Opslaan als...").clicked() {self.save_project_file(true);}
                if ui.button("Instellingen").clicked() {self.show_settings_dialog=true;}
                if ui.button("Import oude voorkeuren").clicked() {self.import_old_settings();}
                if ui.button("Herstel back-up").clicked() {self.restore_backup(ctx);}
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
                if ui.add_enabled(!self.bgt_busy,egui::Button::new("BGT wegdelen")).clicked() {
                    self.start_bgt(BgtKind::Wegdeel);
                }
                if ui.add_enabled(!self.bgt_busy,egui::Button::new("BGT terrein")).clicked() {
                    self.start_bgt(BgtKind::OnbegroeidTerreindeel);
                }
                if ui.add_enabled(!self.ahn_busy,egui::Button::new("AHN hoogte")).clicked() {
                    self.request_ahn();
                }
                if ui.button("Open DXF").clicked() {
                    if let Some(files)=rfd::FileDialog::new()
                        .add_filter("AutoCAD DXF",&["dxf"]).pick_files()
                    {self.load_overlays(files);}
                }
                ui.separator();
                if ui.button("Excel disciplines").clicked() {self.save_excel();}
                if ui.button("DXF objecten").clicked() {self.save_dxf();}
                if ui.button("Kaart terugzetten").clicked() {self.zoom=1.0;self.pan=egui::Vec2::ZERO;}
            });
            if !self.message.is_empty() {
                ui.label(egui::RichText::new(&self.message).small().color(DARK));
            }
        });

        egui::SidePanel::left("sources")
            .default_width(300.0).min_width(230.0).show(ctx,|ui|{

            ui.heading("KickTheMap online");
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
            ui.checkbox(&mut self.active_only,"Alleen actief in Python-SleufBase");
            if self.active_only {
                ui.small("Leest oude KickTheMap-state, maximaal 6 seconden oud.");
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
                for info in &self.geotiffs {
                    ui.label(info.path.file_name().and_then(|s|s.to_str()).unwrap_or("TIFF"));
                    ui.small(format!("{}x{}, EPSG {}, job {}",
                        info.width,info.height,
                        info.epsg.map(|v|v.to_string()).unwrap_or_else(||"?".into()),
                        info.kickthemap_job_id.map(|v|v.to_string()).unwrap_or_else(||"?".into())));
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
            for trench in &self.virtual_trenches {
                ui.label(format!("{} ({:.2} m)",trench.label,trench.length()));
            }
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
            ui.heading("PDOK BGT-lagen");
            for layer in &mut self.bgt_layers {
                if ui.checkbox(&mut layer.visible,
                    format!("{} ({} paden)",layer.kind.label(),layer.paths.len())
                ).changed(){self.dirty=true;}
            }
            ui.separator();
            ui.heading("DXF-lagen");
            egui::ScrollArea::vertical().max_height(150.0).show(ui,|ui|{
                for overlay in &mut self.overlays {
                    let label=overlay.path.file_name().and_then(|s|s.to_str()).unwrap_or("DXF");
                    let changed=ui.checkbox(&mut overlay.visible,label).changed();
                    if changed {self.dirty=true;}
                    ui.small(format!("{} entiteiten",overlay.features.len()));
                }
            });
            ui.separator();
            if let Some(ds)=self.selected.and_then(|i|self.datasets.get(i)) {
                ui.strong(&ds.title);
                let summary=sleufbase_rust::summarize(ds,RULES);
                ui.label(format!("Job-ID {}, {} objecten, {} disciplines",
                    ds.job_id,ds.features.len(),summary.distinct_count()));
                for (rule,count) in RULES.iter().zip(summary.counts.iter()) {
                    if *count>0 {ui.label(format!("{}: {}",rule.label,count));}
                }
            }
        });

        egui::CentralPanel::default().show(ctx,|ui|{
            ui.heading("Kaart");
            ui.label("Zoom met scrollwiel, sleep om te verschuiven. Basis GeoTIFF en DXF zonder PDOK ondergrond.");
            let selected:Vec<Dataset>=self.selected.and_then(|i|self.datasets.get(i))
                .into_iter().cloned().collect();
            canvas(ui,&selected,&self.geotiffs,&self.raster_textures,
                &self.overlays,&self.virtual_trenches,&self.bgt_layers,&self.comments,
                self.pdok_texture.as_ref(),self.pdok_bounds,
                &mut self.zoom,&mut self.pan);
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
                ui.strong("Veilige, eigen Rust-instellingen");
                ui.checkbox(&mut self.migrated_settings.template_auto_fill_bgt_fysiek_voorkomen,
                    "BGT fysiek voorkomen automatisch aanvullen (voorbereid)");
                ui.checkbox(&mut self.migrated_settings.template_auto_export_discipline_excel,
                    "Excel automatisch bij template-DXF (voorbereid)");
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

    }
}
