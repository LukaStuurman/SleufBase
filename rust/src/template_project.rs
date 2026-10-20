//! Build complete native template exports directly from the open Rust project.
//! Keep this independent from egui so matching, validation and output can be tested.
use crate::{
    bgt::BgtLayer,
    cadastral::CadastralLayer,
    export_control::Control,
    dxf_read::DxfOverlay,
    geotiff::GeoTiffInfo,
    map_export::{Background, Source as MapSource, Stroke},
    map_page,
    model::{Bounds, Dataset, Geometry},
    template_export::{self, Profile, Slot},
    template_raster,
    virtual_trench::VirtualTrench,
    Rule,
};
use std::{collections::HashSet, path::PathBuf};

pub const MAX_SLOTS: usize = 128;
const MAX_MAP_POINTS: usize = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    Job(usize),
    Virtual(usize),
}
#[derive(Clone, Debug)]
pub struct Choice {
    pub source: Source,
    pub raster: Option<usize>,
    pub include: bool,
    pub title: String,
    pub profile: bool,
    pub reverse: bool,
    pub comments: String,
    pub photos: Vec<crate::template_photo::Photo>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all="snake_case")]
pub enum BackgroundKind {
    Blank,
    Pdok,
}
pub struct Options {
    pub template: PathBuf,
    pub project: String,
    pub drawn_by: String,
    pub dpi: u32,
    pub scale: f64,
    pub background: BackgroundKind,
    pub include_bgt: bool,
    pub include_cadastral: bool,
    pub include_cad: bool,
    pub choices: Vec<Choice>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            template: PathBuf::new(),
            project: String::new(),
            drawn_by: String::new(),
            dpi: 150,
            scale: 225.,
            background: BackgroundKind::Blank,
            include_bgt: true,
            include_cadastral: true,
            include_cad: true,
            choices: Vec::new(),
        }
    }
}
/// Match exact job IDs first, then unique RD geometry coverage only when the
/// source sidecar does not point to a different job. Uncertain matches stay unassigned.
pub fn suggest(datasets: &[Dataset], rasters: &[GeoTiffInfo], trenches: &[VirtualTrench]) -> Vec<Choice> {
    let mut choices = Vec::with_capacity(datasets.len()+trenches.len());
    for (i, job) in datasets.iter().enumerate() {
        choices.push(Choice {
            source: Source::Job(i),
            raster: crate::template_match::for_job(job,rasters).raster,
            include: true, title: job.title.clone(), profile: true,
            reverse: false, comments: String::new(), photos:Vec::new(),
        });
    }
    for (i, trench) in trenches.iter().enumerate() {
        choices.push(Choice {
            source: Source::Virtual(i),
            raster: crate::template_match::for_trench(trench,rasters).raster,
            include: true, title: trench.label.clone(), profile: true,
            reverse: false, comments: String::new(), photos:Vec::new(),
        });
    }
    choices
}
fn checked_bounds(bounds: Bounds) -> Result<Bounds, String> {
    if ![bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y]
        .into_iter().all(|v| v.is_finite() && v.abs() < 1e9)
        || bounds.max_x <= bounds.min_x || bounds.max_y <= bounds.min_y
    {
        return Err("Ongeldige GeoTIFF-kaartgrenzen".into());
    }
    Ok(bounds)
}
/// Clip every visible path segment to the raster viewport. Outside-only
/// lines must not disappear when they cross the viewport. Avoid sending huge
/// far-offscreen coordinates to the bounded rasterizer.
fn append_stroke(
    overlays: &mut Vec<Stroke>,
    total: &mut usize,
    bounds: Bounds,
    points: Vec<[f64; 2]>,
    color: [u8; 3],
    highlight: bool,
) -> Result<(), String> {
    if points.len()<2 {return Ok(());}
    if points.iter().flatten().any(|v|!v.is_finite()) {
        return Err("Kaartpad bevat niet-eindige RD-coördinaten".into());
    }
    for edge in points.windows(2) {
        if let Some((from,to))=crate::template_cad::clip(edge[0],edge[1],bounds) {
            *total=total.checked_add(2).ok_or("Te veel kaartgeometrie")?;
            if *total>MAX_MAP_POINTS {
                return Err("Kaart bevat te veel overlaypunten voor deze export".into());
            }
            overlays.push(Stroke{
                points:vec![from,to],color,selected:false,highlighted:highlight,
            });
        }
    }
    Ok(())
}
/// The selection is validated before any DXF or image is written. The existing
/// atomic template-export transaction remains responsible for final publication.
pub fn build(
    opts: &Options,
    jobs: &[Dataset],
    rasters: &[GeoTiffInfo],
    trenches: &[VirtualTrench],
    bgt: &[BgtLayer],
    cadastral: Option<&CadastralLayer>,
    rules: &[Rule],
    control: &Control,
) -> Result<template_export::Request, String> {
    build_with_cad(opts, jobs, rasters, trenches, bgt, cadastral, &[], rules, control)
}
/// Build using the current CAD overlay scene, preserving drawing-layer visibility
/// and respecting linework from nested INSERT/MINSERT blocks.
pub fn build_with_cad(
    opts: &Options,
    jobs: &[Dataset],
    rasters: &[GeoTiffInfo],
    trenches: &[VirtualTrench],
    bgt: &[BgtLayer],
    cadastral: Option<&CadastralLayer>,
    cad: &[DxfOverlay],
    rules: &[Rule],
    control: &Control,
) -> Result<template_export::Request, String> {
    control.check()?;
    if !opts.template.is_file() {
        return Err("Kies een bestaand DXF-sjabloon".into());
    }
    if !(30..=600).contains(&opts.dpi)
        || !opts.scale.is_finite() || !(1. ..=100_000.).contains(&opts.scale)
    {
        return Err("Ongeldige kaart-DPI of schaal".into());
    }
    let enabled = opts.choices.iter().filter(|c| c.include).collect::<Vec<_>>();
    if enabled.is_empty() { return Err("Selecteer minstens één proefsleufvak".into()); }
    if enabled.len() > MAX_SLOTS { return Err("Te veel proefsleufvakken voor één export".into()); }
    let mut seen = HashSet::new();
    let mut datasets = Vec::with_capacity(enabled.len());
    let mut slots = Vec::with_capacity(enabled.len());
    // Full BGT geometry is retained once per export; only visible source lines
    // needed near a slot become map overlays.
    let mut roads: Vec<Vec<crate::profile::XY>> = Vec::new();
    let mut terrain: Vec<Vec<crate::profile::XY>> = Vec::new();
    if opts.include_bgt {
        for layer in bgt.iter().filter(|layer| layer.visible) {
            control.check()?;
            let to = if matches!(layer.kind, crate::bgt::BgtKind::Wegdeel) {
                &mut roads
            } else { &mut terrain };
            for path in &layer.paths {
                if path.len() > MAX_MAP_POINTS || to.len() >= 10_000 { break; }
                to.push(path.iter().map(|&(x,y)| [x,y]).collect());
            }
        }
    }
    for choice in enabled {
        control.check()?;
        crate::template_photo::validate(&choice.photos)?;
        if !seen.insert(choice.source) { return Err("Dubbele proefsleufselectie".into()); }
        let raster = choice.raster.map(|index|
            rasters.get(index).ok_or("GeoTIFF-verwijzing buiten de projectlijst")
        ).transpose()?;
        if raster.is_none() && matches!(choice.source,Source::Job(_)){
            return Err(format!("Voor '{}' ontbreekt een GeoTIFF; koppel deze in het exportvenster",choice.title));
        }
        if let Some(raster)=raster {
            if raster.epsg.is_some_and(|epsg|epsg!=28992){
                return Err(format!("GeoTIFF '{}' staat niet in EPSG:28992",raster.path.display()));
            }
            if !raster.path.is_file(){
                return Err(format!("GeoTIFF niet gevonden: {}",raster.path.display()));
            }
        }
        // A virtual or measured MarXact trench can export its own transparent
        // synthetic TIFF pane; it does not need an unrelated KickTheMap job.
        let virtual_source=match (choice.source,raster){
            (Source::Virtual(i),None)=>{
                let trench=trenches.get(i).ok_or("Virtuele sleuf niet meer beschikbaar")?;
                Some(crate::virtual_raster::Source{trench:trench.clone(),quality:1.,reverse:choice.reverse})
            },
            _=>None,
        };
        let bounds=match (raster,virtual_source.as_ref()){
            (Some(info),_)=>checked_bounds(info.bounds)?,
            (None,Some(source))=>checked_bounds(
                crate::virtual_raster::layout(&source.trench,source.quality)?.bounds
            )?,
            _=>return Err("Geen proefsleufrasterbron".into()),
        };
        let (dataset, forced_start, axis, extra_polyline) = match choice.source {
            Source::Job(i) => {
                let job = jobs.get(i).ok_or("Job bestaat niet meer in dit project")?;
                (job.clone(), None, None, Vec::new())
            }
            Source::Virtual(i) => {
                let trench = trenches.get(i).ok_or("Virtuele proefsleuf bestaat niet meer")?;
                trench.validate()?;
                if choice.profile && (trench.start.z.is_none() || trench.end.z.is_none()) {
                    return Err(format!(
                        "Dwarsprofiel voor '{}' heeft een gemeten Z-hoogte van het begin- en eindpunt nodig. Vul de hoogtes in de virtuele sleufeditor in of schakel profiel tekenen uit",
                        trench.label
                    ));
                }
                let outline = trench.measured_boundary().into_iter()
                    .map(|(x,y)| [x,y]).collect::<Vec<_>>();
                (trench.to_dataset(trench.job_id, true),
                    Some([trench.start.x, trench.start.y]),
                    Some([[trench.start.x,trench.start.y],[trench.end.x,trench.end.y]]),
                    outline)
            }
        };
        let label = if choice.title.trim().is_empty() {
            dataset.title.clone()
        } else { choice.title.trim().to_owned() };
        if label.is_empty() || label.chars().any(char::is_control) || label.len() > 256
            || choice.comments.chars().any(|c| c.is_control() && c != '\n')
            || choice.comments.len() > 4096
        {
            return Err("Ongeldige proefsleuftitel of opmerking".into());
        }
        let mut overlays = Vec::new();
        let mut points = 0usize;
        for feature in &dataset.features {
            if let Geometry::Polyline(vertices) = &feature.geometry {
                append_stroke(&mut overlays, &mut points, bounds,
                    vertices.iter().map(|p| [p.x,p.y]).collect(),
                    [249,115,22], true)?;
            }
        }
        if !extra_polyline.is_empty() {
            append_stroke(&mut overlays, &mut points, bounds, extra_polyline,
                [249,115,22], true)?;
        }
        if opts.include_bgt {
            for path in &roads {
                control.check()?;
                append_stroke(&mut overlays, &mut points, bounds, path.clone(), [109,119,130], false)?;
            }
            for path in &terrain {
                control.check()?;
                append_stroke(&mut overlays, &mut points, bounds, path.clone(), [98,145,83], false)?;
            }
        }
        if opts.include_cadastral {
            if let Some(cad) = cadastral.filter(|layer|layer.visible) {
                for l in &cad.linework {
                    for path in &l.paths {
                        control.check()?;
                        append_stroke(&mut overlays, &mut points, bounds,
                            path.iter().map(|&(x,y)| [x,y]).collect(), [140,80,145], false)?;
                    }
                }
            }
        }
        if opts.include_cad {
            crate::template_cad::append_visible(cad, bounds, &mut overlays, control)?;
            if overlays.iter().map(|stroke|stroke.points.len()).sum::<usize>()>MAX_MAP_POINTS {
                return Err("Kaart bevat te veel gecombineerde CAD-, BGT- en Kadasterpunten".into());
            }
        }
        let local_index = datasets.len();
        datasets.push(dataset);
        slots.push(Some(Slot {
            bounds, label: format!("PS{:03}", local_index + 1),
            title: label, address: None, comments: choice.comments.clone(),
            tiff_png: PathBuf::new(),
            tiff_source: raster.map(|raster|template_raster::Source {
                path: raster.path.clone(), transform: raster.transform,
                reverse_orientation: choice.reverse, axis,
            }),
            virtual_source,
            map_png: PathBuf::new(),
            map_source: Some(MapSource {
                settings: map_page::Settings { dpi: opts.dpi, scale: opts.scale },
                background: match opts.background {
                    BackgroundKind::Blank => Background::Solid { color: [250,250,250,255] },
                    BackgroundKind::Pdok => Background::Pdok,
                },
                overlays, ..MapSource::default()
            }),
            profile: choice.profile.then_some(Profile {
                dataset: local_index, forced_start, reverse: choice.reverse,
                ..Profile::default()
            }),
            photos:choice.photos.clone(),
        }));
    }
    Ok(template_export::Request {
        template: opts.template.clone(), slots, datasets,
        rules: rules.to_vec(), roads, terrain_boundaries: terrain,
        surfaces: Vec::new(), layers: Vec::new(), geometry: Vec::new(),
        settings: template_export::Settings {
            project: (!opts.project.trim().is_empty()).then(||opts.project.trim().to_owned()),
            drawn_by: opts.drawn_by.trim().to_owned(),
            ..template_export::Settings::default()
        },
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::{Feature, Vertex}, geo::GeoTransform};
    fn sample_job(id:u64) -> Dataset {
        Dataset {job_id:id,title:format!("PS-{id}"),source_path:PathBuf::new(),
            features:vec![Feature {object_name:"Test".into(),source_name:"KABEL".into(),
                geometry:Geometry::Polyline(vec![
                    Vertex{x:155000.,y:463000.,z:Some(-1.)},
                    Vertex{x:155005.,y:463000.,z:Some(-1.)}]),
                attributes:Default::default()}]}
    }
    fn raster(id:Option<u64>) -> GeoTiffInfo {
        GeoTiffInfo {path:PathBuf::from("test.tif"),width:100,height:100,
            transform:GeoTransform{a:0.05,b:0.,c:154999.,d:0.,e:-0.05,f:463001.},
            bounds:Bounds{min_x:154999.,min_y:462996.,max_x:155004.,max_y:463001.},
            epsg:Some(28992),kickthemap_job_id:id,kickthemap_job_title:String::new(),
            kickthemap_job_sidecar:None}
    }
    #[test]
    fn exact_job_id_matching_and_ambiguity_are_not_guessed() {
        let jobs = [sample_job(17),sample_job(18)];
        let r = [raster(Some(18)),raster(Some(17))];
        let proposed = suggest(&jobs,&r,&[]);
        assert_eq!(proposed[0].raster,Some(1));
        assert_eq!(proposed[1].raster,Some(0));
        let ambiguous = [raster(Some(17)),raster(Some(17))];
        let choices = suggest(&jobs,&ambiguous,&[]);
        assert_eq!(choices[0].raster,None);
        assert_eq!(choices[1].raster,None);
        // A single image with no job-ID is not sufficient when its bounds
        // do not cover the actual proefsleuf geometry.
        assert_eq!(suggest(&[sample_job(1)], &[raster(None)], &[])[0].raster,None);
    }
    #[test]
    fn offscreen_endpoints_crossing_the_tiff_are_exported_as_clipped_lines() {
        let bounds=Bounds{min_x:0.,min_y:0.,max_x:10.,max_y:10.};
        let mut strokes=Vec::new();
        let mut count=0;
        append_stroke(&mut strokes,&mut count,bounds,
            vec![[-100.,5.],[100.,5.]],[10,20,30],false).unwrap();
        assert_eq!(strokes.len(),1);
        assert_eq!(count,2);
        assert_eq!(strokes[0].points,vec![[0.,5.],[10.,5.]]);
        append_stroke(&mut strokes,&mut count,bounds,
            vec![[-100.,50.],[100.,50.]],[10,20,30],false).unwrap();
        assert_eq!(strokes.len(),1);
    }
    #[test]
    fn validation_before_export_never_requires_an_image_decode() {
        let jobs=[sample_job(17)];
        let mut opts=Options::default();
        opts.choices=suggest(&jobs,&[raster(Some(17))],&[]);
        assert!(build(&opts,&jobs,&[raster(Some(17))],&[],&[],None,&[],&Control::default())
            .err().unwrap().contains("DXF-sjabloon"));
        let dir=tempfile::tempdir().unwrap();
        opts.template=dir.path().join("template.dxf");
        std::fs::write(&opts.template,b"dummy").unwrap();
        assert!(build(&opts,&jobs,&[raster(Some(17))],&[],&[],None,&[],&Control::default())
            .err().unwrap().contains("GeoTIFF niet gevonden"));
        opts.choices[0].raster=None;
        assert!(build(&opts,&jobs,&[raster(Some(17))],&[],&[],None,&[],&Control::default())
            .err().unwrap().contains("koppel"));
        opts.choices[0].include=false;
        assert!(build(&opts,&jobs,&[raster(Some(17))],&[],&[],None,&[],&Control::default())
            .err().unwrap().contains("minstens"));
    }
}
