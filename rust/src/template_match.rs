//! Conservative, geometry-aware matching of local KickTheMap jobs and GeoTIFFs.
//! User edits always override these suggestions; conflicting job sidecars never
//! silently match a different job merely because the image is geographically close.
use crate::{
    geotiff::GeoTiffInfo,
    model::{Bounds, Dataset, Geometry, Vertex},
    virtual_trench::VirtualTrench,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchKind { JobId, Location, Missing, Ambiguous }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub raster: Option<usize>,
    pub reason: MatchKind,
}

fn valid(bounds: Bounds) -> bool {
    [bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y]
        .into_iter().all(f64::is_finite)
        && bounds.min_x < bounds.max_x && bounds.min_y < bounds.max_y
}
fn compatible(r: &GeoTiffInfo, job_id: u64) -> bool {
    r.epsg.is_none_or(|epsg|epsg == 28992) &&
        (r.kickthemap_job_id.is_none() || (job_id != 0 && r.kickthemap_job_id == Some(job_id))) &&
        valid(r.bounds)
}
fn in_bounds(p: [f64;2], bounds: Bounds) -> bool {
    (bounds.min_x..=bounds.max_x).contains(&p[0])
        && (bounds.min_y..=bounds.max_y).contains(&p[1])
}
fn sample_geometry(dataset: &Dataset) -> Vec<[f64;2]> {
    let mut points = Vec::new();
    for feature in dataset.features.iter().take(2048) {
        match &feature.geometry {
            Geometry::Point(v) => points.push([v.x, v.y]),
            Geometry::Polyline(vertices) => {
                if !vertices.is_empty() {
                    let sample = [0, vertices.len()/2, vertices.len()-1];
                    for i in sample {
                        let v = vertices[i];
                        points.push([v.x, v.y]);
                    }
                }
            }
        }
        if points.len() >= 4096 { break; }
    }
    points.retain(|p|p.iter().all(|value|value.is_finite()));
    points
}
fn sample_trench(trench: &VirtualTrench) -> Vec<[f64;2]> {
    let mut points = vec![[trench.start.x,trench.start.y],[trench.end.x,trench.end.y]];
    let center = [
        (trench.start.x+trench.end.x)/2.,
        (trench.start.y+trench.end.y)/2.
    ];
    points.push(center);
    for o in trench.objects.iter().take(2045) {
        points.push([o.position.x,o.position.y]);
    }
    points.retain(|p|p.iter().all(|v|v.is_finite()));
    points
}
fn coverage(points:&[[f64;2]], bounds:Bounds)->f64 {
    if points.is_empty() || !valid(bounds) { return 0.; }
    let covered=points.iter().filter(|&&p|in_bounds(p,bounds)).count();
    covered as f64 / points.len() as f64
}
fn best_spatial(candidates:&[usize], points:&[[f64;2]],
    rasters:&[GeoTiffInfo])->Suggestion
{
    if points.len()<2 {
        return Suggestion{raster:None,reason:MatchKind::Missing};
    }
    let mut scored=candidates.iter().map(|&i|(i,coverage(points,rasters[i].bounds)))
        .filter(|(_,score)|*score>=0.70).collect::<Vec<_>>();
    scored.sort_by(|a,b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    match scored.as_slice() {
        []=>Suggestion {raster:None,reason:MatchKind::Missing},
        [(index,_)]=>Suggestion{raster:Some(*index),reason:MatchKind::Location},
        [(index,best),(_,runner),..] if *best-*runner>=0.25 && *best>=0.90 =>
            Suggestion{raster:Some(*index),reason:MatchKind::Location},
        _=>Suggestion{raster:None,reason:MatchKind::Ambiguous}
    }
}
fn choose(job_id:u64, points:&[[f64;2]], rasters:&[GeoTiffInfo])->Suggestion {
    let matches=rasters.iter().enumerate().filter(|(_,r)|
        job_id!=0 && r.kickthemap_job_id==Some(job_id)
            && r.epsg.is_none_or(|epsg|epsg==28992) && valid(r.bounds))
        .map(|(i,_)|i).collect::<Vec<_>>();
    if matches.len()==1 {
        return Suggestion {raster:Some(matches[0]),reason:MatchKind::JobId};
    }
    if matches.len()>1 {
        let spatial=best_spatial(&matches,points,rasters);
        return if spatial.raster.is_some(){spatial}
            else{Suggestion{raster:None,reason:MatchKind::Ambiguous}};
    }
    // No ID match: only candidates with no conflicting job sidecar.
    let eligible=rasters.iter().enumerate().filter(|(_,r)|compatible(r,job_id))
        .map(|(i,_)|i).collect::<Vec<_>>();
    best_spatial(&eligible,points,rasters)
}
pub fn for_job(dataset:&Dataset, rasters:&[GeoTiffInfo])->Suggestion {
    choose(dataset.job_id,&sample_geometry(dataset),rasters)
}
pub fn for_trench(trench:&VirtualTrench, rasters:&[GeoTiffInfo])->Suggestion {
    choose(trench.job_id,&sample_trench(trench),rasters)
}
pub fn source_coverage(dataset:&Dataset, raster:&GeoTiffInfo)->f64 {
    coverage(&sample_geometry(dataset),raster.bounds)
}
pub fn trench_coverage(trench:&VirtualTrench,raster:&GeoTiffInfo)->f64 {
    coverage(&sample_trench(trench),raster.bounds)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{geo::GeoTransform,model::{Feature, Vertex}};
    use std::path::PathBuf;
    fn raster(id:Option<u64>,min_x:f64)->GeoTiffInfo {
        GeoTiffInfo {
            path:PathBuf::new(),width:400,height:100,epsg:Some(28992),
            bounds:Bounds{min_x,min_y:462990.,max_x:min_x+50.,max_y:463010.},
            transform:GeoTransform{a:1.,b:0.,c:min_x,d:0.,e:-1.,f:463010.},
            kickthemap_job_id:id,kickthemap_job_title:String::new(),kickthemap_job_sidecar:None,metadata:Default::default(),
        }
    }
    fn job(id:u64,x:f64)->Dataset {
        Dataset {job_id:id,title:"Test".into(),source_path:PathBuf::new(),
            features:(0..30).map(|i|Feature {
                source_name:"KABEL".into(),object_name:"Wire".into(),
                geometry:Geometry::Point(Vertex{x:x+i as f64,y:463000.,z:None}),
                attributes:Default::default()}).collect()}
    }
    #[test]
    fn matching_is_conservative_and_never_steals_a_foreign_job_id() {
        let rasters=[raster(Some(17),155000.),raster(None,155100.)];
        assert_eq!(for_job(&job(17,155000.),&rasters).raster,Some(0));
        assert_eq!(for_job(&job(18,155000.),&rasters).raster,None);
        assert_eq!(for_job(&job(18,155100.),&rasters).raster,Some(1));
        assert_eq!(for_job(&job(18,155100.),&rasters).reason,MatchKind::Location);
    }
    #[test]
    fn duplicate_sidecars_are_resolved_only_with_clear_spatial_evidence() {
        let rs=[raster(Some(77),155000.),raster(Some(77),155100.)];
        let pick=for_job(&job(77,155000.),&rs);
        assert_eq!(pick.raster,Some(0));
        assert_eq!(pick.reason,MatchKind::Location);
        let overlapping=[raster(None,155000.),raster(None,155000.)];
        assert_eq!(for_job(&job(0,155000.),&overlapping).reason,MatchKind::Ambiguous);
        assert_eq!(for_job(&job(0,155000.),&overlapping).raster,None);
    }
    #[test]
    fn empty_or_non_finite_geometry_is_not_a_safe_match() {
        let mut ds=job(0,155000.);
        ds.features.clear();
        assert_eq!(for_job(&ds,&[raster(None,155000.)]).raster,None);
        ds.features.push(Feature{object_name:String::new(),source_name:String::new(),
            geometry:Geometry::Point(Vertex{x:f64::NAN,y:463000.,z:None}),
            attributes:Default::default()});
        assert_eq!(for_job(&ds,&[raster(None,155000.)]).raster,None);
    }
}
