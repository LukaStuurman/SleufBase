//! Native PDOK AHN WCS ground-level sampling (RD EPSG:28992).
//! Matches Python ahn.py request pattern: 2m/5px, 8m/9px, 24m/13px,
//! choose nearest finite non-nodata pixel and round to millimetres.

use reqwest::blocking::Client;
use reqwest::redirect;
use std::io::{Cursor,Read};
use std::time::Duration;
use std::{collections::VecDeque,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}}};
use tiff::decoder::{Decoder,DecodingResult};

const WCS_ENDPOINT:&str="https://service.pdok.nl/rws/ahn/wcs/v1_0";
const NO_DATA:f64=3.4028234663852886e38;
const MAX_TIFF:u64=16*1024*1024;

/// Only absent or explicitly automatic endpoint heights participate. Measured
/// MarXact layers and manually entered heights never receive automatic updates.
pub fn eligible_endpoints(trench:&crate::virtual_trench::VirtualTrench)->[bool;2]{
    if crate::virtual_trench_editor::is_measured(trench){return [false,false];}
    std::array::from_fn(|i|{let (point,meta)=if i==0{(trench.start,&trench.start_metadata)}else{(trench.end,&trench.end_metadata)};
        match crate::virtual_map::point_surface_mode(meta).as_str(){"auto"=>true,""=>point.z.is_none(),_=>false}
    })
}

pub fn build_url(x:f64,y:f64,size:f64,pixels:u32)->Result<reqwest::Url,String>{
    if !x.is_finite() || !y.is_finite() || x.abs()>1e12 || y.abs()>1e12 || !size.is_finite()
        || size<=0.0 || size>100.0 || pixels==0 || pixels>32 {
        return Err("AHN-aanvraag heeft ongeldige RD-coördinaten of schaal".into());
    }
    let mut url=reqwest::Url::parse(WCS_ENDPOINT).map_err(|e|e.to_string())?;
    let half=size*0.5;
    url.query_pairs_mut()
        .append_pair("SERVICE","WCS")
        .append_pair("VERSION","2.0.1")
        .append_pair("REQUEST","GetCoverage")
        .append_pair("CoverageID","dtm_05m")
        .append_pair("format","image/tiff")
        .append_pair("crs","http://www.opengis.net/def/crs/EPSG/0/28992")
        .append_pair("scalesize",&format!("x({pixels}),y({pixels})"))
        .append_pair("subset",&format!("x({:.3},{:.3})",x-half,x+half))
        .append_pair("subset",&format!("y({:.3},{:.3})",y-half,y+half));
    Ok(url)
}
pub fn nearest_valid_height(values:&[f64],width:usize,height:usize)->Option<f64> {
    nearest_valid_height_with_nodata(values,width,height,NO_DATA)
}
pub fn nearest_valid_height_with_nodata(values:&[f64],width:usize,height:usize,no_data:f64)->Option<f64> {
    if width==0 || height==0 || width.checked_mul(height)!=Some(values.len()) {return None;}
    let center_x=(width as f64-1.0)*0.5;
    let center_y=(height as f64-1.0)*0.5;
    let mut best=None;
    let mut best_dist=f64::INFINITY;
    for (i,value) in values.iter().enumerate() {
        if !value.is_finite() || (*value-no_data).abs().partial_cmp(&1e-6)!=Some(std::cmp::Ordering::Greater) ||
            value.partial_cmp(&(no_data*0.99))!=Some(std::cmp::Ordering::Less) {
            continue;
        }
        let x=(i%width) as f64;
        let y=(i/width) as f64;
        let dist=(x-center_x).powi(2)+(y-center_y).powi(2);
        if dist<best_dist {best_dist=dist;best=Some(*value);}
    }
    // Round the binary float directly with ties-to-even, as Python round(v,3).
    // Multiplying first can turn a value just below a tie into an exact tie.
    best.and_then(|v|format!("{v:.3}").parse().ok())
}
pub fn decode_ground_level(bytes:&[u8])->Result<Option<f64>,String>{
    if bytes.len() as u64>MAX_TIFF{return Err("AHN TIFF is groter dan 16 MB".into());}
    let mut decoder=Decoder::new(Cursor::new(bytes))
        .map_err(|e|format!("AHN TIFF ongeldig: {e}"))?;
    let (width,height)=decoder.dimensions().map_err(|e|e.to_string())?;
    let count=u64::from(width)*u64::from(height);
    if count==0 || count>1024 {
        return Err("AHN antwoord heeft onverwacht grote afmetingen".into());
    }
    let nodata=decoder.get_tag_ascii_string(tiff::tags::Tag::GdalNodata).ok()
        .and_then(|s|s.trim().trim_end_matches('\0').parse().ok()).unwrap_or(NO_DATA);
    let pixels=decoder.read_image().map_err(|e|format!("AHN pixels: {e}"))?;
    let values:Vec<f64>=match pixels {
        DecodingResult::F32(v)=>v.into_iter().map(f64::from).collect(),
        DecodingResult::F64(v)=>v,
        DecodingResult::I16(v)=>v.into_iter().map(f64::from).collect(),
        DecodingResult::U16(v)=>v.into_iter().map(f64::from).collect(),
        DecodingResult::U8(v)=>v.into_iter().map(f64::from).collect(),
        DecodingResult::I8(v)=>v.into_iter().map(f64::from).collect(),
        DecodingResult::I32(v)=>v.into_iter().map(f64::from).collect(),
        DecodingResult::U32(v)=>v.into_iter().map(f64::from).collect(),
        _=>return Err("AHN-raster heeft een niet-ondersteund TIFF pixeltype".into()),
    };
    if values.len()!=count as usize {
        return Err("AHN-raster heeft onverwacht aantal pixels".into());
    }
    Ok(nearest_valid_height_with_nodata(&values,width as usize,height as usize,nodata))
}

