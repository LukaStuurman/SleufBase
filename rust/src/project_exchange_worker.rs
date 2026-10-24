//! A bounded Python-project export worker. Ownership returns on success, error,
//! cancellation and caught processor panic; project vectors are never cloned.
use crate::{export_control::Control,legacy_project::{ProjectUiState,write_legacy_project_controlled},project::NativeProject};
use std::{path::PathBuf,sync::{Arc,mpsc::{self,Receiver,SyncSender,TryRecvError,TrySendError}}};
#[derive(Debug)]
pub struct Ready {pub project:NativeProject,pub path:PathBuf,pub result:Result<(),String>}
#[derive(Debug)]
pub struct StartFailure {pub project:NativeProject,pub message:String}
struct Work {project:NativeProject,path:PathBuf,control:Arc<Control>}
pub struct Worker {requests:SyncSender<Work>,results:Receiver<Ready>,active:Option<Arc<Control>>,closed:bool}
impl Worker {
    pub fn new()->Result<Self,String>{Self::with_processor(|project,path,control|{
        let ui=ProjectUiState::from_project(project)?.unwrap_or_default();
        control.check()?;write_legacy_project_controlled(project,path,&ui,control)
    })}
    #[doc(hidden)]
    pub fn with_processor<F>(processor:F)->Result<Self,String> where F:Fn(&NativeProject,&std::path::Path,&Control)->Result<(),String>+Send+'static{
        let (requests,rx)=mpsc::sync_channel::<Work>(1);let (tx,results)=mpsc::sync_channel(1);
        std::thread::Builder::new().name("sleufbase-python-project-export".into()).spawn(move||{
            while let Ok(work)=rx.recv(){
                let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
                    work.control.check()?;processor(&work.project,&work.path,&work.control)
                })).unwrap_or_else(|_|Err("Python-projectexport onverwacht gestopt; projectgegevens behouden".into()));
                work.control.finish();
                if tx.send(Ready{project:work.project,path:work.path,result}).is_err(){break;}
            }
        }).map_err(|e|format!("Python-exportworker starten: {e}"))?;
        Ok(Self{requests,results,active:None,closed:false})
    }
    pub fn start(&mut self,project:NativeProject,path:PathBuf)->Result<(),Box<StartFailure>>{
        if self.active.is_some()||self.closed{return Err(Box::new(StartFailure{project,message:"Python-projectexport is bezig of gestopt".into()}));}
        let control=Arc::new(Control::default());let work=Work{project,path,control:control.clone()};
        match self.requests.try_send(work){
            Ok(())=>{self.active=Some(control);Ok(())},
            Err(TrySendError::Full(work))=>Err(Box::new(StartFailure{project:work.project,message:"Python-projectexport is bezig".into()})),
            Err(TrySendError::Disconnected(work))=>{self.closed=true;Err(Box::new(StartFailure{project:work.project,message:"Python-projectexport is gestopt".into()}))}
        }
    }
    pub fn is_busy(&self)->bool{self.active.is_some()}
    pub fn is_closed(&self)->bool{self.closed}
    pub fn cancel(&self)->bool{self.active.as_ref().is_some_and(|c|c.cancel())}
    pub fn poll(&mut self)->Option<Ready>{match self.results.try_recv(){Ok(ready)=>{self.active=None;Some(ready)},Err(TryRecvError::Empty)=>None,Err(TryRecvError::Disconnected)=>{self.closed=true;None}}}
}
impl Drop for Worker{fn drop(&mut self){self.cancel();}}
#[cfg(test)]
mod tests{
    use super::*;
    use std::time::{Duration,Instant};
    fn wait(worker:&mut Worker)->Ready{let deadline=Instant::now()+Duration::from_secs(5);loop{if let Some(ready)=worker.poll(){return ready;}assert!(Instant::now()<deadline);std::thread::sleep(Duration::from_millis(2));}}
    #[test]
    fn worker_returns_same_source_allocation_after_panic_and_recovers(){
        let mut worker=Worker::with_processor(|_,_,_|panic!("injected export fault")).unwrap();
        let mut project=NativeProject::default();project.comments.push(crate::project::Comment{x:1.,y:2.,text:"keep".into()});let pointer=project.comments.as_ptr();
        worker.start(project,"new.klicproject".into()).ok().unwrap();let ready=wait(&mut worker);
        assert!(ready.result.unwrap_err().contains("behouden"));assert_eq!(ready.project.comments.as_ptr(),pointer);
        worker.start(ready.project,"other.klicproject".into()).ok().unwrap();assert!(wait(&mut worker).result.is_err());
    }
    #[test]
    fn worker_cancels_and_returns_busy_submission_without_copying(){
        let mut worker=Worker::with_processor(|_,_,control|loop{control.check()?;std::thread::sleep(Duration::from_millis(2));}).unwrap();
        worker.start(NativeProject::default(),"cancel.klicproject".into()).ok().unwrap();
        let mut other=NativeProject::default();other.comments.push(crate::project::Comment{x:1.,y:2.,text:"other".into()});let pointer=other.comments.as_ptr();
        let failure=worker.start(other,"busy.klicproject".into()).err().unwrap();assert_eq!(failure.project.comments.as_ptr(),pointer);
        assert!(worker.cancel());assert!(wait(&mut worker).result.unwrap_err().contains("geannuleerd"));
    }
    #[test]
    fn real_export_failure_returns_unchanged_project_and_never_overwrites(){
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("existing.klicproject");std::fs::write(&path,b"original").unwrap();
        let mut worker=Worker::new().unwrap();let mut project=NativeProject::default();project.name="keep".into();worker.start(project,path.clone()).ok().unwrap();
        let ready=wait(&mut worker);assert!(ready.result.is_err());assert_eq!(ready.project.name,"keep");assert_eq!(std::fs::read(path).unwrap(),b"original");
    }
}
