//! Cancellable AHN work belongs to the open draft; closing the editor drops it.
use super::*;
use sleufbase_rust::virtual_map::{self,MapPoint};
use std::sync::{Arc,OnceLock,atomic::{AtomicBool,Ordering}};

pub(super) struct Job {
    cancel:Arc<AtomicBool>,
    axis:[Vertex;2],
    metadata:[serde_json::Value;2],
    rx:Receiver<Result<[Option<Option<f64>>;2],String>>,
}
impl Drop for Job {fn drop(&mut self){self.cancel.store(true,Ordering::Relaxed);}}
fn client()->Result<ahn::AhnClient,String>{
    static CLIENT:OnceLock<Result<ahn::AhnClient,String>>=OnceLock::new();
    CLIENT.get_or_init(ahn::AhnClient::new).clone()
}
fn eligible(point:Vertex,metadata:&serde_json::Value)->bool{
    match metadata.get("surface_mode").and_then(|v|v.as_str()) {
        Some("auto")=>true,None|Some("")=>point.z.is_none(),_=>false,
    }
}
impl Editor {
    pub(super) fn mark_endpoint_height(&mut self,start:bool){
        let(point,metadata)=if start{(self.start,&mut self.draft.start_metadata)}else{(self.end,&mut self.draft.end_metadata)};
        if !metadata.is_object(){*metadata=serde_json::json!({});}
        let object=metadata.as_object_mut().unwrap();
        object.remove("sleufbase_depth_meters");object.remove("surface_z");
        object.insert("surface_mode".into(),serde_json::json!(if point.z.is_some(){"manual"}else{"auto"}));
        object.insert("surface_status".into(),serde_json::json!(if point.z.is_some(){"manual"}else{"pending"}));
    }
    fn start_ahn(&mut self)->Result<(),String>{
        if self.ahn_job.is_some(){return Ok(());}
        let draft=self.build()?;
        if edit::is_measured(&draft){return Err("Gemeten MarXact-hoogtes blijven behouden".into());}
        let axis=[draft.start,draft.end];
        let metadata=[draft.start_metadata.clone(),draft.end_metadata.clone()];
        let wanted=[eligible(axis[0],&metadata[0]),eligible(axis[1],&metadata[1])];
        if !wanted.iter().any(|v|*v){return Err("Beide hoogtes zijn handmatig ingevuld; maak een hoogte leeg om AHN te gebruiken".into());}
        let client=client()?;
        let cancel=Arc::new(AtomicBool::new(false));let worker_cancel=cancel.clone();
        let(tx,rx)=mpsc::channel();
        std::thread::Builder::new().name("sleufbase-ahn".into()).spawn(move||{
            let result=(||{
                let mut heights=[None,None];
                for i in 0..2 {
                    if worker_cancel.load(Ordering::Relaxed){return Err("AHN-aanvulling geannuleerd".into());}
                    if wanted[i]{heights[i]=Some(client.ground_level(axis[i].x,axis[i].y,&worker_cancel)?);}
                }
                Ok(heights)
            })();
            let _=tx.send(result);
        }).map_err(|error|format!("AHN-taak kon niet starten: {error}"))?;
        self.ahn_job=Some(Job{cancel,axis,metadata,rx});self.error.clear();self.notice.clear();Ok(())
    }
    pub(super) fn poll_ahn(&mut self){
        let event=self.ahn_job.as_ref().map(|job|job.rx.try_recv());
        let result=match event {
            Some(Ok(result))=>result,
            Some(Err(TryRecvError::Disconnected))=>Err("AHN-taak is afgebroken; het concept is behouden".into()),
            _=>return,
        };
        let job=self.ahn_job.take().unwrap();
        if job.cancel.load(Ordering::Relaxed){self.notice="AHN-aanvulling geannuleerd; het concept is behouden".into();return;}
        let result=result.and_then(|heights|{
            if job.axis!=[self.start,self.end]||job.metadata!=[self.draft.start_metadata.clone(),self.draft.end_metadata.clone()] {
                return Err("De eindpunten zijn gewijzigd; vraag de AHN-hoogtes opnieuw op".into());
            }
            let mut candidate=self.build()?;
            for (target,height) in [MapPoint::Start,MapPoint::End].into_iter().zip(heights){
                if let Some(height)=height {virtual_map::apply_surface_height(&mut candidate,target,height)?;}
            }
            edit::check(&candidate)?;
            self.start=candidate.start;self.end=candidate.end;self.draft=candidate;
            Ok(heights.iter().flatten().filter(|value|value.is_some()).count())
        });
        match result {
            Ok(count)=>{self.error.clear();self.notice=format!("{count} AHN-hoogtes ontvangen. Klik op Opslaan in project om het concept te bewaren.");}
            Err(error)=>self.error=error,
        }
    }
    pub(super) fn ahn_controls(&mut self,ui:&mut egui::Ui,ctx:&egui::Context){
        if let Some(job)=&self.ahn_job {
            ui.horizontal(|ui|{ui.spinner();ui.label("AHN-hoogtes voor de eindpunten ophalen…");if ui.button("Annuleren").clicked(){job.cancel.store(true,Ordering::Relaxed);}});
            ctx.request_repaint_after(Duration::from_millis(100));
        }else if !edit::is_measured(&self.original) && ui.button("AHN-hoogtes voor deze sleuf aanvullen").clicked(){
            if let Err(error)=self.start_ahn(){self.error=error;}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn draft()->Editor {
        let mut trench=VirtualTrench::new("AHN concept".into(),Vertex{x:155000.,y:463000.,z:None},Vertex{x:155010.,y:463000.,z:None},0.6).unwrap();
        trench.objects.push(TrenchPoint{metadata:serde_json::json!({"surface_mode":"start","sleufbase_depth_meters":1.}),role:"object".into(),object_name:"LS".into(),source_name:"LS".into(),position:Vertex{x:155002.,y:463000.,z:None},attributes:Default::default()});
        Editor::new(0,&trench)
    }
    fn pending(editor:&mut Editor)->mpsc::Sender<Result<[Option<Option<f64>>;2],String>>{
        let(tx,rx)=mpsc::channel();
        editor.ahn_job=Some(Job{cancel:Arc::new(AtomicBool::new(false)),axis:[editor.start,editor.end],metadata:[editor.draft.start_metadata.clone(),editor.draft.end_metadata.clone()],rx});tx
    }
    #[test]
    fn response_updates_only_draft_and_preserves_object_depth(){
        let mut editor=draft();let original=serde_json::to_value(&editor.original).unwrap();
        pending(&mut editor).send(Ok([Some(Some(4.)),Some(Some(5.))])).unwrap();editor.poll_ahn();
        assert_eq!(editor.start.z,Some(4.));assert_eq!(editor.end.z,Some(5.));assert_eq!(editor.draft.objects[0].position.z,Some(3.));
        assert_eq!(serde_json::to_value(&editor.original).unwrap(),original);
    }
    #[test]
    fn cancelled_stale_failed_or_disconnected_work_keeps_draft(){
        for mode in 0..4 {
            let mut editor=draft();let tx=pending(&mut editor);
            match mode {0=>editor.ahn_job.as_ref().unwrap().cancel.store(true,Ordering::Relaxed),1=>editor.start.x+=1.,_=>{}}
            let before=serde_json::to_value(editor.build().unwrap()).unwrap();
            if mode==3{drop(tx);}else{tx.send(if mode==2{Err("offline".into())}else{Ok([Some(Some(4.)),Some(Some(5.))])}).unwrap();}
            editor.poll_ahn();assert!(editor.ahn_job.is_none());assert_eq!(serde_json::to_value(editor.build().unwrap()).unwrap(),before);
        }
    }
    #[test]
    fn manual_height_protects_metadata_and_closing_cancels_worker(){
        let mut editor=draft();editor.start.z=Some(8.);editor.mark_endpoint_height(true);
        assert!(!eligible(editor.start,&editor.draft.start_metadata));
        let _tx=pending(&mut editor);let cancel=editor.ahn_job.as_ref().unwrap().cancel.clone();drop(editor);assert!(cancel.load(Ordering::Relaxed));
    }
}
