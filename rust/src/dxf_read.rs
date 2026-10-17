//! Pure-Rust ASCII/binary DXF overlay reader.
//! Supports common entity families from existing SleufBase files without Python/ezdxf.
//! Does not yet cover SPLINE, ELLIPSE or block transforms.

use std::{fs::File,io::Read};
use crate::dxf_tags::{self,Group};
use std::path::{Path, PathBuf};

use crate::model::{Bounds, Vertex};

const MAX_DXF_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ENTITIES: usize = 2_000_000;
const MAX_POINTS: usize = 10_000_000;

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
    /// None only for older saved overlays that did not retain this header flag.
    #[serde(default)]
    pub closed: Option<bool>,
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


fn append_feature(result:&mut Vec<DxfFeature>,feature:DxfFeature,point_count:&mut usize)->Result<(),String> {
    *point_count=point_count.checked_add(feature.points.len()).ok_or("DXF heeft te veel punten")?;
    if *point_count>MAX_POINTS || result.len()>=MAX_ENTITIES {return Err("DXF heeft te veel punten/entiteiten".into());}
    result.push(feature);Ok(())
}

fn field<'a>(groups: &'a [Group], code: i32) -> Option<&'a str> {
    groups.iter().find(|g| g.code == code).map(|g| g.value.as_ref())
}
fn number(groups: &[Group], code: i32) -> Option<f64> {
    field(groups, code)?.trim().parse::<f64>().ok().filter(|x| x.is_finite())
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
        color_aci:field(groups,62).and_then(|v|v.trim().parse::<i32>().ok()).unwrap_or(256),
        points:Vec::new(),
        raw_text:String::new(),
        block_name:String::new(),
        closed:None,
    }
}


/// Read MULTILEADER scopes instead of confusing the text/landing base point
/// with a measured arrow. The same group codes have different meanings inside
/// CONTEXT_DATA, LEADER and LEADER_LINE; 304 is both MTEXT and a scope marker.
fn mleader(groups: &[Group]) -> Result<(String, Option<Vertex>), String> {
    if !groups.iter().any(|g| g.code == 300 && g.value == "CONTEXT_DATA{") {
        // Keep compatibility with simple flat vendor exports and older projects.
        let content=groups.iter().filter(|g|g.code==304).map(|g|g.value.as_ref()).collect::<Vec<_>>().join("");
        return Ok((content,vertex(groups,10,20,30)));
    }
    let invalid=||"Ongeldige geneste MULTILEADER-context".to_owned();
    let mut scope=Vec::new();
    let mut content=String::new();
    let mut tip=None;
    let mut line_start=0;
    for (index,g) in groups.iter().enumerate() {
        match (g.code,g.value.as_ref()) {
            (300,"CONTEXT_DATA{") => {
                if !scope.is_empty(){return Err(invalid());}
                scope.push(300);
            }
            (302,"LEADER{") => {
                if scope.as_slice()!=[300]{return Err(invalid());}
                scope.push(302);
            }
            (304,"LEADER_LINE{") => {
                if scope.as_slice()!=[300,302]{return Err(invalid());}
                scope.push(304);line_start=index+1;
            }
            (305,"}") => {
                if scope.pop()!=Some(304){return Err(invalid());}
                if tip.is_none() {
                    let line=&groups[line_start..index];
                    if let Some(first)=line.iter().position(|g|g.code==10) {
                        let end=line[first+1..].iter().position(|g|g.code==10)
                            .map_or(line.len(),|next|first+1+next);
                        tip=Some(vertex(&line[first..end],10,20,30).ok_or_else(invalid)?);
                    }
                }
            }
            (303,"}") => {if scope.pop()!=Some(302){return Err(invalid());}}
            (301,"}") => {if scope.pop()!=Some(300){return Err(invalid());}}
            (304,text) if scope.as_slice()==[300] => content.push_str(text),
            _=>{}
        }
    }
    if !scope.is_empty(){return Err(invalid());}
    Ok((content,tip))
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
                x=g.value.trim().parse::<f64>().ok().filter(|v|v.is_finite());
                y=None;
            }
            20 => y=g.value.trim().parse::<f64>().ok().filter(|v|v.is_finite()),
            _=>{}
        }
    }
    if let (Some(px),Some(py))=(x,y) {
        points.push(Vertex{x:px,y:py,z:elevation});
    }
    let closed=field(groups,70).and_then(|v|v.trim().parse::<i32>().ok())
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
    if text.len() as u64>MAX_DXF_BYTES {return Err("DXF bestand groter dan 256 MiB".into());}
    parse_groups(dxf_tags::ascii(text),path)
}

