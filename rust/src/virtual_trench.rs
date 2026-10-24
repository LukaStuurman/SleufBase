//! Native virtual trench geometry, following Python virtual_trench.py.
//! This is core geometry and project storage, not template-rendering parity.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::model::{Dataset, Feature, Geometry, Vertex};

pub const DEFAULT_WIDTH_METERS:f64=0.6;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrenchPoint {
    #[serde(default,skip_serializing_if="serde_json::Value::is_null")]
    pub metadata:serde_json::Value,
    pub role:String,
    pub object_name:String,
    pub source_name:String,
    pub position:Vertex,
    #[serde(default)]
    pub attributes:[String;3],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dekband {
    pub start_chainage:f64,
    pub end_chainage:f64,
    #[serde(default)]
    pub start_depth:f64,
    #[serde(default)]
    pub end_depth:f64,
    pub source_name:String,
    #[serde(default)]
    pub display_rgb:Option<[u8;3]>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VirtualTrench {
    pub label:String,
    pub width_meters:f64,
    pub start:Vertex,
    pub end:Vertex,
    #[serde(default,skip_serializing_if="serde_json::Value::is_null")]
    pub start_metadata:serde_json::Value,
    #[serde(default,skip_serializing_if="serde_json::Value::is_null")]
    pub end_metadata:serde_json::Value,
    #[serde(default)]
    pub objects:Vec<TrenchPoint>,
    #[serde(default)]
    pub dekband_rows:Vec<Dekband>,
    #[serde(default)]
    pub source_path:Option<PathBuf>,
    /// Measured 3D MarXact boundary in its current map placement. Source files
    /// stay unchanged; empty for manually drawn trenches.
    #[serde(default)]
    pub boundary_3d:Vec<Vertex>,
    /// Automatic axis baseline and absolute manual correction. Absent in older previews.
    #[serde(default)]
    pub alignment:Option<crate::marxact_alignment::MarXactAlignment>,
    #[serde(default)]
    pub job_id:u64,
    /// Active layer metadata, including original raw RD reference points.
    #[serde(default,skip_serializing_if="serde_json::Value::is_null")]
    pub metadata:serde_json::Value,
}

// Numerically equal chainages (including -0/+0) keep their source order.
// Retain a deterministic fallback for non-comparable derived values.
fn chainage_order(a:f64,b:f64)->std::cmp::Ordering {a.partial_cmp(&b).unwrap_or_else(||a.total_cmp(&b))}

impl VirtualTrench {
    pub fn new(label:String,start:Vertex,end:Vertex,width:f64) -> Result<Self,String> {
        let result=Self {
            label,width_meters:width,start,end,start_metadata:Default::default(),end_metadata:Default::default(),
            objects:Vec::new(),dekband_rows:Vec::new(),source_path:None,boundary_3d:Vec::new(),alignment:None,job_id:0,metadata:Default::default()
        };
        result.validate()?;
        Ok(result)
    }

    pub fn validate(&self) -> Result<(),String> {
        self.point_metadata_bytes()?;
        if let Some(alignment)=&self.alignment {alignment.validate()?;}
        if !self.width_meters.is_finite() || self.width_meters<0.1 {
            return Err("Een virtuele proefsleuf moet minstens 0,1 m breed zijn".into());
        }
        for v in std::iter::once(&self.start)
            .chain(std::iter::once(&self.end))
            .chain(self.objects.iter().map(|p|&p.position))
        {
            if !v.x.is_finite() || !v.y.is_finite() ||
                v.z.is_some_and(|z|!z.is_finite())
            {
                return Err("Virtuele proefsleuf bevat ongeldige coördinaten".into());
            }
        }
        if self.boundary_3d.iter().any(|p|!p.x.is_finite() || !p.y.is_finite() || p.z.is_some_and(|z|!z.is_finite())) {
            return Err("MarXact-grens heeft ongeldige 3D-coördinaten".into());
        }
        if self.dekband_rows.len()>10_000 {return Err("Te veel dekbandsegmenten".into());}
        for row in &self.dekband_rows {
            if !row.start_chainage.is_finite() || !row.end_chainage.is_finite()
                || !row.start_depth.is_finite() || !row.end_depth.is_finite() || row.start_depth<0. || row.end_depth<0.
                || row.end_chainage <= row.start_chainage+1e-6
                || row.source_name.len()>256 || row.source_name.chars().any(char::is_control)
            {return Err("Dekband heeft ongeldige kettingmaten of omschrijving".into());}
        }
        if self.length()<=1e-9 {
            return Err("Begin- en eindpunt van proefsleuf vallen samen".into());
        }
        Ok(())
    }

    pub fn point_metadata_bytes(&self)->Result<usize,String>{
        let mut bytes=0_usize;
        for metadata in std::iter::once(&self.metadata).chain([&self.start_metadata,&self.end_metadata]).chain(self.objects.iter().map(|o|&o.metadata)){
            bytes=bytes.saturating_add(crate::raster_points::validate_metadata(metadata)?);
            if bytes>2*1024*1024{return Err("Virtuele laag- en puntmetadata is groter dan 2 MB".into());}
        }
        Ok(bytes)
    }

    pub fn length(&self)->f64 {
        (self.end.x-self.start.x).hypot(self.end.y-self.start.y)
    }

    /// Returns unbounded projected chainage, identical to Python point_chainage.
    pub fn chainage(&self,p:Vertex)->f64 {
        let dx=self.end.x-self.start.x;
        let dy=self.end.y-self.start.y;
        let squared=dx*dx+dy*dy;
        if squared<=1e-9 { return 0.0; }
        ((p.x-self.start.x)*dx+(p.y-self.start.y)*dy)/squared.sqrt()
    }

    /// Returns (x,y,along_trench_meters, overshoot_distance_meters).
    pub fn project(&self,p:Vertex)->(f64,f64,f64,f64) {
        let dx=self.end.x-self.start.x;
        let dy=self.end.y-self.start.y;
        let squared=dx*dx+dy*dy;
        if squared<=1e-9 {
            return (self.start.x,self.start.y,0.0,0.0);
        }
        let raw=((p.x-self.start.x)*dx+(p.y-self.start.y)*dy)/squared;
        let t=raw.clamp(0.0,1.0);
        let length=squared.sqrt();
        (self.start.x+dx*t,self.start.y+dy*t,t*length,(raw-t).abs()*length)
    }

    /// Clockwise four-corner polygon like virtual_trench_polygon in Python.
    pub fn polygon(&self)->[(f64,f64);4] {
        let dx=self.end.x-self.start.x;
        let dy=self.end.y-self.start.y;
        let length=self.length().max(1e-9);
        let nx=-dy/length*self.width_meters*0.5;
        let ny=dx/length*self.width_meters*0.5;
        [
            (self.start.x+nx,self.start.y+ny),
            (self.start.x-nx,self.start.y-ny),
            (self.end.x-nx,self.end.y-ny),
            (self.end.x+nx,self.end.y+ny)
        ]
    }

    pub fn measured_boundary(&self)->Vec<(f64,f64)> {
        let mut points=if self.boundary_3d.len()>=3 {
            self.boundary_3d.iter().map(|p|(p.x,p.y)).collect::<Vec<_>>()
        }else{self.polygon().to_vec()};
        if points.len()>=3 && points.first()!=points.last() {points.push(points[0]);}
        points
    }

    pub fn add_object(&mut self,object:TrenchPoint) -> Result<(),String> {
        if !object.position.x.is_finite() || !object.position.y.is_finite() {
            return Err("Objectpositie is ongeldig".into());
        }
        self.objects.push(object);
        let start=self.start;
        let end=self.end;
        let dx=end.x-start.x;
        let dy=end.y-start.y;
        self.objects.sort_by(|a,b| {
            let chainage=|v:Vertex|(v.x-start.x)*dx+(v.y-start.y)*dy;
            chainage_order(chainage(a.position),chainage(b.position))
        });
        Ok(())
    }

    pub fn to_dataset(&self,job_id:u64,include_endpoints:bool)->Dataset {
        self.to_dataset_with_cancel(job_id,include_endpoints,&crate::input::NEVER_CANCEL).expect("uncancelled virtual job")
    }
    pub(crate) fn to_dataset_with_cancel(&self,job_id:u64,include_endpoints:bool,cancel:&std::sync::atomic::AtomicBool)->Result<Dataset,String> {
        self.to_base_dataset_with_cancel(job_id,include_endpoints,cancel)
    }
    /// Geometry-only dataset with bands on the measured axis. Template exports
    /// use `to_base_dataset_with_cancel` and `append_dekband_for_profile` instead
    /// because their marker-expanded axis also controls surface interpolation.
    pub fn to_dataset_with_direction(&self,job_id:u64,include_endpoints:bool,reverse:bool,cancel:&std::sync::atomic::AtomicBool)->Result<Dataset,String> {
        let mut dataset=self.to_base_dataset_with_cancel(job_id,include_endpoints,cancel)?;
        let (a,b)=if reverse{(self.end,self.start)}else{(self.start,self.end)};
        Self::append_dekband_on_axis(&self.dekband_rows,&mut dataset,a,b,self.length(),cancel)?;
        Ok(dataset)
    }
    /// Base points only. Template exporters must build the chosen base profile
    /// before injecting deck bands: endpoint markers can extend its axis.
    pub fn to_base_dataset_with_cancel(&self,job_id:u64,include_endpoints:bool,cancel:&std::sync::atomic::AtomicBool)->Result<Dataset,String> {
        crate::input::check(cancel)?;
        let mut features=Vec::with_capacity(self.objects.len()+if include_endpoints{2}else{0});
        for (index,object) in self.objects.iter().enumerate() {
            if index%1024==0 {crate::input::check(cancel)?;}
            features.push(Feature {
                object_name:object.object_name.clone(),
                source_name:object.source_name.clone(),
                geometry:Geometry::Point(object.position),
                attributes:object.attributes.clone()
            });
        }
        // Same stable chainage order as ordered_virtual_trench_points in Python.
        // Keep endpoints outside the object sort: they define marker direction.
        features.sort_by(|a,b| {
            let chainage=|f:&Feature|match f.geometry{Geometry::Point(v)=>self.chainage(v),_=>0.};
            chainage_order(chainage(a),chainage(b))
        });
        if include_endpoints {
            let endpoint=|name:&str,position|Feature{object_name:name.into(),source_name:"PUNT".into(),geometry:Geometry::Point(position),attributes:Default::default()};
            features.insert(0,endpoint("Beginpunt",self.start));
            features.push(endpoint("Eindpunt",self.end));
        }
        crate::input::check(cancel)?;
        Ok(Dataset {
            job_id:if job_id==0 {self.job_id} else {job_id},title:self.label.clone(),
            source_path:self.source_path.clone().unwrap_or_default(),
            features
        })
    }
    /// Match the original exporter, including its marker-expanded profile axis
    /// and chosen direction. Call once on a base dataset before the final build.
    pub fn append_dekband_for_profile(&self,dataset:&mut Dataset,profile:&crate::profile::Profile<'_>,cancel:&std::sync::atomic::AtomicBool)->Result<(),String>{
        append_dekband_rows_for_profile(&self.dekband_rows,dataset,profile,cancel)
    }
    fn append_dekband_on_axis(rows:&[Dekband],dataset:&mut Dataset,a:Vertex,b:Vertex,length:f64,cancel:&std::sync::atomic::AtomicBool)->Result<(),String>{
        crate::input::check(cancel)?;
        if length<=1e-9{return Ok(());}
        let mut bands=Vec::with_capacity(rows.len());
        for (index,row) in rows.iter().enumerate(){
            if index%256==0{crate::input::check(cancel)?;}
            let start=row.start_chainage.clamp(0.,length);let end=row.end_chainage.clamp(0.,length);
            if (end-start).abs()<=1e-6{continue;}
            let vertex=|along:f64,depth:f64|{let ratio=along/length;let surface=a.z.unwrap_or(0.)+(b.z.unwrap_or(0.)-a.z.unwrap_or(0.))*ratio;Vertex{x:a.x+(b.x-a.x)*ratio,y:a.y+(b.y-a.y)*ratio,z:Some(surface-depth)}};
            let source=crate::python_values::trim(&row.source_name);let source=if source.is_empty(){"Dekband"}else{source};
            bands.push(Feature{object_name:if crate::python_values::casefold(source)=="dekband"{format!("Dekband {}",index+1)}else{source.into()},source_name:source.into(),geometry:Geometry::Polyline(vec![vertex(start,row.start_depth),vertex(end,row.end_depth)]),attributes:[String::new(),String::new(),"dekband".into()]});
        }
        crate::input::check(cancel)?;
        dataset.features.extend(bands);
        Ok(())
    }
}

/// The same injection applies to a physical TIFF's declared template bands.
pub fn append_dekband_rows_for_profile(rows:&[Dekband],dataset:&mut Dataset,profile:&crate::profile::Profile<'_>,cancel:&std::sync::atomic::AtomicBool)->Result<(),String>{
    VirtualTrench::append_dekband_on_axis(rows,dataset,profile.start,profile.end,profile.axis_length,cancel)
}

/// Original `_template_dekband_rows`: preserve declaration order, swap depths
/// with reversed chainages, and default a missing end depth to the start depth.
pub fn dekband_rows_from_metadata(metadata:&serde_json::Value)->Vec<Dekband>{
    let Some(rows)=metadata.get("template_dekband_lines").and_then(serde_json::Value::as_array)else{return Vec::new()};
    rows.iter().take(10_000).filter_map(|row|{
        let mut start=crate::python_values::number(row.get("start_chainage"))?;let mut end=crate::python_values::number(row.get("end_chainage"))?;
        let mut start_depth=if let Some(value)=row.get("start_depth"){crate::python_values::number(Some(value))?}else{0.}.max(0.);
        let mut end_depth=if let Some(value)=row.get("end_depth"){crate::python_values::number(Some(value))?}else{start_depth}.max(0.);
        if ![start,end,start_depth,end_depth].iter().all(|v|v.is_finite())||(end-start).abs()<=1e-6{return None;}
        if end<start{std::mem::swap(&mut start,&mut end);std::mem::swap(&mut start_depth,&mut end_depth);}
        let name=row.get("source_name").filter(|v|crate::python_values::truth(v)).or_else(||row.get("label").filter(|v|crate::python_values::truth(v)));
        let name=crate::python_values::string(name).ok()?;let name=crate::python_values::trim(&name);let name=if name.is_empty(){"Dekband"}else{name};
        if name.len()>256||name.chars().any(char::is_control){return None;}
        Some(Dekband{start_chainage:start,end_chainage:end,start_depth,end_depth,source_name:name.into(),display_rgb:crate::virtual_raster::display_rgb(row)})
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn trench()->VirtualTrench{
        VirtualTrench::new("PS1".into(),
            Vertex{x:0.0,y:0.0,z:None},
            Vertex{x:10.0,y:0.0,z:None},0.6).unwrap()
    }
    #[test]
    fn rectangle_width_and_length() {
        let t=trench();
        assert_eq!(t.length(),10.0);
        assert_eq!(t.polygon(),[(0.0,0.3),(0.0,-0.3),(10.0,-0.3),(10.0,0.3)]);
    }
    #[test]
    fn clip_projection_beyond_end() {
        let p=Vertex{x:20.0,y:2.0,z:None};
        let result=trench().project(p);
        assert_eq!(result,(10.0,0.0,10.0,10.0));
    }
    #[test]
    fn sortable_objects_and_dataset() {
        let mut t=trench();
        for x in [8.0,2.0] {
            t.add_object(TrenchPoint {metadata:Default::default(),
                role:String::new(),object_name:"LS".into(),source_name:"ls".into(),
                position:Vertex{x,y:0.0,z:None},attributes:Default::default()
            }).unwrap();
        }
        assert_eq!(t.objects[0].position.x,2.0);
        let result=t.to_dataset(0,true);
        assert_eq!(result.features.len(),4);
        assert_eq!(result.features[0].source_name,"PUNT");
    }
}
