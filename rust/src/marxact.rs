//! Native MarXact DXF import: named 3D boundaries, MULTILEADER labels,
//! INSERT ownership, block-layer mapping, chainage and measured heights.
//!
//! Compatibility target: marxact_import.py and marxact_direction_patch.py.
//! ASCII/binary MULTILEADER contexts isolate MTEXT and the first actual leader-line
//! arrow, compared with the Python/ezdxf importer on synthetic CAD fixtures.

use std::collections::HashMap;
use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::dxf_read::{DxfOverlay, load_dxf};
use crate::model::Vertex;
use crate::virtual_trench::{TrenchPoint, VirtualTrench};

pub(crate) const LEADER_MATCH_TOLERANCE:f64=0.08;
const MIN_AREA:f64=0.02;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarXactLeader {
    pub x:f64,
    pub y:f64,
    pub name:String,
    pub height:Option<f64>,
}

#[derive(Clone, Debug)]
pub struct MarXactResult {
    pub trenches:Vec<VirtualTrench>,
    pub leader_count:usize,
    pub insert_count:usize,
    pub assigned_insert_count:usize,
    pub skipped_boundary_count:usize,
}

pub fn normalize_name(value:&str)->String {
    value.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

pub fn parse_leader_content(value:&str)->(String,Option<f64>) {
    use std::sync::LazyLock;
    static NAME:LazyLock<regex::Regex>=LazyLock::new(||regex::Regex::new(
        r"(?i)(?:^|\^J|\r?\n)\s*Name\s*=\s*([^\^\r\n]+)"
    ).unwrap());
    static HEIGHT:LazyLock<regex::Regex>=LazyLock::new(||regex::Regex::new(
        r"(?i)(?:^|\^J|\r?\n)\s*Height\s*=\s*([-+]?\d+(?:[.,]\d+)?)"
    ).unwrap());
    let text=value.replace("\\P","^J");
    let name=NAME.captures(&text).map_or_else(String::new,|c|c[1].trim().to_owned());
    let height=HEIGHT.captures(&text).and_then(|c|c[1].replace(',',".").parse::<f64>().ok()).filter(|n|n.is_finite());
    (name,height)
}

pub fn signed_area(points:&[Vertex])->f64 {
    if points.len()<3{return 0.0;}
    let mut sum=0.0;
    for i in 0..points.len(){
        let a=points[i]; let b=points[(i+1)%points.len()];
        sum+=a.x*b.y-b.x*a.y;
    }
    sum*0.5
}

fn distance_to_segment(x:f64,y:f64,a:Vertex,b:Vertex)->f64{
    let dx=b.x-a.x;let dy=b.y-a.y;
    let len2=dx*dx+dy*dy;
    if len2<=1e-16 {return (x-a.x).hypot(y-a.y);}
    let t=(((x-a.x)*dx+(y-a.y)*dy)/len2).clamp(0.,1.);
    (x-a.x-t*dx).hypot(y-a.y-t*dy)
}

pub fn point_inside_polygon(x:f64,y:f64,points:&[Vertex])->bool{
    if points.len()<3{return false;}
    for i in 0..points.len(){
        if distance_to_segment(x,y,points[i],points[(i+1)%points.len()])
            <=LEADER_MATCH_TOLERANCE {return true;}
    }
    let mut inside=false;
    let mut j=points.len()-1;
    for i in 0..points.len(){
        let (a,b)=(points[i],points[j]);
        if (a.y>y)!=(b.y>y) {
            let crossing=(b.x-a.x)*(y-a.y)/(b.y-a.y)+a.x;
            if x<crossing {inside=!inside;}
        }
        j=i;
    }
    inside
}

pub(crate) fn nearest_boundary_z(x:f64,y:f64,points:&[Vertex])->Option<f64>{
    let mut distance=f64::INFINITY;
    let mut best=None;
    for i in 0..points.len(){
        let a=points[i];let b=points[(i+1)%points.len()];
        let dx=b.x-a.x;let dy=b.y-a.y;
        let len2=dx*dx+dy*dy;
        let t=if len2<=1e-16{0.0}else{
            (((x-a.x)*dx+(y-a.y)*dy)/len2).clamp(0.,1.)
        };
        let dist=(x-a.x-t*dx).powi(2)+(y-a.y-t*dy).powi(2);
        let z=match (a.z,b.z){
            (Some(z0),Some(z1))=>Some(z0+(z1-z0)*t),
            (Some(z),None)|(None,Some(z))=>Some(z),
            _=>None
        };
        if dist<distance && z.is_some(){distance=dist;best=z;}
    }
    best
}

/// Automatic MarXact direction including the Python direction/midpoint patches.
pub fn centerline(points:&[Vertex],objects:&[Vertex])->Result<(Vertex,Vertex,f64),String>{
    crate::marxact_alignment::automatic_centerline(points,objects)
}

#[derive(Clone)]
struct Boundary {
    name:String,
    points:Vec<Vertex>,
    area:f64,
    objects:Vec<TrenchPoint>
}
pub fn parse_overlay(overlay:&DxfOverlay,mappings:&std::collections::BTreeMap<String,String>)
    ->Result<MarXactResult,String>
{
    let leaders=overlay.features.iter().filter(|f|
        f.entity_type=="MULTILEADER"||f.entity_type=="MLEADER"
    ).filter_map(|f|{
        let tip=f.points.first()?;
        let (name,height)=parse_leader_content(&f.raw_text);
        if name.is_empty(){return None;}
        Some(MarXactLeader{x:tip.x,y:tip.y,name,height})
    }).collect::<Vec<_>>();
    if leaders.is_empty(){return Err("Geen MarXact MULTILEADER-labels met Name en pijlpunten gevonden".into());}
    let leader_index=crate::leader_index::LeaderIndex::new(&leaders);
    let mut boundaries=Vec::new();
    let mut skipped=0usize;
    for f in &overlay.features {
        if f.entity_type!="POLYLINE"{continue;}
        let endpoints_match=f.points.len()>1 && f.points.first().zip(f.points.last())
            .is_some_and(|(a,b)|a.x==b.x&&a.y==b.y);
        if !f.closed.unwrap_or(endpoints_match){skipped+=1;continue;}
        let mut vertices=f.points.clone();
        if vertices.len()>1 && vertices.first().is_some_and(|v| {
            vertices.last().is_some_and(|last|last.x==v.x&&last.y==v.y)
        }){vertices.pop();}
        if vertices.len()<3 || signed_area(&vertices).abs()<MIN_AREA{skipped+=1;continue;}
        let names=vertices.iter().map(|v|
            leader_index.nearest(v.x,v.y).map(|l|l.name.clone()).unwrap_or_default()
        ).collect::<Vec<_>>();
        let Some(first)=names.first() else{continue};
        let normalized=normalize_name(first);
        if normalized.is_empty() || names.iter().any(|n|normalize_name(n)!=normalized){
            skipped+=1;continue;
        }
        boundaries.push(Boundary{
            name:first.trim().to_owned(),area:signed_area(&vertices).abs(),
            points:vertices,objects:Vec::new()
        });
    }
    if boundaries.is_empty(){
        return Err("Geen gesloten 3D-POLYLINE met dezelfde MULTILEADER Name op alle hoekpunten".into());
    }
    let mut insert_count=0;
    let mut assigned=0;
    for feature in &overlay.features {
        if feature.entity_type!="INSERT"{continue;}
        let Some(point)=feature.points.first() else{continue};
        insert_count+=1;
        let candidate=boundaries.iter().enumerate().filter(|(_,b)|
            point_inside_polygon(point.x,point.y,&b.points)
        ).min_by(|a,b|a.1.area.total_cmp(&b.1.area)).map(|(idx,_)|idx);
        let Some(index)=candidate else{continue};
        let leader=leader_index.nearest(point.x,point.y);
        let name=leader.map(|l|l.name.clone()).unwrap_or_default();
        let layer=feature.layer.trim();
        let mapping_name=if layer.is_empty()||layer=="0"{name.as_str()}else{layer};
        let normalized=normalize_name(mapping_name);
        let resolved=mappings.iter().find(|(key,_)|normalize_name(key)==normalized)
            .map(|(_,v)|v.as_str()).unwrap_or(mapping_name);
        let z=leader.and_then(|l|l.height).or(point.z);
        boundaries[index].objects.push(TrenchPoint{
            role:"object".into(),
            object_name:if name.is_empty(){mapping_name.to_owned()}else{name.clone()},
            source_name:resolved.to_owned(),
            position:Vertex{x:point.x,y:point.y,z},
            attributes:[name,feature.layer.clone(),feature.block_name.clone()]
        });
        assigned+=1;
    }
    let mut occurrences=HashMap::<String,usize>::new();
    let mut trenches=Vec::new();
    for boundary in boundaries {
        let object_positions=boundary.objects.iter().map(|p|p.position).collect::<Vec<_>>();
        let (start,end,width)=centerline(&boundary.points,&object_positions)?;
        let key=normalize_name(&boundary.name);
        let count=occurrences.entry(key).or_default();
        *count+=1;
        let label=if *count==1 {boundary.name}else{
            format!("{}_{}",boundary.name,*count)
        };
        let mut trench=VirtualTrench::new(label,start,end,width)?;
        trench.boundary_3d=boundary.points;
        trench.alignment=Some(crate::marxact_alignment::MarXactAlignment::new(start,end,width));
        trench.source_path=Some(overlay.path.clone());
        for object in boundary.objects{trench.add_object(object)?;}
        trenches.push(trench);
    }
    Ok(MarXactResult{
        trenches,leader_count:leaders.len(),insert_count,
        assigned_insert_count:assigned,skipped_boundary_count:skipped
    })
}
pub fn import_dxf(path:&Path,mappings:&std::collections::BTreeMap<String,String>)
    ->Result<MarXactResult,String>
{
    parse_overlay(&load_dxf(path)?,mappings)
}

#[cfg(test)]
mod tests{
    use super::*;
    use crate::dxf_read::DxfFeature;
    use std::path::PathBuf;
    fn p(x:f64,y:f64,z:f64)->Vertex{Vertex{x,y,z:Some(z)}}
    fn feature(kind:&str,points:Vec<Vertex>,text:&str,layer:&str)->DxfFeature{
        DxfFeature{
            entity_type:kind.into(),handle:String::new(),layer:layer.into(),
            color_aci:256,points,raw_text:text.into(),block_name:"marxact_point".into(),closed:None,cad:None
        }
    }
    fn rectangle()->Vec<Vertex>{vec![
        p(0.,0.,10.),p(4.,0.,14.),p(4.,2.,18.),p(0.,2.,12.)
    ]}
    #[test]
    fn python_parity_leader_name_height(){
        assert_eq!(parse_leader_content("Id=28^JName=glasvezel kabel (bundel)^JHeight=3.1441^J"),
                   ("glasvezel kabel (bundel)".into(),Some(3.1441)));
        assert_eq!(parse_leader_content("Name = Water\\PHeight=-3,75"),("Water".into(),Some(-3.75)));
    }
    #[test]
    fn python_parity_centerline_z_interpolation(){
        let (start,end,width)=centerline(&rectangle(),&[]).unwrap();
        assert!((start.x-0.).abs()<1e-6);
        assert!((start.y-1.).abs()<1e-6);
        assert!((start.z.unwrap()-11.).abs()<1e-6);
        assert!((end.x-4.).abs()<1e-6);
        assert!((end.y-1.).abs()<1e-6);
        assert!((end.z.unwrap()-16.).abs()<1e-6);
        assert!((width-2.).abs()<1e-6);
    }
    #[test]
    fn measured_blocks_override_polygon_direction(){
        let points=vec![p(0.,0.,10.),p(8.,0.,18.),p(8.,3.,22.),p(0.,3.,14.)];
        let objects=vec![p(4.,0.5,3.),p(4.,1.5,3.),p(4.,2.5,3.)];
        let (start,end,_)=centerline(&points,&objects).unwrap();
        assert!((start.x-4.).abs()<1e-6 && (end.x-4.).abs()<1e-6);
        assert!((start.y-0.).abs()<1e-6 && (end.y-3.).abs()<1e-6);
        assert!((start.z.unwrap()-14.).abs()<1e-6);
        assert!((end.z.unwrap()-18.).abs()<1e-6);
    }
    #[test]
    fn assigns_insert_to_smallest_boundary_and_preserves_source(){
        let boundary=rectangle();
        let mut items=Vec::new();
        for pt in &boundary{
            items.push(feature("MULTILEADER",vec![*pt,*pt],"Name=PS3","TEXT"));
        }
        items.push(feature("POLYLINE",
            [boundary.clone(),vec![boundary[0]]].concat(),"","0"));
        let insert=p(2.,1.,3.);
        items.push(feature("MULTILEADER",vec![insert,insert],
            "Name=ls kabel^JHeight=3,1","TEXT"));
        items.push(feature("INSERT",vec![insert,insert],"","Laagspanning"));
        items.push(feature("INSERT",vec![p(100.,100.,0.),p(100.,100.,0.)],"","0"));
        let overlay=DxfOverlay{path:PathBuf::from("file.dxf"),visible:true,features:items,layers:Default::default(),blocks:Default::default(),warnings:Vec::new(),expanded:Default::default(),block_visibility:Default::default(),block_warnings:Vec::new(),cached_bounds:None,bounds_ready:false};
        let mut maps=std::collections::BTreeMap::new();
        maps.insert("Laagspanning".to_owned(),"ls".to_owned());
        let result=parse_overlay(&overlay,&maps).unwrap();
        assert_eq!(result.trenches.len(),1);
        assert_eq!(result.insert_count,2);
        assert_eq!(result.assigned_insert_count,1);
        assert_eq!(result.trenches[0].boundary_3d.len(),4);
        assert_eq!(result.trenches[0].objects[0].source_name,"ls");
        assert_eq!(result.trenches[0].objects[0].position.z,Some(3.1));
        assert_eq!(result.trenches[0].objects[0].attributes[1],"Laagspanning");
        assert_eq!(result.trenches[0].source_path.as_deref(),Some(Path::new("file.dxf")));
    }
    #[test]
    fn rejects_unlabelled_or_collinear_boundaries(){
        let overlay=DxfOverlay{path:PathBuf::new(),visible:true,layers:Default::default(),blocks:Default::default(),warnings:Vec::new(),expanded:Default::default(),block_visibility:Default::default(),block_warnings:Vec::new(),cached_bounds:None,bounds_ready:false,
            features:vec![feature("POLYLINE",vec![p(0.,0.,0.),p(1.,0.,0.),p(1.,1.,0.),p(0.,0.,0.)],"","0")]};
        assert!(parse_overlay(&overlay,&Default::default()).is_err());
        assert_eq!(signed_area(&[p(0.,0.,0.),p(1.,0.,0.),p(2.,0.,0.)]),0.);
    }
}
