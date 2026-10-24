//! Durable world-space camera and identity-based project layer selection.
use crate::{legacy_project::ProjectUiState,project::NativeProject};
use serde::{Deserialize,Serialize};
use std::path::PathBuf;
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Camera {pub center:[f64;2],pub meters_per_pixel:f64}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum BackgroundProvider {PdokAerial,OpenStreetMap,Cadastral,Cyclomedia,Unknown}
impl BackgroundProvider {
    pub fn from_label(label:&str)->Self{match label.trim(){
        "PDOK luchtfoto"|"pdok"|"Luchtfoto NL actueel (8 cm, deels 5 cm)"|"PDOK Luchtfoto NL actueel (8 cm, deels 5 cm)"=>Self::PdokAerial,
        "OpenStreetMap"|"osm"=>Self::OpenStreetMap,
        "PDOK kadastrale kaart"|"cadastral"=>Self::Cadastral,
        "Cyclomedia Luchtfoto NL (nieuwste jaar / laagste cm)"|"cyclomedia_aerial"=>Self::Cyclomedia,
        _=>Self::Unknown,
    }}
    pub fn label(self)->&'static str{match self{Self::PdokAerial=>"PDOK Luchtfoto NL actueel (8 cm, deels 5 cm)",Self::OpenStreetMap=>"OpenStreetMap",Self::Cadastral=>"PDOK kadastrale kaart",Self::Cyclomedia=>"Cyclomedia Luchtfoto NL (nieuwste jaar / laagste cm)",Self::Unknown=>"Onbekende achtergrondbron"}}
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(tag="kind",rename_all="snake_case",deny_unknown_fields)]
pub enum TiffSelection {Raster{path:PathBuf},Virtual{job_id:u64,label:String}}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(default,deny_unknown_fields)]
pub struct ProjectView {
    pub camera:Option<Camera>,pub opacity_percent:f64,pub use_background:bool,
    pub background_source:String,pub selected_tiff:Option<TiffSelection>,pub selected_dxf:Option<PathBuf>,
}
impl Default for ProjectView {
    fn default()->Self{let legacy=ProjectUiState::default();Self{camera:None,opacity_percent:legacy.opacity_percent,use_background:legacy.use_background,background_source:legacy.background_source,selected_tiff:None,selected_dxf:None}}
}
fn unique<T>(items:impl Iterator<Item=T>)->Option<T>{let mut items=items;let result=items.next()?;items.next().is_none().then_some(result)}
impl ProjectView {
    /// Cyclomedia was explicitly removed from the Rust product. Preserve the
    /// original archive bytes, but migrate its old active preference to PDOK.
    pub fn normalize_removed_provider(&mut self){if self.provider()==BackgroundProvider::Cyclomedia{self.background_source=BackgroundProvider::PdokAerial.label().into();}}
    pub fn provider(&self)->BackgroundProvider{BackgroundProvider::from_label(&self.background_source)}
    pub fn validate(&self)->Result<(),String>{
        let mut ui=ProjectUiState::default();ui.opacity_percent=self.opacity_percent;ui.background_source=self.background_source.clone();
        if let Some(camera)=&self.camera{ui.center_x=camera.center[0];ui.center_y=camera.center[1];ui.meters_per_pixel=camera.meters_per_pixel;}ui.validate()?;
        if self.selected_dxf.as_ref().is_some_and(|p|p.as_os_str().len()>32768)||match &self.selected_tiff{Some(TiffSelection::Raster{path})=>path.as_os_str().len()>32768,Some(TiffSelection::Virtual{label,..})=>label.len()>32768,None=>false}{return Err("Ongeldige projectlaagselectie".into());}Ok(())
    }
    /// Resolve original Python indices before virtual and raster arrays split.
    pub fn from_legacy(project:&NativeProject)->Result<Option<Self>,String>{
        let Some(ui)=ProjectUiState::from_archive(project)? else{return Ok(None)};
        let archive=project.legacy_archive.as_ref().unwrap();
        let selected_tiff=ui.selected_tiff_index.and_then(|index|{
            let old=archive.original["tiffs"].get(index)?;
            if old["type"]=="virtual_trench"{
                let trench=unique(project.virtual_trenches.iter().filter(|t|t.metadata["_sleufbase_python_tiff_index"].as_u64()==Some(index as u64)))?;
                Some(TiffSelection::Virtual{job_id:trench.job_id,label:trench.label.clone()})
            }else{
                let path=crate::legacy_project::asset_path(&archive.source_path,old["path"].as_str()?);
                unique(project.rasters.iter().filter(|r|r.path==path)).map(|r|TiffSelection::Raster{path:r.path.clone()})
            }
        });
        let selected_dxf=ui.selected_dxf_index.and_then(|index|{
            let path=crate::legacy_project::asset_path(&archive.source_path,archive.original["dxfs"].get(index)?["path"].as_str()?);
            unique(project.overlays.iter().filter(|o|o.path==path)).map(|o|o.path.clone())
        });
        let mut view=Self{camera:Some(Camera{center:[ui.center_x,ui.center_y],meters_per_pixel:ui.meters_per_pixel}),opacity_percent:ui.opacity_percent,use_background:ui.use_background,background_source:ui.background_source,selected_tiff,selected_dxf};view.normalize_removed_provider();view.validate()?;Ok(Some(view))
    }
    pub fn selected_virtual_index(&self,project:&NativeProject)->Option<usize>{
        match &self.selected_tiff{Some(TiffSelection::Virtual{job_id,label})=>unique(project.virtual_trenches.iter().enumerate().filter(|(_,t)|t.job_id==*job_id&&t.label==*label).map(|(i,_)|i)),_=>None}
    }
    /// Python export order is virtual trenches, then loaded rasters, then archive-only layers.
    pub fn for_export(&self,project:&NativeProject)->Result<ProjectUiState,String>{
        self.validate()?;let mut ui=ProjectUiState::default();
        if let Some(camera)=&self.camera{ui.center_x=camera.center[0];ui.center_y=camera.center[1];ui.meters_per_pixel=camera.meters_per_pixel;}
        ui.opacity_percent=self.opacity_percent;ui.use_background=self.use_background;ui.background_source=self.background_source.clone();
        ui.selected_tiff_index=match &self.selected_tiff{
            Some(TiffSelection::Virtual{..})=>self.selected_virtual_index(project),
            Some(TiffSelection::Raster{path})=>unique(project.rasters.iter().enumerate().filter(|(_,r)|r.path==*path).map(|(i,_)|project.virtual_trenches.len()+i)),None=>None};
        ui.selected_dxf_index=self.selected_dxf.as_ref().and_then(|path|unique(project.overlays.iter().enumerate().filter(|(_,o)|o.path==*path).map(|(i,_)|i)));Ok(ui)
    }
}
