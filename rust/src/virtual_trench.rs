//! Native virtual trench geometry, following Python virtual_trench.py.
//! This is core geometry and project storage, not template-rendering parity.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::model::{Dataset, Feature, Geometry, Vertex};

pub const DEFAULT_WIDTH_METERS:f64=0.6;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrenchPoint {
    pub role:String,
    pub object_name:String,
    pub source_name:String,
    pub position:Vertex,
    #[serde(default)]
    pub attributes:[String;3],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VirtualTrench {
    pub label:String,
    pub width_meters:f64,
    pub start:Vertex,
    pub end:Vertex,
    #[serde(default)]
    pub objects:Vec<TrenchPoint>,
    #[serde(default)]
    pub source_path:Option<PathBuf>,
    /// Original measured 3D MarXact boundary; empty for manually drawn trenches.
    #[serde(default)]
    pub boundary_3d:Vec<Vertex>,
    /// Automatic axis baseline and absolute manual correction. Absent in older previews.
    #[serde(default)]
    pub alignment:Option<crate::marxact_alignment::MarXactAlignment>,
    #[serde(default)]
    pub job_id:u64,
}

impl VirtualTrench {
    pub fn new(label:String,start:Vertex,end:Vertex,width:f64) -> Result<Self,String> {
        let result=Self {
            label,width_meters:width,start,end,
            objects:Vec::new(),source_path:None,boundary_3d:Vec::new(),alignment:None,job_id:0
        };
        result.validate()?;
        Ok(result)
    }

    pub fn validate(&self) -> Result<(),String> {
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
        if self.length()<=1e-9 {
            return Err("Begin- en eindpunt van proefsleuf vallen samen".into());
        }
        Ok(())
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
            chainage(a.position).total_cmp(&chainage(b.position))
        });
        Ok(())
    }

    pub fn to_dataset(&self,job_id:u64,include_endpoints:bool)->Dataset {
        let mut features=Vec::new();
        if include_endpoints {
            for (label,pos) in [("Beginpunt",self.start),("Eindpunt",self.end)] {
                features.push(Feature {
                    object_name:label.into(),
                    source_name:"PUNT".into(),
                    geometry:Geometry::Point(pos),
                    attributes:Default::default()
                });
            }
        }
        for object in &self.objects {
            features.push(Feature {
                object_name:object.object_name.clone(),
                source_name:object.source_name.clone(),
                geometry:Geometry::Point(object.position),
                attributes:object.attributes.clone()
            });
        }
        Dataset {
            job_id:if job_id==0 {self.job_id} else {job_id},title:self.label.clone(),
            source_path:self.source_path.clone().unwrap_or_default(),
            features
        }
    }
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
            t.add_object(TrenchPoint {
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