fn parse_groups<'a>(records:impl Iterator<Item=Result<Group<'a>,String>>,path:&Path) -> Result<DxfOverlay,String> {
    let mut features=Vec::new();
    let mut in_entities=false;
    let mut current_kind=String::new();
    let mut current_groups: Vec<Group>=Vec::new();
    let mut polyline:Option<DxfFeature>=None;
    let mut polyline_closed=false;
    let mut point_count=0;

    let flush = |kind: &str, current: &[Group], result: &mut Vec<DxfFeature>, polyline: &mut Option<DxfFeature>, polyline_closed: &mut bool,point_count:&mut usize| -> Result<(),String> {
        if polyline.is_some() && kind!="VERTEX" && kind!="SEQEND" {
            return Err("DXF-polyline is onderbroken zonder SEQEND".into());
        }
        if kind=="POLYLINE" {
            let mut new_polyline=common(current,kind);
            *polyline_closed=field(current,70).and_then(|v|v.trim().parse::<i32>().ok())
                .is_some_and(|flags|flags & 1 != 0);
            new_polyline.closed=Some(*polyline_closed);
            *polyline=Some(new_polyline);
            return Ok(());
        }
        if kind=="VERTEX" {
            if let Some(parent)=polyline {
                if let Some(p)=vertex(current,10,20,30) {
                    if parent.points.len()>=MAX_POINTS {return Err("DXF-polyline heeft te veel punten".into());}
                    parent.points.push(p);
                }
            }
            return Ok(());
        }
        if kind=="SEQEND" {
            if let Some(mut old)=polyline.take() {
                if old.points.len()>=2 {
                    if *polyline_closed { old.points.push(old.points[0]); }
                    append_feature(result,old,point_count)?;
                }
            }
            *polyline_closed=false;
            return Ok(());
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
                let (content,tip)=mleader(current)?;
                feature.raw_text=content;
                if let Some(a)=tip {feature.points.extend_from_slice(&[a,a]);}
            }
            _ => {}
        }
        if feature.points.len()>=2 {
            append_feature(result,feature,point_count)?;
        }
        Ok(())
    };

    let mut section_name: Option<String> = None;
    let mut expect_section_name=false;
    let mut section_open=false;
    let mut saw_eof=false;
    for (index,entry) in records.enumerate() {
        let entry=entry?;
        if saw_eof && entry.code!=999 {return Err("DXF bevat gegevens na EOF".into());}
        if index>=MAX_ENTITIES*40 {return Err("DXF bevat te veel velden".into());}
        if entry.code==0 {
            if in_entities && !current_kind.is_empty() {
                flush(&current_kind,&current_groups,&mut features,&mut polyline,&mut polyline_closed,&mut point_count)?;
                current_groups.clear();
            }
            if features.len()>MAX_ENTITIES {return Err("DXF heeft te veel entiteiten".into());}
            match entry.value.as_ref() {
                "SECTION" => {
                    if section_open {return Err("DXF heeft geneste SECTION zonder ENDSEC".into());}
                    section_open=true;
                    in_entities=false;
                    expect_section_name=true;
                    current_kind.clear();
                }
                "ENDSEC" | "EOF" => {
                    if entry.value=="EOF" {
                        if section_open {return Err("DXF mist ENDSEC voor EOF".into());}
                        saw_eof=true;
                    }else{
                        if !section_open||expect_section_name {return Err("Ongeldige DXF ENDSEC".into());}
                        section_open=false;
                    }
                    in_entities=false;
                    current_kind.clear();
                    if polyline.is_some(){return Err("DXF-polyline mist SEQEND".into());}
                }
                other => current_kind=if in_entities {other.to_owned()} else {String::new()},
            }
            continue;
        }
        if expect_section_name && entry.code==2 {
            section_name=Some(entry.value.to_string());
            in_entities=entry.value.eq_ignore_ascii_case("ENTITIES");
            expect_section_name=false;
            continue;
        }
        if in_entities && !current_kind.is_empty() {
            if current_groups.len()>=1_000_000 {return Err("DXF-entiteit heeft meer dan 1 miljoen velden".into());}
            current_groups.push(entry);
        }
    }
    if section_name.is_none() {return Err("DXF bevat geen SECTION/ENTITIES".into());}
    if !saw_eof||section_open||expect_section_name {return Err("DXF afgebroken: ENDSEC/EOF ontbreekt".into());}
    Ok(DxfOverlay{path:path.to_owned(),visible:true,features})
}

/// ASCII and binary DXF share the same entity interpretation and limits.
pub fn parse_dxf_bytes(bytes:&[u8],path:&Path)->Result<DxfOverlay,String> {
    if bytes.len() as u64>MAX_DXF_BYTES {return Err("DXF bestand groter dan 256 MiB".into());}
    if bytes.starts_with(b"AutoCAD Binary DXF") {
        parse_groups(dxf_tags::Binary::new(bytes)?,path)
    } else {
        let text=dxf_tags::text(bytes)?;
        parse_dxf(&text,path)
    }
}

pub fn load_dxf(path: &Path) -> Result<DxfOverlay, String> {
    let file=File::open(path).map_err(|e|format!("DXF lezen mislukt: {e}"))?;
    if file.metadata().map_err(|e|e.to_string())?.len()>MAX_DXF_BYTES {
        return Err("DXF bestand groter dan 256 MiB".into());
    }
    let mut bytes=Vec::new();
    file.take(MAX_DXF_BYTES+1).read_to_end(&mut bytes).map_err(|e|format!("DXF lezen mislukt: {e}"))?;
    parse_dxf_bytes(&bytes,path)
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
