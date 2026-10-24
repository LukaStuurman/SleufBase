//! Standalone original cadastral export, with source ownership returned before
//! any deferred project action. The desktop never clones whole project arrays.
use super::*;
use sleufbase_rust::{cadastral_overview as overview,export_control::Control,project_exchange_worker::Worker};

pub(super) struct State {
    open:bool,settings:overview::Settings,offline:bool,include_bgt:bool,error:String,
    worker:Option<Worker>,events:Option<Receiver<String>>,deferred:Option<ProjectAction>,
}
impl Default for State {fn default()->Self{Self{open:false,settings:overview::Settings::default(),offline:false,include_bgt:false,error:String::new(),worker:None,events:None,deferred:None}}}
impl State {pub(super) fn busy(&self)->bool{self.worker.as_ref().is_some_and(Worker::is_busy)}}
fn request(project:&NativeProject,mut settings:overview::Settings,offline:bool,include_bgt:bool,control:&Control)->Result<overview::Request,String>{
    if project.rasters.iter().any(|r|r.epsg.is_some_and(|epsg|epsg!=28992)){return Err("Kadastrale export gebruikt RD (EPSG:28992). Transformeer afwijkende GeoTIFFs eerst.".into());}
    let mut layers:Vec<_>=project.rasters.iter().map(|r|overview::Layer{path:r.path.clone(),transform:r.transform,bounds:r.bounds,metadata:r.metadata.clone(),virtual_trench:None}).collect();
    for trench in &project.virtual_trenches{
        control.check()?;let layout=sleufbase_rust::virtual_raster::layout(trench,2.5)?;
        // Original virtual layers use their synthetic TIFF names for PS labels.
        layers.push(overview::Layer{path:format!("{}.tiff",trench.label).into(),transform:layout.transform(),bounds:layout.bounds,metadata:trench.metadata.clone(),virtual_trench:Some(trench.clone())});
    }
    if layers.is_empty(){return Err("Laad eerst GeoTIFFs of virtuele proefsleuven".into());}
    let mut combined=layers[0].bounds;for layer in &layers[1..]{let b=layer.bounds;combined=Bounds{min_x:combined.min_x.min(b.min_x),min_y:combined.min_y.min(b.min_y),max_x:combined.max_x.max(b.max_x),max_y:combined.max_y.max(b.max_y)};}
    let pad=((combined.max_x-combined.min_x).max(combined.max_y-combined.min_y)*0.35).clamp(60.,180.);
    let bounds=Bounds{min_x:combined.min_x-pad,min_y:combined.min_y-pad,max_x:combined.max_x+pad,max_y:combined.max_y+pad};
    let mut linework=Vec::new();let mut labels=Vec::new();
    if offline{
        let cad=project.cadastral.as_ref().filter(|c|c.visible).ok_or("Laad eerst kadastrale gegevens of schakel offline export uit")?;
        linework.extend(cad.linework.iter().cloned());labels.extend(cad.labels.iter().cloned());
    }else{
        let cad=cadastral::fetch_cadastral_with_cancel(bounds,control.cancellation_flag())?;linework=cad.linework;labels=cad.labels;
    }
    if include_bgt{
        if offline{
            for layer in project.bgt_layers.iter().filter(|l|l.visible&&l.kind==BgtKind::Wegdeel){linework.push(cadastral::CadLinework{layer_name:"BGT_WEGDEEL".into(),paths:layer.paths.clone()});}
        }else{
            let layer=bgt::request_bgt_with_cancel(BgtKind::Wegdeel,bounds,control.cancellation_flag())?;
            linework.push(cadastral::CadLinework{layer_name:"BGT_WEGDEEL".into(),paths:layer.paths});
        }
    }
    if let Some(archive)=&project.legacy_archive{settings.protected_sources.push(archive.source_path.clone());}
    settings.protected_sources.extend(project.datasets.iter().map(|d|d.source_path.clone()));
    settings.protected_sources.extend(project.overlays.iter().map(|d|d.path.clone()));
    control.check()?;Ok(overview::Request{layers,linework,labels,settings})
}
impl SleufBaseApp {
    pub(super) fn defer_for_overview(&mut self,action:&ProjectAction)->bool{
        if self.overview_ui.busy(){self.overview_ui.worker.as_ref().unwrap().cancel();self.overview_ui.deferred=Some(action.clone());true}else{false}
    }
    pub(super) fn open_cadastral_overview(&mut self){
        if self.project_sources_owned()||self.template_job.is_some()||self.export_intent.is_some()||self.file_intent.is_some()||self.pending_project_action.is_some()||self.trench_editor.is_some(){self.message="Rond de lopende taak of het sleufconcept eerst af".into();return;}
        if self.geotiffs.is_empty()&&self.virtual_trenches.is_empty(){self.message="Laad eerst GeoTIFFs of virtuele proefsleuven".into();return;}
        match self.project_export_settings().and_then(|s|overview::Settings::from_settings(&s)){
            Ok(settings)=>{self.overview_ui.settings=settings;self.overview_ui.error.clear();self.overview_ui.open=true;},Err(error)=>self.message=error,
        }
    }
    pub(super) fn start_cadastral_overview(&mut self,output:PathBuf)->Result<(),String>{
        if self.project_sources_owned()||self.template_job.is_some()||self.export_intent.is_some()||self.file_intent.is_some()||self.pending_project_action.is_some()||self.trench_editor.is_some(){return Err("Rond de lopende taak eerst af".into());}
        if output.exists(){return Err("Het uitvoerbestand bestaat al; kies een andere naam".into());}
        if output.extension().is_none_or(|s|!s.eq_ignore_ascii_case("dxf")){return Err("Kies een uitvoerbestand met extensie .dxf".into());}
        let mut settings=self.overview_ui.settings.clone();settings.protected_sources.extend(self.project_path.iter().cloned());
        let offline=self.overview_ui.offline;let include_bgt=self.overview_ui.include_bgt;let(tx,rx)=mpsc::channel();
        let worker=Worker::with_processor(move|project,path,control|{
            let _=tx.send(if offline{"Geladen kaartgegevens voorbereiden…"}else{"Kadastrale gegevens ophalen…"}.into());
            let request=request(project,settings.clone(),offline,include_bgt,control)?;
            let result=overview::export_with_progress(path,&request,control,|s|{let _=tx.send(s.into());})?;
            let _=tx.send(format!("{} proefsleuven en {} TIFF-afbeeldingen opgeslagen",result.layers,result.images));Ok(())
        })?;
        self.launch_overview_worker(worker,rx,output)
    }
    fn launch_overview_worker(&mut self,mut worker:Worker,rx:Receiver<String>,output:PathBuf)->Result<(),String>{
        let project=self.take_project_sources();
        match worker.start(project,output){Ok(())=>{self.overview_ui.worker=Some(worker);self.overview_ui.events=Some(rx);self.overview_ui.error.clear();Ok(())},Err(error)=>{self.restore_project_sources(error.project);Err(error.message)}}
    }
    pub(super) fn poll_cadastral_overview(&mut self,ctx:&egui::Context){
        if let Some(rx)=&self.overview_ui.events{for event in rx.try_iter(){self.message=event;}}
        let ready=self.overview_ui.worker.as_mut().and_then(Worker::poll);if let Some(ready)=ready{
            self.restore_project_sources(ready.project);self.overview_ui.worker=None;self.overview_ui.events=None;
            match ready.result{Ok(())=>{self.message=format!("Kadastrale DXF opgeslagen: {}",ready.path.display());self.overview_ui.open=false;},Err(error)=>{self.overview_ui.error=error;self.overview_ui.open=true;}}
            if let Some(action)=self.overview_ui.deferred.take(){self.request_project_action(action,ctx);}
        }
    }
    pub(super) fn show_cadastral_overview(&mut self,ctx:&egui::Context){
        if !self.overview_ui.open&&!self.overview_ui.busy(){return;}
        let mut state=std::mem::take(&mut self.overview_ui);let mut open=state.open;let mut start=false;
        egui::Window::new("Kadastrale DXF exporteren").open(&mut open).default_width(530.).show(ctx,|ui|{
            if state.busy(){ui.spinner();ui.label(&self.message);if ui.button("Annuleren").clicked(){state.worker.as_ref().unwrap().cancel();}ctx.request_repaint_after(Duration::from_millis(40));return;}
            ui.label(format!("{} GeoTIFFs en {} virtuele proefsleuven",self.geotiffs.len(),self.virtual_trenches.len()));
            egui::ComboBox::from_label("Proefsleuven tekenen als").selected_text(match state.settings.trench_mode{overview::TrenchMode::Polygon=>"Vlak",overview::TrenchMode::Centerline=>"Hartlijn",overview::TrenchMode::None=>"Geen"}).show_ui(ui,|ui|{for(mode,label)in[(overview::TrenchMode::Polygon,"Vlak"),(overview::TrenchMode::Centerline,"Hartlijn"),(overview::TrenchMode::None,"Geen")]{ui.selectable_value(&mut state.settings.trench_mode,mode,label);}});
            ui.checkbox(&mut state.settings.include_tiff_images,"TIFF-afbeeldingen opnemen");ui.checkbox(&mut state.include_bgt,"BGT-wegdelen meenemen");ui.checkbox(&mut state.offline,"Alleen geladen kaartgegevens gebruiken (offline)");
            for(label,color)in[("Hartlijnkleur",&mut state.settings.centerline_color),("Labelkleur",&mut state.settings.label_color)]{ui.horizontal(|ui|{ui.label(label);ui.color_edit_button_srgb(color);});}
            ui.horizontal(|ui|{ui.label("Labelafstand (m)");ui.add(egui::DragValue::new(&mut state.settings.label_gap).range(0. ..=1000.).speed(0.1));});
            if !state.error.is_empty(){ui.colored_label(DARK,&state.error);}start=ui.button("Kies nieuwe uitvoer-DXF…").clicked();
        });
        if !open&&state.busy(){state.worker.as_ref().unwrap().cancel();}state.open=open;self.overview_ui=state;
        if start{if let Some(path)=rfd::FileDialog::new().add_filter("AutoCAD DXF",&["dxf"]).set_file_name("SleufBase-Kadastraal.dxf").save_file(){if let Err(error)=self.start_cadastral_overview(path){self.overview_ui.error=error;}}}
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    fn app()->SleufBaseApp{
        let mut app=SleufBaseApp::default();app.project_name="Behoud".into();app.project_path=Some("source.json".into());app.dirty=true;
        app.virtual_trenches.push(VirtualTrench::new("PS7".into(),Vertex{x:155000.,y:463000.,z:None},Vertex{x:155010.,y:463000.,z:None},1.).unwrap());
        app.cadastral=Some(cadastral::CadastralLayer{visible:true,bounds:Bounds{min_x:154900.,min_y:462900.,max_x:155100.,max_y:463100.},linework:vec![cadastral::CadLinework{layer_name:"KAD_GRENS".into(),paths:vec![vec![(154995.,463015.),(155015.,463015.)]]}],labels:Vec::new()});app.overview_ui.offline=true;app
    }
    fn wait(app:&mut SleufBaseApp,ctx:&egui::Context){let deadline=Instant::now()+Duration::from_secs(10);while app.overview_ui.busy(){assert!(Instant::now()<deadline);app.poll_cadastral_overview(ctx);std::thread::sleep(Duration::from_millis(2));}}
    #[test]
    fn real_desktop_overview_exports_virtual_source_and_restores_allocations(){
        let mut app=app();let pointer=app.virtual_trenches.as_ptr();let generation=app.project_generation;app.overview_ui.settings.trench_mode=overview::TrenchMode::Centerline;
        let dir=tempfile::tempdir().unwrap();let output=dir.path().join("overview.dxf");app.start_cadastral_overview(output.clone()).unwrap();assert!(app.virtual_trenches.is_empty());wait(&mut app,&egui::Context::default());
        assert!(output.is_file(),"{}",app.overview_ui.error);assert_eq!(app.virtual_trenches.as_ptr(),pointer);assert_eq!(app.project_generation,generation);assert_eq!(app.project_path,Some("source.json".into()));assert!(app.dirty);
        let doc=sleufbase_rust::dxf_document::Document::load(&output,&std::sync::atomic::AtomicBool::new(false)).unwrap();assert!(doc.records().any(|r|r.kind()=="TEXT"&&r.first(1)==Some("PS7")));
        let bytes=std::fs::read(&output).unwrap();assert!(app.start_cadastral_overview(output.clone()).is_err());assert_eq!(std::fs::read(output).unwrap(),bytes);
    }
    #[test]
    fn cancelled_overview_returns_sources_before_unsaved_close_and_blocks_other_workers(){
        let mut app=app();let pointer=app.virtual_trenches.as_ptr();let worker=Worker::with_processor(|_,_,c|loop{c.check()?;std::thread::sleep(Duration::from_millis(2));}).unwrap();let(_,rx)=mpsc::channel();let dir=tempfile::tempdir().unwrap();app.launch_overview_worker(worker,rx,dir.path().join("cancel.dxf")).unwrap();
        assert!(!app.save_project_to_path(dir.path().join("empty.json")));app.start_python_project_export(dir.path().join("other.klicproject"));assert!(!app.project_exchange_busy());
        let ctx=egui::Context::default();app.request_project_action(ProjectAction::Close,&ctx);wait(&mut app,&ctx);assert_eq!(app.virtual_trenches.as_ptr(),pointer);assert_eq!(app.pending_project_action,Some(ProjectAction::Close));assert!(!app.close_confirmed);assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),0);
    }
    #[test]
    fn incompatible_crs_is_reported_before_provider_work(){
        let mut app=app();let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/gray16.tiff");let mut raster=sleufbase_rust::geotiff::read_geotiff_metadata(&path).unwrap();raster.epsg=Some(4326);app.geotiffs.push(raster);
        let cancel=Control::default();cancel.cancel();assert!(request(&app.snapshot(),Default::default(),false,false,&cancel).unwrap_err().contains("EPSG:28992"));
    }
}
