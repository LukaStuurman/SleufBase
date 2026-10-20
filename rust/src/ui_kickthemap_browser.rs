//! KickTheMap browser-account and jobs workspace for native desktop.
//! Browser auth, filtering, persistent offline snapshots and queued downloads
//! are coordinated without blocking egui's main thread.
use super::*;
use sleufbase_rust::{
    kickthemap_browser::Browser,
    jobs_cache::{self, Job},
};
use std::{collections::{BTreeSet,VecDeque},sync::{Arc,atomic::{AtomicBool,Ordering}}};

#[derive(Clone,Copy,PartialEq,Eq)]
enum Sort {Newest,Name,Id}
pub(super) struct BrowserUi {
    pub(super) open:bool,
    pub(super) browser:Option<Browser>,
    launching:Option<Receiver<Result<Browser,String>>>,
    capturing:Option<Receiver<(Browser,Result<(KickTheMapLiveClient,Vec<Job>),String>)>>,
    pub(super) selected:BTreeSet<u64>,
    filter:String,
    only_downloadable:bool,
    only_loaded:bool,
    sort:Sort,
    pub(super) queue:VecDeque<(u64,RemoteFileKind)>,
    pub(super) batch_mode:bool,
    pub(super) downloaded_json:Vec<PathBuf>,
    pub(super) downloaded_tiffs:Vec<PathBuf>,
    pub(super) completed:usize,
    pub(super) failures:Vec<String>,
    pub(super) cancel:Arc<AtomicBool>,
    status:String,
}
impl Default for BrowserUi {
    fn default()->Self {
        Self {open:false,browser:None,launching:None,capturing:None,
            selected:BTreeSet::new(),filter:String::new(),
            only_downloadable:false,only_loaded:false,
            sort:Sort::Newest,queue:VecDeque::new(),batch_mode:false,
            downloaded_json:Vec::new(),downloaded_tiffs:Vec::new(),
            completed:0,failures:Vec::new(),cancel:Arc::new(AtomicBool::new(false)),
            status:String::new()}
    }
}
impl SleufBaseApp {
    pub(super) fn launch_kickthemap_browser(&mut self) {
        if self.browser_ui.launching.is_some() || self.browser_ui.capturing.is_some() ||
            self.browser_ui.browser.is_some() {return;}
        let (tx,rx)=mpsc::channel();
        self.browser_ui.launching=Some(rx);
        self.browser_ui.status="Een privé Microsoft Edge-venster starten. Log daarin zelf in, inclusief reCAPTCHA.".into();
        std::thread::spawn(move||{
            let _=tx.send(Browser::launch());
        });
    }
    pub(super) fn capture_kickthemap_browser(&mut self) {
        if self.browser_ui.capturing.is_some() || self.live_busy{return;}
        let Some(browser)=self.browser_ui.browser.take() else {return};
        let (tx,rx)=mpsc::channel();
        self.browser_ui.capturing=Some(rx);
        self.browser_ui.status="Sessie van het eigen Edge-venster controleren…".into();
        std::thread::spawn(move|| {
            let result=browser.cookies()
                .and_then(|cookies|KickTheMapLiveClient::from_browser_cookies(&cookies))
                .and_then(|mut session|{
                    match session.fetch_jobs() {
                        Ok(jobs)=>Ok((session,jobs)),
                        Err(original)=>{
                            // Site API endpoints and token behavior can differ
                            // for requests made outside the real browser. Try
                            // the same-origin fetch from this same owned tab.
                            browser.browser_jobs().map(|jobs|(session,jobs))
                                .map_err(|fallback|format!("{original}; browserfallback: {fallback}"))
                        }
                    }
                });
            let _=tx.send((browser,result));
        });
    }
    pub(super) fn poll_kickthemap_browser(&mut self,ctx:&egui::Context) {
        let event=self.browser_ui.launching.as_ref().map(|rx|rx.try_recv());
        match event {
            Some(Ok(Ok(browser)))=>{
                self.browser_ui.browser=Some(browser);
                self.browser_ui.launching=None;
                self.browser_ui.status="Edge staat open. Meld je aan op KickTheMap en klik daarna op 'Sessie overnemen'.".into();
            }
            Some(Ok(Err(err)))=>{
                self.browser_ui.launching=None;
                self.browser_ui.status=format!("Edge starten mislukt: {err}");
            }
            Some(Err(TryRecvError::Disconnected))=>{
                self.browser_ui.launching=None;
                self.browser_ui.status="Edge-browserproces is onverwacht gestopt".into();
            }
            _=>{}
        }
        let event=self.browser_ui.capturing.as_ref().map(|rx|rx.try_recv());
        match event {
            Some(Ok((browser,result)))=>{
                self.browser_ui.browser=Some(browser);
                self.browser_ui.capturing=None;
                match result {
                    Ok((session,jobs))=>{
                        self.live_session=Some(session);
                        self.live_jobs=jobs;
                        self.live_password.clear();
                        self.save_live_cache();
                        self.browser_ui.status=format!(
                            "KickTheMap verbonden: {} live jobs. Selecteer jobs voor downloads.",
                            self.live_jobs.len()
                        );
                    }
                    Err(err)=>self.browser_ui.status=format!(
                        "Browseraanmelding nog niet bruikbaar: {err}. Je kunt opnieuw proberen."
                    ),
                }
            }
            Some(Err(TryRecvError::Disconnected))=>{
                self.browser_ui.capturing=None;
                self.browser_ui.status="Browserovername afgebroken; start desgewenst een nieuw venster".into();
            }
            _=>{}
        }
        // Prepare one kind at a time for FileLoader; otherwise starting TIFF
        // import would cancel an unfinished jobFeatures import.
        if self.file_intent.is_none() && !self.live_busy &&
            !self.browser_ui.batch_mode && self.browser_ui.queue.is_empty() {
            if !self.browser_ui.downloaded_json.is_empty() {
                let paths=std::mem::take(&mut self.browser_ui.downloaded_json);
                self.load_files(paths);
            } else if !self.browser_ui.downloaded_tiffs.is_empty() {
                let paths=std::mem::take(&mut self.browser_ui.downloaded_tiffs);
                self.load_geotiffs(paths,ctx);
            }
        }
        if self.browser_ui.batch_mode && !self.live_busy && !self.browser_ui.queue.is_empty() {
            if self.browser_ui.cancel.load(Ordering::Relaxed) {
                self.browser_ui.queue.clear();
            } else if let Some((job_id,kind))=self.browser_ui.queue.pop_front() {
                self.download_live_job(job_id,kind);
            }
        }
        if self.browser_ui.batch_mode && !self.live_busy && self.browser_ui.queue.is_empty() {
            self.browser_ui.batch_mode=false;
            self.browser_ui.status=format!(
                "Downloadopdracht afgerond: {} bestanden, {} fouten. Bestanden worden nu ingeladen.",
                self.browser_ui.completed,self.browser_ui.failures.len()
            );
        }
    }
    pub(super) fn save_live_cache(&mut self) {
        if self.live_jobs.is_empty() {return;}
        let email=self.live_email.trim();
        let root=kickthemap_cache_dir();
        let path=jobs_cache::cache_path(&root,email);
        if jobs_cache::write_live_cache(&path,email,&self.live_jobs).is_ok() {
            if let Ok(cache)=read_cache(&path) {self.jobs_cache=Some(cache);}
        }
    }
    pub(super) fn queue_kickthemap_jobs(&mut self,selected:&[u64],
        download_tiff:bool,download_json:bool)
    {
        if self.live_session.is_none() || self.live_busy || self.browser_ui.batch_mode {
            self.browser_ui.status="Log eerst in en voltooi de lopende download".into();
            return;
        }
        if !download_tiff && !download_json {return;}
        let mut queued=VecDeque::new();
        for &id in selected.iter().take(500) {
            if self.live_jobs.iter().any(|job|job.job_id==id) {
                if download_json {queued.push_back((id,RemoteFileKind::JobFeatures));}
                if download_tiff {queued.push_back((id,RemoteFileKind::GeoTiff));}
            }
        }
        if queued.is_empty() {return;}
        self.browser_ui.queue=queued;
        self.browser_ui.completed=0;
        self.browser_ui.failures.clear();
        self.browser_ui.cancel.store(false,Ordering::SeqCst);
        self.browser_ui.batch_mode=true;
        self.browser_ui.status=format!(
            "{} bestanden ingepland. Elke download wordt na validatie opgeslagen zonder overschrijven.",
            self.browser_ui.queue.len()
        );
    }
    pub(super) fn show_kickthemap_workspace(&mut self,ctx:&egui::Context) {
        if !self.browser_ui.open {return;}
        let mut open=true;
        let mut action=None;
        let loaded=self.active_job_ids();
        let jobs=&self.live_jobs;
        let cache=self.jobs_cache.as_ref().map(|c|c.jobs.as_slice()).unwrap_or(&[]);
        let live=self.live_session.is_some();
        let busy=self.live_busy;
        let source=if live {jobs.as_slice()} else {cache};
        let browser=&mut self.browser_ui;
        let query=browser.filter.trim().to_lowercase();
        let mut indices=(0..source.len()).filter(|&i|{
            let j=&source[i];
            (!browser.only_downloadable || j.downloadable)
                && (!browser.only_loaded || loaded.contains(&j.job_id))
                && (query.is_empty()
                    || j.title.to_lowercase().contains(&query)
                    || j.address.to_lowercase().contains(&query)
                    || j.municipality.to_lowercase().contains(&query)
                    || j.client_date.to_lowercase().contains(&query)
                    || j.job_id.to_string().contains(&query))
        }).collect::<Vec<_>>();
        match browser.sort {
            Sort::Newest=>indices.sort_by(|&a,&b|
                source[b].client_date.cmp(&source[a].client_date)
                    .then(source[a].job_id.cmp(&source[b].job_id))),
            Sort::Name=>indices.sort_by(|&a,&b|
                source[a].title.to_lowercase().cmp(&source[b].title.to_lowercase())),
            Sort::Id=>indices.sort_by_key(|&i|source[i].job_id),
        }
        let filtered_ids=indices.iter().map(|&i|source[i].job_id).collect::<Vec<_>>();
        egui::Window::new("KickTheMap browser & jobs — Rust")
            .open(&mut open).default_width(850.).default_height(670.)
            .resizable(true).show(ctx,|ui|{
                egui::ScrollArea::vertical().max_height(610.).show(ui,|ui|{
                    ui.heading("Browseraanmelding");
                    ui.label("SleufBase opent een eigen tijdelijk Edge-profiel. Log rechtstreeks in op KickTheMap en los reCAPTCHA zelf op; er wordt geen wachtwoord of bestaande browsersessie uitgelezen.");
                    ui.horizontal(|ui|{
                        if ui.add_enabled(browser.browser.is_none() &&
                            browser.launching.is_none()&&browser.capturing.is_none(),
                            egui::Button::new("Open KickTheMap in Edge")).clicked() {
                            action=Some("launch");
                        }
                        if ui.add_enabled(browser.browser.is_some()&&browser.capturing.is_none()&&!busy,
                            egui::Button::new("Sessie overnemen")).clicked(){
                            action=Some("capture");
                        }
                        if ui.add_enabled(browser.browser.is_some()&&browser.capturing.is_none(),
                            egui::Button::new("Privé Edge-venster sluiten")).clicked(){action=Some("close");}
                        if ui.add_enabled(live&&!busy,
                            egui::Button::new("Vernieuw live jobs")).clicked(){action=Some("refresh");}
                    });
                    if browser.launching.is_some()||browser.capturing.is_some() {ui.spinner();}
                    if !browser.status.is_empty(){ui.small(&browser.status);}
                    ui.separator();
                    ui.heading(format!("Jobs: {} zichtbaar / {} {}",
                        indices.len(),source.len(),
                        if live {"live"} else {"offline cache — niet live"}));
                    ui.horizontal(|ui|{
                        ui.label("Zoeken");
                        ui.add(egui::TextEdit::singleline(&mut browser.filter)
                            .desired_width(320.).hint_text("Naam, adres, gemeente, datum, job-ID"));
                        ui.checkbox(&mut browser.only_downloadable,"Downloadbaar");
                        ui.checkbox(&mut browser.only_loaded,"Alleen geladen");
                    });
                    ui.horizontal(|ui|{
                        ui.radio_value(&mut browser.sort,Sort::Newest,"Datum");
                        ui.radio_value(&mut browser.sort,Sort::Name,"Naam");
                        ui.radio_value(&mut browser.sort,Sort::Id,"Job-ID");
                        if ui.small_button("Selecteer zichtbare").clicked(){
                            browser.selected.extend(filtered_ids.iter().copied());
                        }
                        if ui.small_button("Deselecteer zichtbare").clicked(){
                            for id in &filtered_ids{browser.selected.remove(id);}
                        }
                        if ui.small_button("Wis selectie").clicked(){browser.selected.clear();}
                    });
                    egui::ScrollArea::vertical().max_height(285.)
                        .show_rows(ui,27.,indices.len(),|ui,rows|{
                            for position in rows {
                                let job=&source[indices[position]];
                                ui.horizontal(|ui|{
                                    let mut selected=browser.selected.contains(&job.job_id);
                                    if ui.checkbox(&mut selected,"").changed(){
                                        if selected {browser.selected.insert(job.job_id);}
                                        else{browser.selected.remove(&job.job_id);}
                                    }
                                    ui.label(format!("{} | {} | {} | {} | {} {}",
                                        job.job_id,job.title,job.municipality,
                                        job.client_date,if job.downloadable {"✓"}else{"–"},
                                        if loaded.contains(&job.job_id) {"(geladen)"}else{""}))
                                        .on_hover_text(&job.address);
                                });
                            }
                        });
                    ui.separator();
                    ui.heading(format!("Downloads — {} geselecteerd",browser.selected.len()));
                    ui.horizontal(|ui|{
                        let can=live&&!busy&&!browser.batch_mode&&
                            !browser.selected.is_empty()&&browser.capturing.is_none();
                        if ui.add_enabled(can,egui::Button::new("Objecten downloaden + laden")).clicked(){
                            action=Some("objects");
                        }
                        if ui.add_enabled(can,egui::Button::new("GeoTIFF downloaden + laden")).clicked(){
                            action=Some("tiffs");
                        }
                        if ui.add_enabled(can,egui::Button::new("Beide bestanden per job")).clicked(){
                            action=Some("both");
                        }
                    });
                    if browser.batch_mode||busy {ui.spinner();}
                    if browser.batch_mode {
                        ui.label(format!("{} verwerkt, {} in wachtrij",
                            browser.completed,browser.queue.len()));
                        if ui.button("Annuleer batch").clicked(){action=Some("cancel");}
                    }
                    if !browser.failures.is_empty(){
                        ui.collapsing(format!("Downloadfouten: {}",browser.failures.len()),|ui|{
                            for message in browser.failures.iter().rev().take(20){ui.label(message);}
                        });
                    }
                    ui.small("Bestaande downloads worden nooit overschreven. JobFeatures en GeoTIFF krijgen een identificerende sidecar; de bestanden worden pas na validatie in het actieve Rust-project geladen.");
                });
            });
        self.browser_ui.open=open;
        match action {
            Some("launch")=>self.launch_kickthemap_browser(),
            Some("capture")=>self.capture_kickthemap_browser(),
            Some("close")=>{
                self.browser_ui.browser=None;
                self.browser_ui.status="Privé Edge-venster gesloten".into();
            }
            Some("refresh")=>self.refresh_live_jobs(),
            Some("cancel")=>{
                self.browser_ui.cancel.store(true,Ordering::SeqCst);
                self.browser_ui.queue.clear();
                self.browser_ui.status="Downloadbatch geannuleerd".into();
            }
            Some("objects")|Some("tiffs")|Some("both")=>{
                let selection=self.browser_ui.selected.iter().copied().collect::<Vec<_>>();
                self.queue_kickthemap_jobs(&selection,action!=Some("objects"),action!=Some("tiffs"));
            }
            _=>{}
        }
    }
}
