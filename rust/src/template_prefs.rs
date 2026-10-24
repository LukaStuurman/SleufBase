//! Durable, identity-based template layout preferences for Rust project v3.
//! Never serialize volatile vector indices: reopening or importing projects
//! may reorder loaded jobs/rasters. Missing/ambiguous sources are never guessed.
use crate::{
    geotiff::GeoTiffInfo, model::Dataset,
    template_project::{self, BackgroundKind, Options, Source},
    virtual_trench::VirtualTrench,
};
use serde::{Deserialize,Serialize};
use std::{collections::HashSet,path::PathBuf};

#[derive(Clone,Debug,PartialEq,Eq,Hash,Serialize,Deserialize)]
#[serde(tag="kind",rename_all="snake_case",deny_unknown_fields)]
enum SourceKey {
    Job {job_id:u64, source_path:PathBuf},
    Virtual {job_id:u64, label:String},
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedChoice {
    source:SourceKey,
    raster_path:Option<PathBuf>,
    include:bool,
    title:String,
    profile:bool,
    reverse:bool,
    comments:String,
    #[serde(default)]
    photos:Vec<crate::template_photo::Photo>,
}
fn default_paired()->bool{true}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplatePreferences {
    #[serde(default="crate::cadastral_overview::template_defaults")]
    pub overview_settings:crate::cadastral_overview::Settings,
    #[serde(default)]
    pub profile_style:crate::settings_compat::ProfilePreferences,
    #[serde(default,skip_serializing_if="crate::template_branding::Logos::is_empty")]
    pub logos:crate::template_branding::Logos,
    pub template:PathBuf,
    #[serde(default="default_paired")]
    pub paired:bool,
    pub project:String,
    pub drawn_by:String,
    #[serde(default)]
    pub font_directories:Vec<PathBuf>,
    pub dpi:u32,
    pub scale:f64,
    pub background:BackgroundKind,
    pub include_bgt:bool,
    pub include_cadastral:bool,
    pub include_cad:bool,
    choices:Vec<SavedChoice>,
}
fn key(source:Source,jobs:&[Dataset],trenches:&[VirtualTrench])->Option<SourceKey>{
    match source {
        Source::Job(i) => jobs.get(i).map(|j|SourceKey::Job {
            job_id:j.job_id,source_path:j.source_path.clone(),
        }),
        Source::Virtual(i)=>trenches.get(i).map(|v|SourceKey::Virtual {
            job_id:v.job_id,label:v.label.clone(),
        }),
    }
}
impl TemplatePreferences {
    pub fn capture(opts:&Options,jobs:&[Dataset],rasters:&[GeoTiffInfo],
        trenches:&[VirtualTrench])->Self
    {
        let choices=opts.choices.iter().filter_map(|item|{
            Some(SavedChoice {
                source:key(item.source,jobs,trenches)?,
                raster_path:item.raster.and_then(|idx|rasters.get(idx)).map(|r|r.path.clone()),
                include:item.include,title:item.title.clone(),
                profile:item.profile,reverse:item.reverse,comments:item.comments.clone(),
                photos:item.photos.clone(),
            })
        }).collect();
        Self {template:opts.template.clone(),paired:opts.paired,project:opts.project.clone(),
            overview_settings:opts.overview_settings.clone(),
            profile_style:opts.profile_style.clone(),
            logos:opts.logos.clone(),
            drawn_by:opts.drawn_by.clone(),font_directories:opts.font_directories.clone(),dpi:opts.dpi,scale:opts.scale,
            background:opts.background,include_bgt:opts.include_bgt,
            include_cadastral:opts.include_cadastral,include_cad:opts.include_cad,
            choices}
    }
    pub fn validate(&self)->Result<(),String>{
        self.profile_style.validate()?;
        crate::dxf_fonts::Catalog::validate_dirs(&self.font_directories)?;
        if self.choices.len()>20_000 {return Err("Te veel sjabloonvoorkeuren".into());}
        if self.project.len()>2048 || self.drawn_by.len()>2048
            || !self.scale.is_finite() || !(1. ..=100_000.).contains(&self.scale)
            || !(30..=600).contains(&self.dpi)
        {return Err("Ongeldige sjabloonvoorkeuren".into());}
        for item in &self.choices {
            if item.photos.len()>crate::template_photo::MAX_PHOTOS_PER_SLOT
                || item.photos.iter().any(|p|p.turns>3) {
                return Err("Te veel of ongeldig gedraaide projectfoto's".into());
            }
            if item.title.len()>256 || item.comments.len()>4096
                || item.title.chars().any(char::is_control)
                || item.comments.chars().any(|c|c.is_control()&&c!='\n')
            {return Err("Ongeldige opgeslagen sjabloontekst".into());}
        }
        Ok(())
    }
    pub fn restore(&self,jobs:&[Dataset],rasters:&[GeoTiffInfo],
        trenches:&[VirtualTrench])->Options
    {
        let mut options=Options {
            overview_settings:self.overview_settings.clone(),
            profile_style:self.profile_style.clone(),
            logos:self.logos.clone(),
            template:self.template.clone(),paired:self.paired,project:self.project.clone(),
            drawn_by:self.drawn_by.clone(),font_directories:self.font_directories.clone(),dpi:self.dpi,scale:self.scale,
            background:self.background,include_bgt:self.include_bgt,
            include_cadastral:self.include_cadastral,include_cad:self.include_cad,
            choices:template_project::suggest(jobs,rasters,trenches),
        };
        let candidates=std::mem::take(&mut options.choices);
        let keys=candidates.iter().map(|item|key(item.source,jobs,trenches))
            .collect::<Vec<_>>();
        let mut selected=HashSet::new();
        for old in &self.choices {
            let indices=keys.iter().enumerate().filter(|(_,value)|value.as_ref()==Some(&old.source))
                .map(|(i,_)|i).collect::<Vec<_>>();
            // The same identity can appear twice in legacy imports. Never
            // remap settings to an arbitrary duplicate job or virtual trench.
            if indices.len()!=1 || !selected.insert(indices[0]) {continue;}
            let mut next=candidates[indices[0]].clone();
            next.include=old.include;
            next.title=old.title.clone();
            next.profile=old.profile;
            next.reverse=old.reverse;
            next.comments=old.comments.clone();
            next.photos=old.photos.clone();
            if let Some(saved_path)=&old.raster_path {
                let found=rasters.iter().enumerate().filter(|(_,r)|&r.path==saved_path)
                    .map(|(i,_)|i).collect::<Vec<_>>();
                next.raster=if found.len()==1 {Some(found[0])} else {None};
            } else {next.raster=None;}
            options.choices.push(next);
        }
        for (i,item) in candidates.into_iter().enumerate() {
            if !selected.contains(&i) {options.choices.push(item);}
        }
        options
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{geo::GeoTransform,model::{Bounds,Feature,Geometry,Vertex}};
    fn job(id:u64,path:&str)->Dataset {
        Dataset{job_id:id,title:format!("PS{id}"),source_path:PathBuf::from(path),features:
            vec![Feature {object_name:"Kabel".into(),source_name:"KABEL".into(),
                geometry:Geometry::Point(Vertex{x:155000.,y:463000.,z:None}),
                attributes:Default::default()}]}
    }
    fn raster(path:&str)->GeoTiffInfo {
        GeoTiffInfo {path:PathBuf::from(path),width:16,height:16,
            transform:GeoTransform{a:1.,b:0.,c:154999.,d:0.,e:-1.,f:463010.},
            bounds:Bounds{min_x:154999.,min_y:462994.,max_x:155015.,max_y:463010.},
            epsg:Some(28992),kickthemap_job_id:None,kickthemap_job_title:String::new(),
            kickthemap_job_sidecar:None,metadata:Default::default()}
    }
    #[test]
    fn stable_identity_restores_sheet_order_and_moved_raster_indices() {
        let jobs=[job(1,"one.json"),job(2,"two.json")];
        let rasters=[raster("one.tif"),raster("two.tif")];
        let mut original=Options {choices:template_project::suggest(&jobs,&rasters,&[]),
            include_cad:false,..Default::default()};
        original.profile_style=crate::settings_compat::ProfilePreferences{marker_diameter:0.07,show_reference:false,use_custom_maaiveld:false,auto_fill_bgt:false,avoid_collisions:false,clip_markers:true,diameter_prefix:"DN".into()};
        original.logos=crate::template_branding::Logos{techbase:Some("company.png".into()),client:Some("client.jpg".into())};
        original.choices.swap(0,1);
        original.choices[0].raster=Some(1);
        original.choices[0].title="Custom PS2".into();
        original.choices[0].reverse=true;
        original.choices[0].photos.push(crate::template_photo::Photo {
            path:PathBuf::from("site-photo.png"),
            turns:3,area:crate::template_photo::Area::Map,
        });
        original.choices[1].raster=Some(0);
        original.choices[1].include=false;
        let prefs=TemplatePreferences::capture(&original,&jobs,&rasters,&[]);
        let encoded=serde_json::to_string(&prefs).unwrap();
        let prefs:TemplatePreferences=serde_json::from_str(&encoded).unwrap();
        assert!(prefs.validate().is_ok());
        let next=prefs.restore(&[jobs[1].clone(),jobs[0].clone()],
            &[rasters[1].clone(),rasters[0].clone()],&[]);
        assert_eq!(next.profile_style,original.profile_style);assert_eq!(next.logos,original.logos);
        assert_eq!(next.choices[0].title,"Custom PS2");
        assert_eq!(next.choices[0].raster,Some(0));
        assert!(next.choices[0].reverse);
        assert_eq!(next.choices[0].photos.len(),1);
        assert_eq!(next.choices[0].photos[0].turns,3);
        assert_eq!(next.choices[0].photos[0].area,crate::template_photo::Area::Map);
        assert_eq!(next.choices[0].photos[0].path,PathBuf::from("site-photo.png"));
        let mut legacy:serde_json::Value=serde_json::from_str(&encoded).unwrap();
        legacy.as_object_mut().unwrap().remove("profile_style");legacy.as_object_mut().unwrap().remove("logos");
        for choice in legacy["choices"].as_array_mut().unwrap() {
            choice.as_object_mut().unwrap().remove("photos");
        }
        let older:TemplatePreferences=serde_json::from_value(legacy).unwrap();
        assert_eq!(older.profile_style,Default::default());assert!(older.logos.is_empty());
        assert!(older.restore(&jobs,&rasters,&[]).choices.iter().all(|x|x.photos.is_empty()));
        assert_eq!(next.choices[1].raster,Some(1));
        assert!(!next.choices[1].include);
        assert!(!next.include_cad);
    }
    #[test]
    fn missing_source_is_skipped_and_missing_tiff_is_never_replaced_by_guess() {
        let jobs=[job(7,"seven.json")];
        let originals=[raster("A.tif")];
        let mut o=Options {choices:template_project::suggest(&jobs,&originals,&[]),
            ..Default::default()};
        o.choices[0].raster=Some(0);
        let prefs=TemplatePreferences::capture(&o,&jobs,&originals,&[]);
        let restored=prefs.restore(&jobs,&[raster("B.tif")],&[]);
        assert!(restored.choices[0].raster.is_none());
        assert!(prefs.restore(&[],&originals,&[]).choices.is_empty());
        assert_eq!(prefs.restore(&[job(7,"seven.json"),job(8,"new.json")],
            &originals,&[]).choices.len(),2);
    }
}
