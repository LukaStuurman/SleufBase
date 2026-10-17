//! Native PDOK aerial WMS integration (EPSG:28992).
//! Uses the same default PDOK source and WMS 1.1.1 parameters as Python pdok.py.
//! The source URL is fixed; this module never forwards arbitrary endpoints.

use crate::model::Bounds;
use std::time::Duration;

const PDOK_WMS_URL:&str="https://service.pdok.nl/hwh/luchtfotorgb/wms/v1_0";
const MAX_RESPONSE_BYTES:usize=24*1024*1024;
const MAX_OUTPUT_PIXELS:u64=2_048*2_048;

#[derive(Clone, Debug)]
pub struct PdokRaster {
    pub bounds: Bounds,
    pub width:u32,
    pub height:u32,
    pub rgba:Vec<u8>
}

fn validate(bounds:Bounds,width:u32,height:u32)->Result<(),String> {
    if width==0 || height==0 || width>2048 || height>2048 ||
        u64::from(width)*u64::from(height)>MAX_OUTPUT_PIXELS {
        return Err("PDOK WMS-tegel moet 1 tot 2048 pixels per zijde zijn".into());
    }
    if [bounds.min_x,bounds.min_y,bounds.max_x,bounds.max_y].iter().any(|x|!x.is_finite()) ||
        bounds.min_x>=bounds.max_x || bounds.min_y>=bounds.max_y {
        return Err("BBOX-coördinaten zijn ongeldig".into());
    }
    Ok(())
}

pub fn build_wms_url(bounds:Bounds,width:u32,height:u32)->Result<reqwest::Url,String> {
    validate(bounds,width,height)?;
    let mut url=reqwest::Url::parse(PDOK_WMS_URL).map_err(|e|e.to_string())?;
    {
        let mut query=url.query_pairs_mut();
        query.append_pair("SERVICE","WMS");
        query.append_pair("VERSION","1.1.1");
        query.append_pair("REQUEST","GetMap");
        query.append_pair("LAYERS","Actueel_orthoHR");
        query.append_pair("STYLES","");
        query.append_pair("FORMAT","image/png");
        query.append_pair("TRANSPARENT","FALSE");
        query.append_pair("SRS","EPSG:28992");
        query.append_pair("WIDTH",&width.to_string());
        query.append_pair("HEIGHT",&height.to_string());
        query.append_pair("BBOX",&format!("{},{},{},{}",
            bounds.min_x,bounds.min_y,bounds.max_x,bounds.max_y));
    }
    Ok(url)
}

pub fn request_map(bounds:Bounds,width:u32,height:u32)->Result<PdokRaster,String> {
    let url=build_wms_url(bounds,width,height)?;
    let client=reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("SleufBase-Rust/0.1")
        .build().map_err(|e|format!("Netwerkclient mislukt: {e}"))?;
    let response=client.get(url).send()
        .map_err(|e|format!("PDOK onbereikbaar: {e}"))?
        .error_for_status().map_err(|e|format!("PDOK HTTP-fout: {e}"))?;
    if response.content_length().is_some_and(|len|len>MAX_RESPONSE_BYTES as u64) {
        return Err("PDOK-antwoord is onverwacht groot".into());
    }
    let bytes=response.bytes().map_err(|e|format!("PDOK afbeelding lezen mislukt: {e}"))?;
    if bytes.len()>MAX_RESPONSE_BYTES || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("PDOK leverde geen geldige PNG of een te groot bestand".into());
    }
    let decoded=image::load_from_memory_with_format(
        &bytes,image::ImageFormat::Png
    ).map_err(|e|format!("PDOK PNG ongeldig: {e}"))?;
    if decoded.width()!=width || decoded.height()!=height {
        return Err(format!("PDOK leverde onverwachte dimensies {}x{}",decoded.width(),decoded.height()));
    }
    Ok(PdokRaster {
        bounds,width,height,rgba:decoded.to_rgba8().into_raw()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pdok_url_matches_legacy_parameters() {
        let b=Bounds{min_x:160000.,min_y:400000.,max_x:161000.,max_y:401000.};
        let url=build_wms_url(b,1024,768).unwrap();
        assert_eq!(url.host_str(),Some("service.pdok.nl"));
        let query:std::collections::HashMap<_,_>=url.query_pairs().collect();
        assert_eq!(query["LAYERS"],"Actueel_orthoHR");
        assert_eq!(query["SRS"],"EPSG:28992");
        assert_eq!(query["BBOX"],"160000,400000,161000,401000");
        assert_eq!(query["WIDTH"],"1024");
    }
    #[test]
    fn rejects_invalid_dimensions() {
        let b=Bounds{min_x:1.,min_y:2.,max_x:3.,max_y:4.};
        assert!(build_wms_url(b,0,10).is_err());
        assert!(build_wms_url(b,4096,4096).is_err());
    }
}
