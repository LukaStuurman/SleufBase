//! Original Gemma marker geometry: local job/virtual points, fresh BRT and BGT
//! context, base-profile deck-band injection, final profile, then map rotation.
use crate::{ai_maaiveld::Point,export_control::Control,profile_context::{self,Kind,Paths},project::NativeProject,settings_compat::MigratedSettings,Bounds,Dataset,Vertex};
use std::{borrow::Cow,sync::atomic::AtomicBool};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Source {Raster(usize),Virtual(usize)}
pub struct Prepared {
    pub points:Vec<Point>,pub layer_bounds:Bounds,pub context_bounds:Bounds,
    pub final_axis:[Vertex;2],pub bands_added:usize,pub context_failures:Vec<String>,
}
pub fn prepare(project:&NativeProject,source:Source,settings:&MigratedSettings,control:&Control)->Result<Prepared,String>{
    control.check()?;
    let client=std::sync::Arc::new(std::sync::Mutex::new(profile_context::Client::new()?));
    prepare_with(project,source,settings,control,|kind,bounds,cancel|{
        let client=client.clone();
        crate::cadastral::cancellable_request(cancel,move|stop|client.lock().map_err(|_|"Profielcontext is niet beschikbaar".to_owned())?.fetch(kind,bounds,&stop))?
    })
}
pub fn prepare_with(project:&NativeProject,source:Source,settings:&MigratedSettings,control:&Control,mut fetch:impl FnMut(Kind,Bounds,&AtomicBool)->Result<Paths,String>)->Result<Prepared,String>{
    control.check()?;let cancel=control.cancellation_flag();
    let (mut dataset,forced_start,layer_bounds,metadata,virtual_index)=resolve(project,source,cancel)?;
    let context_bounds=profile_context::padded(layer_bounds)?;
    let mut failures=Vec::new();
    // Original failures are independent: missing roads must not suppress BGT.
    let mut context=|kind|match fetch(kind,context_bounds,cancel){Ok(paths)=>paths,Err(error)=>{failures.push(error);std::sync::Arc::new(Vec::new())}};
    let roads=context(Kind::Roads);control.check()?;let terrain=context(Kind::Terrain);control.check()?;
    let options=crate::profile::Options{roads:&roads,terrain_boundaries:&terrain,forced_start,marker_scale:settings.export_preferences.template_cross_section_marker_diameter,..Default::default()};
    let base=crate::profile::build_with_cancel(&dataset,&settings.object_rules,options,cancel)?.ok_or("Voor deze proefsleuf kon geen dwarsprofiel met geldige eindpunthoogtes worden opgebouwd")?;
    let bands=virtual_index.and_then(|i|project.virtual_trenches.get(i)).map(|t|t.dekband_rows.as_slice());
    let parsed;
    let bands=if let Some(rows)=bands.filter(|rows|!rows.is_empty()){rows}else{parsed=crate::virtual_trench::dekband_rows_from_metadata(metadata);&parsed};
    let before=dataset.features.len();
    if !bands.is_empty(){crate::virtual_trench::append_dekband_rows_for_profile(bands,dataset.to_mut(),&base,cancel)?;}
    let profile=crate::profile::build_with_cancel(&dataset,&settings.object_rules,options,cancel)?.ok_or("Voor deze proefsleuf kon geen definitief dwarsprofiel worden opgebouwd")?;
    let a=profile.start;let b=profile.end;
    let raw=[[a.x,a.y],[(a.x+b.x)/2.,(a.y+b.y)/2.],[b.x,b.y]];
    let mut points=Vec::with_capacity(3);
    for (index,(key,label)) in [("start","Beginpunt"),("middle","Middenpunt"),("end","Eindpunt")].into_iter().enumerate(){
        let displayed=match source{Source::Raster(i)=>crate::raster_points::display(&project.rasters[i],raw[index])?,Source::Virtual(i)=>crate::profile_reference::Reference{raw:raw[index],chainage:0.,description:String::new()}.display(metadata,crate::virtual_map::raster_center(&project.virtual_trenches[i]))?};
        points.push(Point{key:key.into(),label:label.into(),x:displayed[0],y:displayed[1]});
    }
    control.check()?;
    Ok(Prepared{points,layer_bounds,context_bounds,final_axis:[a,b],bands_added:dataset.features.len()-before,context_failures:failures})
}
type Resolved<'a>=(Cow<'a,Dataset>,Option<[f64;2]>,Bounds,&'a serde_json::Value,Option<usize>);
fn resolve<'a>(project:&'a NativeProject,source:Source,cancel:&AtomicBool)->Result<Resolved<'a>,String>{
    match source{
        Source::Virtual(i)=>{let t=project.virtual_trenches.get(i).ok_or("De gekozen virtuele sleuf is niet meer beschikbaar")?;t.validate()?;Ok((Cow::Owned(t.to_base_dataset_with_cancel(t.job_id,true,cancel)?),Some([t.start.x,t.start.y]),crate::virtual_raster::layout(t,1.)?.bounds,&t.metadata,Some(i)))},
        Source::Raster(i)=>{
            let r=project.rasters.get(i).ok_or("De gekozen TIFF is niet meer beschikbaar")?;
            if let Some(path)=r.metadata.get("kickthemap_job_features_path").and_then(serde_json::Value::as_str).filter(|s|!s.is_empty()){
                let path=std::path::PathBuf::from(path);let path=if path.is_absolute(){path}else{r.path.parent().unwrap_or(std::path::Path::new(".")).join(path)};
                if path.is_file(){let bytes=crate::input::read(&path,256*1024*1024,"Lokale profieljob",cancel)?;let text=std::str::from_utf8(&bytes).map_err(|_|"Lokale profieljob bevat geen geldige UTF-8")?;let mut dataset=crate::parse::parse_job_features_with_cancel(text,r.kickthemap_job_id.unwrap_or(0),&r.kickthemap_job_title,cancel)?;dataset.source_path=path;return Ok((Cow::Owned(dataset),crate::raster_points::start_in(&r.metadata),r.bounds,&r.metadata,None));}
            }
            let matches=project.datasets.iter().filter(|d|r.kickthemap_job_id.is_some_and(|id|id>0&&d.job_id==id)).collect::<Vec<_>>();
            if matches.len()>1{return Err("De TIFF-jobkoppeling is niet eenduidig".into());}
            if let Some(dataset)=matches.first(){return Ok((Cow::Borrowed(*dataset),crate::raster_points::start_in(&r.metadata),r.bounds,&r.metadata,None));}
            let mut matches=crate::template_project::suggest(&project.datasets,&project.rasters,&project.virtual_trenches).into_iter().filter(|c|c.raster==Some(i));
            let choice=matches.next().ok_or("Koppel deze TIFF eerst aan een profieljob of kies de bijbehorende virtuele sleuf")?;
            if matches.next().is_some(){return Err("Deze TIFF heeft meerdere mogelijke profielen; maak de jobkoppeling eenduidig".into());}
            match choice.source{
                crate::template_project::Source::Job(j)=>Ok((Cow::Borrowed(&project.datasets[j]),crate::raster_points::start_in(&r.metadata),r.bounds,&r.metadata,None)),
                crate::template_project::Source::Virtual(j)=>{let t=&project.virtual_trenches[j];Ok((Cow::Owned(t.to_base_dataset_with_cancel(t.job_id,true,cancel)?),Some([t.start.x,t.start.y]),r.bounds,&r.metadata,Some(j)))},
            }
        }
    }
}