type Cache=VecDeque<([u64;2],Option<f64>)>;
/// One connection pool and bounded original 512-entry, two-decimal RD LRU.
/// Clones share the cache; no account or project settings are read or written.
#[derive(Clone)]
pub struct AhnClient {client:Client,cache:Arc<Mutex<Cache>>}
fn check(cancel:&AtomicBool)->Result<(),String>{if cancel.load(Ordering::Relaxed){Err("AHN-aanvulling geannuleerd".into())}else{Ok(())}}
fn key(x:f64,y:f64)->Result<[u64;2],String>{
    if !x.is_finite() || !y.is_finite() || x.abs()>1e12 || y.abs()>1e12{return Err("Ongeldige AHN RD-coördinaten".into());}
    let round=|v:f64|{let v=format!("{v:.2}").parse::<f64>().unwrap();if v==0.{0_f64.to_bits()}else{v.to_bits()}};
    Ok([round(x),round(y)])
}
impl AhnClient {
pub fn new()->Result<Self,String>{
    let client=Client::builder().timeout(Duration::from_secs(12))
        .redirect(redirect::Policy::none())
        .user_agent("SleufBase-Rust/0.1")
        .build().map_err(|e|e.to_string())?;
    Ok(Self{client,cache:Arc::new(Mutex::new(VecDeque::new()))})
}
pub fn lookup_cached(&self,x:f64,y:f64)->Result<Option<Option<f64>>,String>{
    let key=key(x,y)?;let mut cache=self.cache.lock().map_err(|_|"AHN-cache is niet beschikbaar")?;
    if let Some(i)=cache.iter().position(|(k,_)|*k==key){let entry=cache.remove(i).unwrap();let value=entry.1;cache.push_back(entry);Ok(Some(value))}else{Ok(None)}
}
fn fetch_with(&self,x:f64,y:f64,cancel:&AtomicBool,mut fetch:impl FnMut(reqwest::Url)->Result<Vec<u8>,String>)->Result<Option<f64>,String>{
    check(cancel)?;let key=key(x,y)?;
    if let Some(value)=self.lookup_cached(x,y)?{return Ok(value);}
    let mut value=None;
    for (size,pixels) in [(2.,5),(8.,9),(24.,13)] {
        check(cancel)?;
        let url=build_url(x,y,size,pixels)?;
        if let Ok(bytes)=fetch(url){
            check(cancel)?;
            if let Ok(Some(height))=decode_ground_level(&bytes){value=Some(height);break;}
        }
    }
    check(cancel)?;
    let mut cache=self.cache.lock().map_err(|_|"AHN-cache is niet beschikbaar")?;
    if let Some(i)=cache.iter().position(|(k,_)|*k==key){cache.remove(i);}
    cache.push_back((key,value));while cache.len()>512{cache.pop_front();}
    Ok(value)
}
pub fn ground_level(&self,x:f64,y:f64,cancel:&AtomicBool)->Result<Option<f64>,String>{
    self.fetch_with(x,y,cancel,|url|{
        let response=match self.client.get(url).send(){
            Ok(r)=>r,
            Err(e)=>return Err(e.to_string())
        };
        check(cancel)?;
        if !response.status().is_success(){return Err(format!("AHN HTTP {}",response.status()));}
        if response.content_length().is_some_and(|len|len>MAX_TIFF){return Err("AHN antwoord te groot".into());}
        if !response.headers().get("content-type")
            .and_then(|v|v.to_str().ok())
            .is_some_and(|v|v.to_ascii_lowercase().contains("image")){
            return Err("AHN antwoord bevat geen raster".into());
        }
        let mut bytes=Vec::new();
        if response.take(MAX_TIFF+1).read_to_end(&mut bytes).is_err()
            || bytes.len() as u64>MAX_TIFF {return Err("AHN raster kon niet begrensd worden ingelezen".into());}
        check(cancel)?;Ok(bytes)
    })
}
}
pub fn ground_level(x:f64,y:f64)->Result<Option<f64>,String>{
    AhnClient::new()?.ground_level(x,y,&AtomicBool::new(false))
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn original_case_whitespace_modes_follow_start_and_measured_manual_points_stay_protected(){
        use crate::{model::Vertex,virtual_trench::{VirtualTrench,TrenchPoint},virtual_map::{self,MapPoint}};
        let mut t=VirtualTrench::new("AHN".into(),Vertex{x:0.,y:0.,z:Some(4.)},Vertex{x:10.,y:0.,z:Some(6.)},0.6).unwrap();
        t.start_metadata=serde_json::json!({"surface_mode":"\u{1c} AuTo \t","surface_z":4.});t.end_metadata=serde_json::json!({"surface_mode":" MANUAL "});
        t.objects.push(TrenchPoint{metadata:serde_json::json!({"surface_mode":"\u{1c}START ","surface_z":4.}),role:"object".into(),object_name:"LS".into(),source_name:"ls".into(),position:Vertex{x:3.,y:0.,z:Some(3.)},attributes:Default::default()});
        assert_eq!(eligible_endpoints(&t),[true,false]);virtual_map::apply_surface_height(&mut t,MapPoint::Start,Some(5.)).unwrap();assert_eq!(t.start.z,Some(5.));assert_eq!(t.objects[0].position.z,Some(4.));assert_eq!(t.objects[0].metadata["surface_mode"],"start");assert_eq!(t.end.z,Some(6.));
        let before=serde_json::to_vec(&t).unwrap();assert!(!virtual_map::apply_surface_height(&mut t,MapPoint::End,Some(100.)).unwrap());assert_eq!(serde_json::to_vec(&t).unwrap(),before);
        t.boundary_3d=vec![t.start,t.end,Vertex{x:10.,y:1.,z:Some(6.)}];assert_eq!(eligible_endpoints(&t),[false,false]);let before=serde_json::to_vec(&t).unwrap();assert!(!virtual_map::apply_surface_height(&mut t,MapPoint::Start,Some(100.)).unwrap());assert_eq!(serde_json::to_vec(&t).unwrap(),before);
    }
    #[test]
    fn wcs_query_and_subset_repeat(){
        let url=build_url(168000.,401000.,2.,5).unwrap();
        let pairs:Vec<_>=url.query_pairs().collect();
        let subsets:Vec<_>=pairs.iter().filter(|(k,_)|k=="subset").map(|(_,v)|v.to_string()).collect();
        assert_eq!(subsets,vec!["x(167999.000,168001.000)","y(400999.000,401001.000)"]);
        assert!(build_url(f64::NAN,401000.,2.,5).is_err());
    }
    #[test]
    fn nearest_valid_skips_nodata_and_center(){
        let values=vec![
            NO_DATA,NO_DATA,NO_DATA,
            20.,NO_DATA,10.,
            NO_DATA,NO_DATA,NO_DATA
        ];
        // Equidistant: first valid scan order as numpy argmin.
        assert_eq!(nearest_valid_height(&values,3,3),Some(20.));
        assert_eq!(nearest_valid_height(&vec![NO_DATA;9],3,3),None);
        assert_eq!(nearest_valid_height(&[],0,0),None);
    }
    #[test]
    fn original_window_fallback_cache_negative_values_lru_and_cancel(){
        let client=AhnClient::new().unwrap();let cancel=AtomicBool::new(false);
        let bytes=include_bytes!("../tests/fixtures/ahn/sample-002.tif");
        let expected=decode_ground_level(bytes).unwrap();let mut calls=Vec::new();
        assert_eq!(client.fetch_with(168000.,401000.,&cancel,|url|{calls.push(url);if calls.len()<3{Err("offline".into())}else{Ok(bytes.to_vec())}}).unwrap(),expected);
        assert_eq!(calls.len(),3);assert!(calls[2].as_str().contains("x%2813%29"));
        assert_eq!(client.clone().fetch_with(168000.001,401000.001,&cancel,|_|panic!("shared cache must avoid HTTP")).unwrap(),expected);
        let mut none_calls=0;
        assert_eq!(client.fetch_with(1.,2.,&cancel,|_|{none_calls+=1;Err("offline".into())}).unwrap(),None);assert_eq!(none_calls,3);
        assert_eq!(client.fetch_with(1.,2.,&cancel,|_|panic!("negative result must be cached")).unwrap(),None);
        let data:serde_json::Value=serde_json::from_str(include_str!("../tests/fixtures/ahn/reference.json")).unwrap();
        for row in data["cache"].as_array().unwrap(){let got=key(row["x"].as_f64().unwrap(),row["y"].as_f64().unwrap()).unwrap();for i in 0..2{assert_eq!(f64::from_bits(got[i]),row["key"][i].as_f64().unwrap());}}
        for n in 0..513{client.fetch_with(n as f64,0.,&cancel,|_|Err("offline".into())).unwrap();}
        assert_eq!(client.cache.lock().unwrap().len(),512);
        assert_eq!(client.lookup_cached(0.,0.).unwrap(),None);
        cancel.store(true,Ordering::Relaxed);
        assert!(client.fetch_with(513.,0.,&cancel,|_|panic!("cancel before network")).is_err());
        assert_eq!(client.lookup_cached(513.,0.).unwrap(),None);
        assert!(client.ground_level(f64::NAN,0.,&AtomicBool::new(false)).is_err());
    }
}
