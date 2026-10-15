//! Native PDOK AHN WCS ground-level sampling (RD EPSG:28992).
//! Matches Python ahn.py request pattern: 2m/5px, 8m/9px, 24m/13px,
//! choose nearest finite non-nodata pixel and round to millimetres.

use reqwest::blocking::Client;
use reqwest::redirect;
use std::io::{Cursor,Read};
use std::time::Duration;
use tiff::decoder::{Decoder,DecodingResult};

const WCS_ENDPOINT:&str="https://service.pdok.nl/rws/ahn/wcs/v1_0";
const NO_DATA:f64=3.4028234663852886e38;
const MAX_TIFF:u64=16*1024*1024;

pub fn build_url(x:f64,y:f64,size:f64,pixels:u32)->Result<reqwest::Url,String>{
    if !x.is_finite() || !y.is_finite() || !size.is_finite()
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
    if width==0 || height==0 || values.len()!=width*height {return None;}
    let center_x=(width as f64-1.0)*0.5;
    let center_y=(height as f64-1.0)*0.5;
    let mut best=None;
    let mut best_dist=f64::INFINITY;
    for (i,value) in values.iter().enumerate() {
        if !value.is_finite() || (*value-NO_DATA).abs()<=1e-6 ||
            *value>=NO_DATA*0.99 {
            continue;
        }
        let x=(i%width) as f64;
        let y=(i/width) as f64;
        let dist=(x-center_x).powi(2)+(y-center_y).powi(2);
        if dist<best_dist {best_dist=dist;best=Some(*value);}
    }
    best.map(|v|(v*1000.0).round()/1000.0)
}
pub fn decode_ground_level(bytes:&[u8])->Result<Option<f64>,String>{
    let mut decoder=Decoder::new(Cursor::new(bytes))
        .map_err(|e|format!("AHN TIFF ongeldig: {e}"))?;
    let (width,height)=decoder.dimensions().map_err(|e|e.to_string())?;
    let count=u64::from(width)*u64::from(height);
    if count==0 || count>1024 {
        return Err("AHN antwoord heeft onverwacht grote afmetingen".into());
    }
    let pixels=decoder.read_image().map_err(|e|format!("AHN pixels: {e}"))?;
    let values:Vec<f64>=match pixels {
        DecodingResult::F32(v)=>v.into_iter().map(f64::from).collect(),
        DecodingResult::F64(v)=>v,
        DecodingResult::I16(v)=>v.into_iter().map(f64::from).collect(),
        DecodingResult::U16(v)=>v.into_iter().map(f64::from).collect(),
        DecodingResult::U8(v)=>v.into_iter().map(f64::from).collect(),
        _=>return Err("AHN-raster heeft een niet-ondersteund TIFF pixeltype".into()),
    };
    if values.len()!=count as usize {
        return Err("AHN-raster heeft onverwacht aantal pixels".into());
    }
    Ok(nearest_valid_height(&values,width as usize,height as usize))
}

pub fn ground_level(x:f64,y:f64)->Result<Option<f64>,String> {
    let client=Client::builder().timeout(Duration::from_secs(12))
        .redirect(redirect::Policy::none())
        .user_agent("SleufBase-Rust/0.1")
        .build().map_err(|e|e.to_string())?;
    for (size,pixels) in [(2.,5),(8.,9),(24.,13)] {
        let url=build_url(x,y,size,pixels)?;
        let mut response=match client.get(url).send(){
            Ok(r)=>r,
            Err(_)=>continue
        };
        if !response.status().is_success(){continue;}
        if response.content_length().is_some_and(|len|len>MAX_TIFF){continue;}
        if !response.headers().get("content-type")
            .and_then(|v|v.to_str().ok())
            .is_some_and(|v|v.to_ascii_lowercase().contains("image")){
            continue;
        }
        let mut bytes=Vec::new();
        if response.take(MAX_TIFF+1).read_to_end(&mut bytes).is_err()
            || bytes.len() as u64>MAX_TIFF {continue;}
        if let Ok(Some(height))=decode_ground_level(&bytes) {
            return Ok(Some(height));
        }
    }
    Ok(None)
}
#[cfg(test)]
mod tests{
    use super::*;
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
}
