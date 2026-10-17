//! Pure-Rust ASCII/binary DXF overlay reader.
//! Supports common entity families from existing SleufBase files without Python/ezdxf.
//! Source entities and block definitions stay separate from derived WCS caches.

use std::{fs::File,io::Read};
use crate::dxf_tags::{self,Group};
use std::path::{Path, PathBuf};

use crate::model::{Bounds, Vertex};
use std::collections::BTreeMap;
use std::sync::Arc;

const MAX_DXF_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ENTITIES: usize = 2_000_000;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
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
    #[serde(default)]
    pub cad:Option<Box<crate::cad::Properties>>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DxfOverlay {
    pub path: PathBuf,
    pub visible: bool,
    pub features: Vec<DxfFeature>,
    #[serde(default)]
    pub layers:BTreeMap<String,crate::cad::Layer>,
    #[serde(default,skip_serializing_if="BTreeMap::is_empty")]
    pub blocks:BTreeMap<String,Arc<crate::cad_scene::Block>>,
    #[serde(default,skip_serializing_if="Vec::is_empty")]
    pub warnings:Vec<String>,
    #[serde(skip)]
    pub expanded:Arc<crate::cad_scene::Scene>,
    #[serde(skip)]
    pub block_visibility:Arc<Vec<bool>>,
    #[serde(skip)]
    pub block_warnings:Vec<String>,
    #[serde(skip)]
    pub cached_bounds:Option<Bounds>,
    #[serde(skip)]
    pub bounds_ready:bool,
}

