//! Pure-Rust ASCII DXF overlay reader.
//! Supports common entity families from existing SleufBase files without Python/ezdxf.
//! Does not yet cover binary DXF, SPLINE, ELLIPSE or block transforms.

use std::fs;
use std::path::{Path, PathBuf};

use crate::model::{Bounds, Vertex};

const MAX_DXF_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ENTITIES: usize = 2_000_000;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DxfFeature {
    pub entity_type: String,
    pub handle: String,
    pub layer: String,
    pub color_aci: i32,
    pub points: Vec<Vertex>,
    #[serde(default)]
    pub raw_text: String,
    #[serde(default)]
    pub block_name: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DxfOverlay {
    pub path: PathBuf,
    pub visible: bool,
    pub features: Vec<DxfFeature>,
}

impl DxfOverlay {
    pub fn bounds(&self) -> Option<Bounds> {
        let mut bounds: Option<Bounds> = None;
        for feature in &self.features {
            for vertex in &feature.points {
                if let Some(b) = bounds.as_mut() {
                    b.min_x = b.min_x.min(vertex.x);
                    b.min_y = b.min_y.min(vertex.y);
                    b.max_x = b.max_x.max(vertex.x);
                    b.max_y = b.max_y.max(vertex.y);
                } else {
                    bounds = Some(Bounds {min_x:vertex.x,min_y:vertex.y,max_x:vertex.x,max_y:vertex.y});
                }
            }
        }
        bounds
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Group {
    code: i32,
    value: String,
}

fn groups(text: &str) -> Result<Vec<Group>, String> {
    let mut lines = text.lines().map(|line| line.trim_end_matches('\r'));
    let mut result = Vec::new();
    loop {
        let Some(code_text) = lines.next() else { break };
        let Some(value) = lines.next() else { return Err("DXF mist een waarde bij de laatste groepcode".into()) };
        let code = code_text.trim().parse::<i32>()
            .map_err(|_| format!("Ongeldige DXF-groepcode: {code_text}"))?;
        result.push(Group {code,value:value.trim().to_owned()});
        if result.len() > MAX_ENTITIES * 40 {
            return Err("DXF bevat te veel velden".into());
        }
    }
    Ok(result)
}

fn field<'a>(groups: &'a [Group], code: i32) -> Option<&'a str> {
    groups.iter().find(|g| g.code == code).map(|g| g.value.as_str())
}
fn number(groups: &[Group], code: i32) -> Option<f64> {
    field(groups, code)?.parse::<f64>().ok().filter(|x| x.is_finite())
}
fn vertex(groups: &[Group], xcode: i32, ycode: i32, zcode: i32) -> Option<Vertex> {
    Some(Vertex {
        x:number(groups,xcode)?, y:number(groups,ycode)?,
        z:number(groups,zcode),
    })
}
fn common(groups: &[Group], kind: &str) -> DxfFeature {
    DxfFeature {
        entity_type:kind.to_owned(),
        handle:field(groups,5).unwrap_or("").into(),
        layer:field(groups,8).unwrap_or("0").into(),
        color_aci:field(groups,62).and_then(|v|v.parse::<i32>().ok()).unwrap_or(256),
        points:Vec::new(),
        raw_text:String::new(),
        block_name:String::new(),
    }
}

fn lwpolyline(groups: &[Group], points: &mut Vec<Vertex>) {
    let mut x: Option<f64> = None;
    let mut y: Option<f64> = None;
    let elevation=number(groups, 38);
    for g in groups {
        match g.code {
            10 => {
                if let (Some(px),Some(py))=(x,y) {
                    points.push(Vertex{x:px,y:py,z:elevation});
                }
                x=g.value.parse::<f64>().ok().filter(|v|v.is_finite());
                y=None;
            }
            20 => y=g.value.parse::<f64>().ok().filter(|v|v.is_finite()),
            _=>{}
        }
    }
    if let (Some(px),Some(py))=(x,y) {
        points.push(Vertex{x:px,y:py,z:elevation});
    }
    let closed=field(groups,70).and_then(|v|v.parse::<u32>().ok())
        .is_some_and(|v|v & 1 != 0);
    if closed && points.len()>1 {
        points.push(points[0]);
    }
}

/// Approximate curves to polyline coordinates in the DXF's world XY plane.
/// The approximation tolerance controls maximum chord length.
fn circle_or_arc(groups: &[Group], kind: &str, output: &mut Vec<Vertex>) {
    let Some(center)=vertex(groups,10,20,30) else { return };
    let Some(radius)=number(groups,40) else { return };
    if radius<=0.0 { return; }
    let (start,end) = if kind=="CIRCLE" {
        (0.0,std::f64::consts::TAU)
    } else {
        let Some(start)=number(groups,50) else {return};
        let Some(end)=number(groups,51) else {return};
        let mut sweep=(end-start).rem_euclid(360.0);
        if sweep.abs() < 1e-9 { sweep=360.0; }
        (start.to_radians(),(start+sweep).to_radians())
    };
    let subdivisions=(((end-start).abs()*radius/0.25).ceil() as usize).clamp(12,2048);
    for i in 0..=subdivisions {
        let angle=start+(end-start)*i as f64/subdivisions as f64;
        output.push(Vertex {
            x:center.x+angle.cos()*radius,
            y:center.y+angle.sin()*radius,
            z:center.z
        });
    }
}

/// ASCII text reader with entity group isolation. Block INSERT is represented
/// by a point marker (matching the old Python dxf.py behavior).
pub fn parse_dxf(text: &str, path: &Path) -> Result<DxfOverlay, String> {
    if text.starts_with("AutoCAD Binary DXF") {
        return Err("Binaire DXF vereist een aparte decoder".into());
    }
    let records=groups(text)?;
    let mut features=Vec::new();
    let mut in_entities=false;
    let mut current_kind=String::new();
    let mut current_groups: Vec<Group>=Vec::new();
    let mut polyline:Option<DxfFeature>=None;
    let mut polyline_closed=false;

    let flush = |kind: &str, current: &[Group], result: &mut Vec<DxfFeature>, polyline: &mut Option<DxfFeature>, polyline_closed: &mut bool| {
        if kind=="POLYLINE" {
            let new_polyline=common(current,kind);
            *polyline_closed=field(current,70).and_then(|v|v.parse::<u32>().ok())
                .is_some_and(|flags|flags & 1 != 0);
            *polyline=Some(new_polyline);
            return;
        }
        if kind=="VERTEX" {
            if let Some(parent)=polyline {
                if let Some(p)=vertex(current,10,20,30) { parent.points.push(p); }
            }
            return;
        }
        if kind=="SEQEND" {
            if let Some(mut old)=polyline.take() {
                if old.points.len()>=2 {
                    if *polyline_closed { old.points.push(old.points[0]); }
                    result.push(old);
                }
            }
            *polyline_closed=false;
            return;
        }
        let mut feature=common(current,kind);
        match kind {
            "LINE" => {
                if let (Some(a),Some(b))=(vertex(current,10,20,30),vertex(current,11,21,31)) {
                    feature.points.extend_from_slice(&[a,b]);
                }
            }
            "LWPOLYLINE"=>lwpolyline(current,&mut feature.points),
            "CIRCLE" | "ARC"=>circle_or_arc(current,kind,&mut feature.points),
            "INSERT" => {
                feature.block_name=field(current,2).unwrap_or("").to_owned();
                if let Some(a)=vertex(current,10,20,30) {
                    feature.points.extend_from_slice(&[a,a]);
                }
            }
            "MULTILEADER" | "MLEADER" => {
                // Common MarXact exports put the MTEXT content into group 304
                // and the first leader line's tip into the first 10/20 pair.
                // Not all CAD vendors serialize these groups identically:
                // strict geometry validation in the importer prevents guessing.
                feature.raw_text=current.iter().filter(|v|v.code==304)
                    .map(|v|v.value.as_str()).collect::<Vec<_>>().join("");
                if let Some(a)=vertex(current,10,20,30) {
                    feature.points.extend_from_slice(&[a,a]);
                }
            }
            _ => {}
        }
        if feature.points.len()>=2 {
            result.push(feature);
        }
    };

    let mut section_name: Option<String> = None;
    let mut expect_section_name=false;
    for entry in &records {
        if entry.code==0 {
            if in_entities && !current_kind.is_empty() {
                flush(&current_kind,&current_groups,&mut features,&mut polyline,&mut polyline_closed);
                current_groups.clear();
            }
            if features.len()>MAX_ENTITIES {return Err("DXF heeft te veel entiteiten".into());}
            match entry.value.as_str() {
                "SECTION" => {
                    in_entities=false;
                    expect_section_name=true;
                    current_kind.clear();
                }
                "ENDSEC" | "EOF" => {
                    in_entities=false;
                    current_kind.clear();
                    if let Some(parent)=polyline.take() {
                        if parent.points.len()>=2 {
                            let mut parent=parent;
                            if polyline_closed {parent.points.push(parent.points[0]);}
                            features.push(parent);
                        }
                    }
                }
                other => current_kind=if in_entities {other.to_owned()} else {String::new()},
            }
            continue;
        }
        if expect_section_name && entry.code==2 {
            section_name=Some(entry.value.clone());
            in_entities=entry.value.eq_ignore_ascii_case("ENTITIES");
            expect_section_name=false;
            continue;
        }
        if in_entities && !current_kind.is_empty() {current_groups.push(entry.clone());}
    }
    if section_name.is_none() {return Err("DXF bevat geen SECTION/ENTITIES".into());}
    Ok(DxfOverlay{path:path.to_owned(),visible:true,features})
}

pub fn load_dxf(path: &Path) -> Result<DxfOverlay, String> {
    if fs::metadata(path).map_err(|e|e.to_string())?.len()>MAX_DXF_BYTES {
        return Err("DXF bestand groter dan 256 MiB".into());
    }
    let bytes=fs::read(path).map_err(|e|format!("DXF lezen mislukt: {e}"))?;
    if bytes.starts_with(b"AutoCAD Binary DXF") {
        return Err("Binaire DXF wordt nog niet ondersteund".into());
    }
    let text=String::from_utf8_lossy(&bytes);
    parse_dxf(&text,path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_line_polyline_closed_and_circle() {
        let src="0\nSECTION\n2\nENTITIES\n\
                 0\nLINE\n8\nWATER\n5\nA1\n10\n1\n20\n2\n11\n3\n21\n4\n\
                 0\nLWPOLYLINE\n8\nLS\n70\n1\n10\n0\n20\n0\n10\n10\n20\n0\n10\n10\n20\n10\n\
                 0\nCIRCLE\n8\nGAS\n10\n5\n20\n5\n40\n2\n\
                 0\nENDSEC\n0\nEOF\n";
        let overlay=parse_dxf(src,Path::new("test.dxf")).unwrap();
        assert_eq!(overlay.features.len(),3);
        assert_eq!(overlay.features[0].points[0].x,1.0);
        assert_eq!(overlay.features[1].points.len(),4);
        assert_eq!(overlay.features[1].points.first(),overlay.features[1].points.last());
        assert!(overlay.features[2].points.len()>12);
        let b=overlay.bounds().unwrap();
        assert!(b.max_x>=10.0);
    }
    #[test]
    fn parses_python_export_line_entity() {
        let src="0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nLAYER\n62\n5\n10\n0\n20\n0\n30\n1\n11\n2\n21\n3\n31\n4\n0\nENDSEC\n0\nEOF\n";
        let overlay=parse_dxf(src,Path::new("a.dxf")).unwrap();
        assert_eq!(overlay.features.len(),1);
        assert_eq!(overlay.features[0].color_aci,5);
        assert_eq!(overlay.features[0].points[1].z,Some(4.0));
    }
    #[test]
    fn polyline_uses_header_closed_flag() {
        let dxf="0\nSECTION\n2\nENTITIES\n                 0\nPOLYLINE\n8\nWATER\n70\n1\n                 0\nVERTEX\n10\n1\n20\n2\n                 0\nVERTEX\n10\n3\n20\n4\n                 0\nSEQEND\n0\nENDSEC\n0\nEOF\n";
        let result=parse_dxf(dxf,Path::new("test.dxf")).unwrap();
        assert_eq!(result.features.len(),1);
        assert_eq!(result.features[0].points.len(),3);
        assert_eq!(result.features[0].points[0],result.features[0].points[2]);
    }
    #[test]
    fn marxact_mleader_text_and_insert_block_name() {
        let dxf="0\nSECTION\n2\nENTITIES\n                 0\nMULTILEADER\n8\nLABEL\n304\nName=ps1^JHeight=3,14\n                 10\n1.0\n20\n2.0\n30\n3.0\n                 0\nINSERT\n8\nWATER\n2\nmarxact_point\n10\n1.0\n20\n2.0\n                 0\nENDSEC\n0\nEOF\n";
        let result=parse_dxf(dxf,Path::new("marxact.dxf")).unwrap();
        assert_eq!(result.features.len(),2);
        assert!(result.features[0].raw_text.contains("Name=ps1"));
        assert_eq!(result.features[1].block_name,"marxact_point");
    }
    #[test]
    fn corrupt_and_binary_are_errors() {
        assert!(parse_dxf("random",Path::new("x")).is_err());
        assert!(parse_dxf("AutoCAD Binary DXF",Path::new("x")).is_err());
    }
}
