//! Gemma runs with moved project allocations; returned texts remain a review
//! until explicitly applied to the same unchanged layer in the same project.
use super::*;
use sleufbase_rust::{ai_maaiveld as ai,ai_scene,project_exchange_worker::Worker};
use std::{hash::{Hasher,DefaultHasher},io::Write,sync::Arc};
use sleufbase_rust::ai_profile::Source;

#[derive(Clone,Debug,PartialEq,Eq)]
struct Stamp {source:Source,generation:u64,digest:u64}
struct Preview {image:image::RgbImage,result:ai::Result}
struct Review {stamp:Stamp,texts:[String;3],texture:egui::TextureHandle}
pub(super) struct State {
    open:bool,selected:Option<Source>,worker:Option<Worker>,preview_rx:Option<Receiver<Preview>>,
    stamp:Option<Stamp>,review:Option<Review>,error:String,deferred:Option<ProjectAction>,
    client:Arc<ai::Client>,recovered:Option<NativeProject>,
}
impl Default for State {fn default()->Self{Self{open:false,selected:None,worker:None,preview_rx:None,stamp:None,review:None,error:String::new(),deferred:None,client:Arc::new(ai::Client::default()),recovered:None}}}
impl State {
    pub(super) fn busy(&self)->bool{self.worker.as_ref().is_some_and(Worker::is_busy)}
    pub(super) fn owns_sources(&self)->bool{self.busy()||self.recovered.is_some()}
    pub(super) fn cancel(&self){if let Some(worker)=&self.worker{worker.cancel();}}
}
struct HashWriter(DefaultHasher);
impl Write for HashWriter {fn write(&mut self,bytes:&[u8])->std::io::Result<usize>{self.0.write(bytes);Ok(bytes.len())}fn flush(&mut self)->std::io::Result<()>{Ok(())}}
fn stamp(source:Source,generation:u64,rasters:&[GeoTiffInfo],trenches:&[VirtualTrench])->Result<Stamp,String>{
    let mut writer=HashWriter(DefaultHasher::new());
    match source{Source::Raster(i)=>serde_json::to_writer(&mut writer,rasters.get(i).ok_or("De gekozen TIFF is niet meer beschikbaar")?),Source::Virtual(i)=>serde_json::to_writer(&mut writer,trenches.get(i).ok_or("De gekozen sleuf is niet meer beschikbaar")?)}.map_err(|e|e.to_string())?;
    Ok(Stamp{source,generation,digest:writer.0.finish()})
}
fn metadata<'a>(source:Source,rasters:&'a[GeoTiffInfo],trenches:&'a[VirtualTrench])->Result<&'a serde_json::Value,String>{
    match source{Source::Raster(i)=>rasters.get(i).map(|r|&r.metadata),Source::Virtual(i)=>trenches.get(i).map(|t|&t.metadata)}.ok_or("De gekozen proefsleuf is niet meer beschikbaar".into())
}
fn texts_metadata(original:&serde_json::Value,texts:&[String;3])->Result<serde_json::Value,String>{
    sleufbase_rust::raster_points::validate_metadata(original)?;
    let mut candidate=original.as_object().cloned().unwrap_or_default();
    let entry=candidate.entry("template_maaiveld_segments").or_insert_with(||serde_json::json!({}));
    if !entry.is_object(){*entry=serde_json::json!({});}
    let segments=entry.as_object_mut().unwrap();
    for(key,text)in["start","middle","end"].into_iter().zip(texts){
        let text=text.trim();if text.len()>4096||text.chars().any(char::is_control){return Err("Een maaiveldtekst is te lang of bevat ongeldige tekens".into());}
        let entry=segments.entry(key).or_insert_with(||serde_json::json!({}));if !entry.is_object(){*entry=serde_json::json!({});}
        let object=entry.as_object_mut().unwrap();object.insert("text".into(),serde_json::json!(if text.is_empty(){"INVULLEN"}else{text}));
        object.entry("color").or_insert_with(||serde_json::json!(ai::DEFAULT_MAAIVELD_COLOR));
    }
    let candidate=serde_json::Value::Object(candidate);sleufbase_rust::raster_points::validate_metadata(&candidate)?;Ok(candidate)
}
fn source_name(source:Source,rasters:&[GeoTiffInfo],trenches:&[VirtualTrench])->String{
    match source{Source::Raster(i)=>rasters.get(i).map(|r|r.path.file_name().unwrap_or_default().to_string_lossy().into_owned()),Source::Virtual(i)=>trenches.get(i).map(|t|t.label.clone())}.unwrap_or_else(||"Kies een proefsleuf".into())
}
fn runtime_root()->PathBuf{
    if let Some(root)=std::env::var_os("SLEUFBASE_WORKSPACE_ROOT"){return root.into();}
    for start in [std::env::current_dir().ok(),std::env::current_exe().ok().and_then(|p|p.parent().map(std::path::Path::to_path_buf))].into_iter().flatten(){
        if let Some(root)=start.ancestors().find(|p|p.join(".tools").is_dir()){return root.into();}
    }
    std::env::current_exe().ok().and_then(|p|p.parent().and_then(|p|p.parent()).map(std::path::Path::to_path_buf)).unwrap_or_default()
}
fn process(project:&NativeProject,source:Source,settings:&MigratedSettings,opacity:f64,config:&ai::Config,client:&ai::Client,control:&sleufbase_rust::export_control::Control)->Result<Preview,String>{
    control.check()?;let cancel=control.cancellation_flag();
    let prepared=sleufbase_rust::ai_profile::prepare(project,source,settings,control)?;let points=prepared.points;let layer_bounds=prepared.layer_bounds;let size=[1400,1000];let bounds=ai::numbered_layout(layer_bounds,&points,size)?;
    let mut overlays=Vec::new();if settings.map_export.include_cad{sleufbase_rust::template_cad::append_visible(&project.overlays,bounds,&mut overlays,control)?;}
    let background=ai_scene::fetch_background(&settings.map_export.background,bounds,size,cancel).ok();control.check()?;
    let tiff;let virtual_source;
    let raster=match source{
        Source::Raster(i)=>{let r=&project.rasters[i];tiff=sleufbase_rust::template_raster::Source{path:r.path.clone(),transform:r.transform,reverse_orientation:false,axis:None};sleufbase_rust::map_export::Raster::Tiff(&tiff)},
        Source::Virtual(i)=>{virtual_source=sleufbase_rust::virtual_raster::Source{trench:project.virtual_trenches[i].clone(),quality:1.,reverse:false};sleufbase_rust::map_export::Raster::Virtual(&virtual_source)},
    };
    let image=ai_scene::render(layer_bounds,&points,size,Some(raster),&overlays,background.as_ref(),opacity,cancel)?;
    let result=client.request(&image,config,cancel)?;control.check()?;Ok(Preview{image,result})
}
impl SleufBaseApp {
    pub(super) fn defer_for_ai(&mut self,action:&ProjectAction)->bool{
        if self.ai_maaiveld_ui.recovered.is_some(){self.ai_maaiveld_ui.open=true;self.message="Sla eerst de teruggekomen brongegevens op via het maaiveldvenster".into();return true;}
        if self.ai_maaiveld_ui.busy(){self.ai_maaiveld_ui.cancel();self.ai_maaiveld_ui.deferred=Some(action.clone());true}else{false}
    }
    pub(super) fn open_ai_maaiveld(&mut self){
        if self.project_sources_owned()||self.export_intent.is_some()||self.template_job.is_some()||self.file_intent.is_some()||self.trench_editor.is_some(){self.message="Rond de lopende taak of het sleufconcept eerst af".into();return;}
        self.ai_maaiveld_ui.open=true;self.ai_maaiveld_ui.error.clear();
        let selected=self.virtual_map.selected.map(Source::Virtual).or_else(||self.raster_points.selected.as_ref().and_then(|p|self.geotiffs.iter().position(|r|r.path==*p)).map(Source::Raster)).or_else(||(!self.geotiffs.is_empty()).then_some(Source::Raster(0))).or_else(||(!self.virtual_trenches.is_empty()).then_some(Source::Virtual(0)));
        if self.ai_maaiveld_ui.selected!=selected{self.ai_maaiveld_ui.review=None;}self.ai_maaiveld_ui.selected=selected;
    }
    fn launch_ai_worker(&mut self,source:Source,worker:Worker,rx:Receiver<Preview>)->Result<(),String>{
        let stamp=stamp(source,self.project_generation,&self.geotiffs,&self.virtual_trenches)?;
        let project=self.take_project_sources();let mut worker=worker;
        match worker.start(project,PathBuf::new()){
            Ok(())=>{self.ai_maaiveld_ui.selected=Some(source);self.ai_maaiveld_ui.worker=Some(worker);self.ai_maaiveld_ui.preview_rx=Some(rx);self.ai_maaiveld_ui.stamp=Some(stamp);self.ai_maaiveld_ui.review=None;self.ai_maaiveld_ui.error.clear();Ok(())},
            Err(failure)=>{self.restore_project_sources(failure.project);Err(failure.message)},
        }
    }
    fn start_ai_maaiveld(&mut self,source:Source)->Result<(),String>{
        if self.project_sources_owned()||self.export_intent.is_some()||self.template_job.is_some()||self.file_intent.is_some()||self.pending_project_action.is_some()||self.trench_editor.is_some(){return Err("Rond de lopende taak of het sleufconcept eerst af".into());}
        let settings=self.project_export_settings()?;
        let opacity=(self.project_view.opacity_percent/100.).clamp(0.,1.);let config=ai::Config::from_env(runtime_root());let client=self.ai_maaiveld_ui.client.clone();let(tx,rx)=mpsc::sync_channel(1);
        let worker=Worker::with_processor(move|project,_,control|{let preview=process(project,source,&settings,opacity,&config,&client,control)?;control.check()?;tx.send(preview).map_err(|_|"Maaiveldvenster is gesloten".to_owned())?;Ok(())})?;
        self.launch_ai_worker(source,worker,rx)
    }
    pub(super) fn poll_ai_maaiveld(&mut self,ctx:&egui::Context){
        let ready=self.ai_maaiveld_ui.worker.as_mut().and_then(Worker::poll);let Some(ready)=ready else{return};
        let stamp=self.ai_maaiveld_ui.stamp.take();let preview=self.ai_maaiveld_ui.preview_rx.take().and_then(|rx|rx.try_recv().ok());self.ai_maaiveld_ui.worker=None;
        if stamp.as_ref().is_none_or(|s|s.generation!=self.project_generation){
            self.ai_maaiveld_ui.recovered=Some(ready.project);self.ai_maaiveld_ui.open=true;self.ai_maaiveld_ui.error="Het project is gewijzigd. De analyse is niet toegepast; sla de teruggekomen brongegevens op als apart project.".into();return;
        }
        self.restore_project_sources(ready.project);
        match ready.result{
            Ok(())=>if let Some(preview)=preview{
                let texts=std::array::from_fn(|i|preview.result.text_by_key.get(&(i+1).to_string()).cloned().unwrap_or_else(||"INVULLEN".into()));
                let image=egui::ColorImage::from_rgb([preview.image.width()as usize,preview.image.height()as usize],preview.image.as_raw());let texture=ctx.load_texture("maaiveld-gemma-review",image,egui::TextureOptions::LINEAR);
                self.ai_maaiveld_ui.review=Some(Review{stamp:stamp.unwrap(),texts,texture});self.ai_maaiveld_ui.error.clear();self.ai_maaiveld_ui.open=true;
            }else{self.ai_maaiveld_ui.error="Geen analyseresultaat ontvangen. Het project is behouden.".into();},
            Err(error)=>{self.ai_maaiveld_ui.review=None;self.ai_maaiveld_ui.error=error;},
        }
        if let Some(action)=self.ai_maaiveld_ui.deferred.take(){self.ai_maaiveld_ui.review=None;self.request_project_action(action,ctx);}
    }
    fn apply_ai_maaiveld(&mut self)->Result<(),String>{
        let review=self.ai_maaiveld_ui.review.as_ref().ok_or("Er is nog geen resultaat om te controleren")?;
        if self.ai_maaiveld_ui.selected!=Some(review.stamp.source){return Err("De gekozen proefsleuf is gewijzigd. Start een nieuwe analyse.".into());}
        let current=stamp(review.stamp.source,self.project_generation,&self.geotiffs,&self.virtual_trenches)?;
        if current!=review.stamp{return Err("De gekozen proefsleuf of het project is gewijzigd. Start een nieuwe analyse voordat u teksten toepast.".into());}
        let original=metadata(current.source,&self.geotiffs,&self.virtual_trenches)?;let candidate=texts_metadata(original,&review.texts)?;
        let old_bytes=sleufbase_rust::raster_points::validate_metadata(original)?;let new_bytes=sleufbase_rust::raster_points::validate_metadata(&candidate)?;
        let mut total=0usize;for t in &self.virtual_trenches{total=total.saturating_add(t.point_metadata_bytes()?);}for r in &self.geotiffs{total=total.saturating_add(sleufbase_rust::raster_points::validate_metadata(&r.metadata)?);}
        if total.saturating_sub(old_bytes).saturating_add(new_bytes)>16*1024*1024{return Err("De maaiveldteksten passen niet binnen het projectbudget".into());}
        if let Source::Virtual(i)=current.source{if self.virtual_trenches[i].point_metadata_bytes()?.saturating_sub(old_bytes).saturating_add(new_bytes)>2*1024*1024{return Err("De maaiveldteksten passen niet binnen de sleufmetadata".into());}}
        match current.source{Source::Raster(i)=>self.geotiffs[i].metadata=candidate,Source::Virtual(i)=>self.virtual_trenches[i].metadata=candidate}
        self.dirty=true;self.message="Drie gecontroleerde maaiveldteksten toegepast op de gekozen proefsleuf.".into();self.ai_maaiveld_ui.review=None;self.ai_maaiveld_ui.open=false;Ok(())
    }
    pub(super) fn show_ai_maaiveld(&mut self,ctx:&egui::Context){
        if !self.ai_maaiveld_ui.open&&!self.ai_maaiveld_ui.owns_sources(){return;}
        let mut state=std::mem::take(&mut self.ai_maaiveld_ui);let mut open=state.open||state.recovered.is_some();let mut start=None;let mut apply=false;
        egui::Window::new("Maaiveld via Gemma").open(&mut open).default_width(760.).resizable(true).vscroll(true).show(ctx,|ui|{
            if state.busy(){ui.spinner();ui.label("De genummerde kaart en maaiveldteksten worden voorbereid…");if ui.button("Annuleren").clicked(){state.cancel();}ctx.request_repaint_after(Duration::from_millis(50));return;}
            if let Some(project)=&state.recovered{
                ui.label(&state.error);
                if ui.button("Teruggekomen project opslaan…").clicked(){
                    if let Some(path)=rfd::FileDialog::new().add_filter("Rust-project",&["json"]).set_file_name("SleufBase-herstel.json").save_file(){
                        match sleufbase_rust::project::save_project_new(&path,project){Ok(())=>{state.recovered=None;state.error.clear();},Err(error)=>state.error=error}
                    }
                }return;
            }
            ui.label("Kies de proefsleuf waarvan u het maaiveld wilt lezen.");
            let old_selection=state.selected;
            egui::ComboBox::from_id_salt("gemma-source").selected_text(state.selected.map(|s|source_name(s,&self.geotiffs,&self.virtual_trenches)).unwrap_or_else(||"Kies een proefsleuf".into())).show_ui(ui,|ui|{
                for(i,raster)in self.geotiffs.iter().enumerate(){ui.selectable_value(&mut state.selected,Some(Source::Raster(i)),raster.path.file_name().unwrap_or_default().to_string_lossy());}
                for(i,trench)in self.virtual_trenches.iter().enumerate(){ui.selectable_value(&mut state.selected,Some(Source::Virtual(i)),&trench.label);}
            });
            if state.selected!=old_selection{state.review=None;}
            if ui.add_enabled(state.selected.is_some(),egui::Button::new("Lees maaiveld uit de kaart")).clicked(){start=state.selected;}
            if let Some(review)=&mut state.review{
                ui.separator();ui.heading("Controleer teksten");ui.label("1 = begin, 2 = midden, 3 = einde. Pas de teksten zo nodig aan voordat u ze toepast.");
                let available=ui.available_width().min(720.);let ratio=review.texture.size_vec2();ui.add(egui::Image::new(&review.texture).fit_to_exact_size(egui::vec2(available,available*ratio.y/ratio.x)));
                for (i,label)in["1 · Beginpunt","2 · Middenpunt","3 · Eindpunt"].into_iter().enumerate(){ui.horizontal(|ui|{ui.label(label);ui.text_edit_singleline(&mut review.texts[i]);});}
                if ui.button("Pas gecontroleerde teksten toe").clicked(){apply=true;}
            }
            if !state.error.is_empty(){ui.colored_label(DARK,&state.error);}
        });
        if !open&&state.busy(){state.cancel();}state.open=open;self.ai_maaiveld_ui=state;
        if let Some(source)=start{if let Err(error)=self.start_ai_maaiveld(source){self.ai_maaiveld_ui.error=error;}}
        if apply{if let Err(error)=self.apply_ai_maaiveld(){self.ai_maaiveld_ui.error=error;}}
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    fn app()->SleufBaseApp{
        let mut app=SleufBaseApp::default();app.project_name="Bronproject".into();app.project_path=Some("source.json".into());
        let mut trench=VirtualTrench::new("PS1".into(),Vertex{x:155000.,y:463000.,z:Some(3.)},Vertex{x:155010.,y:463000.,z:Some(4.)},1.).unwrap();
        trench.metadata=serde_json::json!({"unrelated":{"keep":true},"template_maaiveld_segments":{"start":{"color":"#123456","extra":"keep"}}});app.virtual_trenches.push(trench);app
    }
    fn wait(app:&mut SleufBaseApp,ctx:&egui::Context){let deadline=Instant::now()+Duration::from_secs(5);while app.ai_maaiveld_ui.busy(){assert!(Instant::now()<deadline);app.poll_ai_maaiveld(ctx);std::thread::sleep(Duration::from_millis(2));}}
    #[test]
    fn cancellation_restores_original_allocations_before_close_and_blocks_empty_save(){
        let mut app=app();app.dirty=true;let pointer=app.virtual_trenches.as_ptr();let generation=app.project_generation;
        let worker=Worker::with_processor(|_,_,control|loop{control.check()?;std::thread::sleep(Duration::from_millis(2));}).unwrap();let(_,rx)=mpsc::sync_channel(1);
        app.launch_ai_worker(Source::Virtual(0),worker,rx).unwrap();assert!(app.virtual_trenches.is_empty());
        let dir=tempfile::tempdir().unwrap();let unintended=dir.path().join("empty.json");assert!(!app.save_project_to_path(unintended.clone()));assert!(!unintended.exists());
        app.start_python_project_export(dir.path().join("other.klicproject"));assert!(!app.project_exchange_busy());
        let ctx=egui::Context::default();app.request_project_action(ProjectAction::Close,&ctx);assert!(!app.close_confirmed);wait(&mut app,&ctx);
        assert_eq!(app.virtual_trenches.as_ptr(),pointer);assert_eq!(app.project_generation,generation);assert_eq!(app.project_path,Some("source.json".into()));assert!(app.dirty);assert_eq!(app.pending_project_action,Some(ProjectAction::Close));
    }
    #[test]
    fn preview_requires_explicit_apply_preserves_metadata_and_rejects_stale_sources(){
        let mut app=app();let before=app.virtual_trenches[0].metadata.clone();let ctx=egui::Context::default();let(tx,rx)=mpsc::sync_channel(1);
        let worker=Worker::with_processor(move|_,_,_|{tx.send(Preview{image:image::RgbImage::new(2,2),result:ai::Result{text_by_key:[("1".into(),"A".into()),("2".into(),"B".into()),("3".into(),"C".into())].into(),raw_response:String::new()}}).unwrap();Ok(())}).unwrap();
        app.launch_ai_worker(Source::Virtual(0),worker,rx).unwrap();wait(&mut app,&ctx);assert_eq!(app.virtual_trenches[0].metadata,before);assert!(!app.dirty);assert!(app.ai_maaiveld_ui.review.is_some());
        app.ai_maaiveld_ui.selected=Some(Source::Raster(0));assert!(app.apply_ai_maaiveld().unwrap_err().contains("gekozen proefsleuf"));assert_eq!(app.virtual_trenches[0].metadata,before);app.ai_maaiveld_ui.selected=Some(Source::Virtual(0));
        app.virtual_trenches[0].label="Changed".into();assert!(app.apply_ai_maaiveld().unwrap_err().contains("gewijzigd"));assert_eq!(app.virtual_trenches[0].metadata,before);
        app.virtual_trenches[0].label="PS1".into();app.apply_ai_maaiveld().unwrap();let m=&app.virtual_trenches[0].metadata;
        assert_eq!(m["unrelated"],before["unrelated"]);assert_eq!(m["template_maaiveld_segments"]["start"]["color"],"#123456");assert_eq!(m["template_maaiveld_segments"]["start"]["extra"],"keep");
        for(key,text)in[("start","A"),("middle","B"),("end","C")]{assert_eq!(m["template_maaiveld_segments"][key]["text"],text);}assert!(app.dirty);
    }
    #[test]
    fn changed_project_retains_returned_sources_and_blocks_save_until_recovered(){
        let mut app=app();let worker=Worker::with_processor(|_,_,_|Ok(())).unwrap();let(_,rx)=mpsc::sync_channel(1);app.launch_ai_worker(Source::Virtual(0),worker,rx).unwrap();app.project_generation+=1;
        wait(&mut app,&egui::Context::default());assert!(app.ai_maaiveld_ui.recovered.is_some());assert!(app.project_sources_owned());let dir=tempfile::tempdir().unwrap();assert!(!app.save_project_to_path(dir.path().join("empty.json")));assert!(app.defer_for_ai(&ProjectAction::New));
    }
}