impl DxfOverlay {
    pub fn feature_visible(&self,feature:&DxfFeature)->bool {
        feature.color_aci>=0&&!feature.cad.as_deref().is_some_and(|p|p.invisible)
            &&self.layers.get(&feature.layer).is_none_or(|layer|layer.visible)
    }
    pub fn feature_rgb(&self,feature:&DxfFeature)->[u8;3] {
        crate::cad::display_rgb(feature.color_aci,feature.cad.as_deref(),self.layers.get(&feature.layer))
    }
    pub fn bounds(&self)->Option<Bounds>{
        if self.bounds_ready {self.cached_bounds}else{self.calculate_bounds()}
    }
    pub(crate) fn calculate_bounds(&self)->Option<Bounds>{
        let mut bounds: Option<Bounds> = None;
        for feature in self.rendered_features() {
            if !feature.visible{continue;}
            for vertex in feature.points {
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

impl DxfFeature {
    pub fn details(&self,path:&Path)->BTreeMap<String,String> {
        let mut result=BTreeMap::from([
            ("Bestand".into(),path.file_name().unwrap_or_default().to_string_lossy().into_owned()),
            ("Type".into(),self.entity_type.clone()),("Handle".into(),self.handle.clone()),
            ("Laag".into(),self.layer.clone()),("Punten".into(),self.points.len().to_string()),
        ]);
        if let Some(cad)=self.cad.as_deref() {
            if let Some(value)=&cad.line_type {result.insert("Lijntype".into(),value.clone());}
            if let Some(value)=cad.line_weight {result.insert("Lijndikte".into(),value.to_string());}
            for (name,value) in &cad.attributes {result.insert(format!("Attribuut {name}"),value.clone());}
            for (name,tags) in &cad.xdata {if !tags.is_empty(){result.insert(format!("XDATA {name}"),crate::cad::xdata_text(tags));}}
        }
        result
    }
}

/// ASCII text reader with entity group isolation and cached block geometry.
/// Raw INSERT markers remain available to the MarXact measurement importer.
pub fn parse_dxf(text: &str, path: &Path) -> Result<DxfOverlay, String> {
    if text.starts_with("AutoCAD Binary DXF") {
        return Err("Binaire DXF vereist een aparte decoder".into());
    }
    if text.len() as u64>MAX_DXF_BYTES {return Err("DXF bestand groter dan 256 MiB".into());}
    parse_groups(dxf_tags::ascii(text),path)
}

struct BlockStart {name:String,base:crate::cad_geometry::V3,start:usize}
struct Parser {
    entities:crate::dxf_entities::Entities,
    layers:BTreeMap<String,crate::cad::Layer>,
    blocks:BTreeMap<String,Arc<crate::cad_scene::Block>>,
    block:Option<BlockStart>,
}
impl Parser {
    fn flush(&mut self,section:&str,kind:&str,groups:&[Group])->Result<(),String>{
        use crate::dxf_entities::{field,vertex};
        match section {
            "ENTITIES"=>self.entities.push(kind,groups)?,
            "TABLES" if kind=="LAYER"=>{
                let name=field(groups,2).ok_or("DXF LAYER mist naam")?.to_owned();
                let color=field(groups,62).map(|v|v.trim().parse::<i32>().map_err(|_|"Ongeldige DXF-laagkleur")).transpose()?.unwrap_or(7);
                let flags=field(groups,70).and_then(|v|v.trim().parse::<i32>().ok()).unwrap_or(0);
                let layer=crate::cad::Layer{color_aci:color.unsigned_abs().min(i32::MAX as u32)as i32,
                    true_color:field(groups,420).map(|v|v.trim().parse::<u32>().map_err(|_|"Ongeldige DXF-laag true-color")).transpose()?,visible:color>=0&&flags&1==0};
                layer.validate()?;self.layers.insert(name,layer);
            }
            "BLOCKS"=>match kind {
                "BLOCK"=>{
                    if self.block.is_some()||self.entities.pending(){return Err("DXF-blok mist ENDBLK/SEQEND".into());}
                    let name=field(groups,2).ok_or("DXF BLOCK mist naam")?.to_owned();
                    let base=vertex(groups,10,20,30).map(crate::cad_geometry::xyz).unwrap_or([0.;3]);
                    crate::cad_geometry::finite(base)?;
                    self.block=Some(BlockStart{name,base,start:self.entities.features.len()});
                }
                "ENDBLK"=>{
                    if self.entities.pending(){return Err("DXF-blok mist SEQEND".into());}
                    let block=self.block.take().ok_or("DXF ENDBLK zonder BLOCK")?;
                    let key=block.name.to_lowercase();
                    if self.blocks.contains_key(&key){return Err("DXF bevat dubbele bloknamen".into());}
                    let features=self.entities.features.split_off(block.start);
                    self.blocks.insert(key,Arc::new(crate::cad_scene::Block{name:block.name,base:block.base,features}));
                }
                _=>{
                    if self.block.is_none(){return Err("DXF-blokentiteit buiten BLOCK/ENDBLK".into());}
                    self.entities.push(kind,groups)?;
                }
            },
            _=>{}
        }
        Ok(())
    }
}

fn parse_groups<'a>(records:impl Iterator<Item=Result<Group<'a>,String>>,path:&Path)->Result<DxfOverlay,String>{
    let mut parser=Parser{entities:Default::default(),layers:BTreeMap::new(),blocks:BTreeMap::new(),block:None};
    let mut kind=String::new();let mut groups=Vec::new();let mut section=String::new();
    let mut saw_section=false;let mut expect_section=false;let mut open=false;let mut eof=false;
    for (index,entry) in records.enumerate(){
        let entry=entry?;
        if index>=MAX_ENTITIES*40{return Err("DXF bevat te veel velden".into());}
        if eof&&entry.code!=999{return Err("DXF bevat gegevens na EOF".into());}
        if entry.code==0{
            if !kind.is_empty(){parser.flush(&section,&kind,&groups)?;groups.clear();}
            match entry.value.as_ref(){
                "SECTION"=>{
                    if open{return Err("DXF heeft geneste SECTION zonder ENDSEC".into());}
                    open=true;expect_section=true;section.clear();kind.clear();
                }
                "ENDSEC"|"EOF"=>{
                    if entry.value=="EOF"{if open{return Err("DXF mist ENDSEC voor EOF".into());}eof=true;}
                    else{if !open||expect_section{return Err("Ongeldige DXF ENDSEC".into());}open=false;}
                    if parser.entities.pending(){return Err("DXF-entiteit mist SEQEND".into());}
                    if parser.block.is_some(){return Err("DXF-blok mist ENDBLK".into());}
                    kind.clear();section.clear();
                }
                other=>kind=if matches!(section.as_str(),"ENTITIES"|"BLOCKS"|"TABLES"){other.to_owned()}else{String::new()},
            }
        }else if expect_section&&entry.code==2{
            section=entry.value.to_ascii_uppercase();expect_section=false;saw_section=true;
        }else if !kind.is_empty(){
            if groups.len()>=1_000_000{return Err("DXF-entiteit heeft meer dan 1 miljoen velden".into());}
            groups.push(entry);
        }
    }
    if !saw_section||!eof||open||expect_section{return Err("DXF afgebroken: SECTION/ENDSEC/EOF ontbreekt".into());}
    for feature in parser.entities.features.iter().chain(parser.blocks.values().flat_map(|b|b.features.iter())){
        parser.layers.entry(feature.layer.clone()).or_default();
    }
    let warnings=parser.entities.unsupported.iter().map(|(kind,count)|format!("{kind}: {count} entiteit(en) nog niet volledig weergegeven")).collect();
    let mut overlay=DxfOverlay{path:path.to_owned(),visible:true,features:parser.entities.features,layers:parser.layers,blocks:parser.blocks,
        warnings,..Default::default()};
    overlay.rebuild_block_cache()?;Ok(overlay)
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
        for edge in overlay.features[2].points.windows(2) {
            for point in edge {let radius=(point.x-5.).hypot(point.y-5.);assert!((radius-2.).abs()<1e-9);}
            let midpoint=((edge[0].x+edge[1].x)/2.,(edge[0].y+edge[1].y)/2.);
            assert!(2.-(midpoint.0-5.).hypot(midpoint.1-5.)<=0.25+1e-10);
        }
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
