//! Native Kadaster PDOK WFS 2.0 GeoJSON client.
//! Ref: Python cadastral_wfs.py; geometry and text labels on EPSG:28992.
//! Uses bounded pagination and small RD tiles. Real-service parity still requires
//! validation with representative cadastral drawing exports.

use crate::bgt::geometry_paths;
use crate::model::Bounds;
use reqwest::{blocking::Client,redirect,Url};
use serde::{Serialize,Deserialize};
use serde_json::Value;
use std::collections::HashSet;
use std::io::Read;
use std::thread;
use std::time::Duration;

const ENDPOINT:&str="https://service.pdok.nl/kadaster/kadastralekaart/wfs/v5_0";
const DIRECT_LIMIT:f64=700.0;
const TILE:f64=350.0;
const OVERLAP:f64=8.0;
const MAX_TILES:usize=100;
const MAX_PAGE_COUNT:usize=100;
const MAX_FEATURES:usize=150_000;
const MAX_BYTES:u64=24*1024*1024;

#[derive(Copy,Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
pub enum CadastralKind { Perceel,KadastraleGrens,Bebouwing,OpenbareRuimteNaam,Nummeraanduidingreeks }
impl CadastralKind {
    pub fn feature_type(self)->&'static str {
        match self {
            Self::Perceel=>"kadastralekaart:Perceel",
            Self::KadastraleGrens=>"kadastralekaart:KadastraleGrens",
            Self::Bebouwing=>"kadastralekaart:Bebouwing",
            Self::OpenbareRuimteNaam=>"kadastralekaart:OpenbareRuimteNaam",
            Self::Nummeraanduidingreeks=>"kadastralekaart:Nummeraanduidingreeks",
        }
    }
    pub fn layer_name(self)->&'static str {
        match self {
            Self::Perceel=>"KAD_PERCEEL",
            Self::KadastraleGrens=>"KAD_GRENS",
            Self::Bebouwing=>"KAD_BEBOUWING",
            Self::OpenbareRuimteNaam=>"KAD_STRAATNAAM",
            Self::Nummeraanduidingreeks=>"KAD_HUISNUMMER"
        }
    }
    pub fn is_label(self)->bool {
        matches!(self,Self::OpenbareRuimteNaam|Self::Nummeraanduidingreeks)
    }
    pub fn all()->[Self;5] {
        [Self::Perceel,Self::KadastraleGrens,Self::Bebouwing,
        Self::OpenbareRuimteNaam,Self::Nummeraanduidingreeks]
    }
}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct CadLinework {
    pub layer_name:String,
    pub paths:Vec<Vec<(f64,f64)>>
}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct CadLabel {
    pub layer_name:String,
    pub text:String,
    pub position:(f64,f64),
    pub rotation:f64,
}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct CadastralLayer {
    pub visible:bool,
    pub bounds:Bounds,
    pub linework:Vec<CadLinework>,
    pub labels:Vec<CadLabel>,
}
fn validate_bounds(bounds:Bounds)->Result<(),String>{
    if [bounds.min_x,bounds.min_y,bounds.max_x,bounds.max_y]
        .iter().any(|v|!v.is_finite())
        || bounds.max_x<=bounds.min_x || bounds.max_y<=bounds.min_y
    {
        return Err("Ongeldige RD-uitsnede".into());
    }
    if bounds.max_x-bounds.min_x>3500.0 || bounds.max_y-bounds.min_y>3500.0 {
        return Err("Kadastrale uitsnede groter dan 3,5 km; deel het gebied op".into());
    }
    Ok(())
}
pub fn build_url(kind:CadastralKind,bounds:Bounds,index:usize)
    ->Result<Url,String>
{
    validate_bounds(bounds)?;
    let mut url=Url::parse(ENDPOINT).map_err(|e|e.to_string())?;
    url.query_pairs_mut()
        .append_pair("service","WFS")
        .append_pair("version","2.0.0")
        .append_pair("request","GetFeature")
        .append_pair("typeNames",kind.feature_type())
        .append_pair("srsName","EPSG:28992")
        .append_pair("bbox",&format!("{},{},{},{},EPSG:28992",
            bounds.min_x,bounds.min_y,bounds.max_x,bounds.max_y))
        .append_pair("count","1000")
        .append_pair("startIndex",&index.to_string())
        .append_pair("outputFormat","application/json");
    Ok(url)
}
pub fn tiles(bounds:Bounds)->Result<Vec<Bounds>,String> {
    validate_bounds(bounds)?;
    let dx=bounds.max_x-bounds.min_x;
    let dy=bounds.max_y-bounds.min_y;
    if dx<=DIRECT_LIMIT && dy<=DIRECT_LIMIT {return Ok(vec![bounds]);}
    let cols=(dx/TILE).ceil() as usize;
    let rows=(dy/TILE).ceil() as usize;
    if cols.saturating_mul(rows)>MAX_TILES{
        return Err("Te veel PDOK-kadastertegels in één aanvraag".into());
    }
    let mut result=Vec::new();
    for col in 0..cols {
        for row in 0..rows {
            let x0=bounds.min_x+col as f64*TILE;
            let x1=(x0+TILE).min(bounds.max_x);
            let y0=bounds.min_y+row as f64*TILE;
            let y1=(y0+TILE).min(bounds.max_y);
            result.push(Bounds{
                min_x:(x0-OVERLAP).max(bounds.min_x),
                min_y:(y0-OVERLAP).max(bounds.min_y),
                max_x:(x1+OVERLAP).min(bounds.max_x),
                max_y:(y1+OVERLAP).min(bounds.max_y),
            });
        }
    }
    Ok(result)
}
pub fn parse_features(content:&str)->Result<Vec<Value>,String> {
    let value:Value=serde_json::from_str(content)
        .map_err(|e|format!("Kadaster WFS gaf ongeldige JSON: {e}"))?;
    let features=value.get("features").and_then(Value::as_array)
        .ok_or("Kadaster WFS gaf geen GeoJSON-featurelijst")?;
    if features.len()>10_000 {return Err("Kadaster WFS bevat te veel features in één pagina".into());}
    Ok(features.clone())
}
pub fn feature_label(feature:&Value,kind:CadastralKind)->Option<CadLabel> {
    if feature.get("geometry")?.get("type")?.as_str()?!="Point" {return None;}
    let coords=feature.get("geometry")?.get("coordinates")?.as_array()?;
    let x=coords.first()?.as_f64()?;
    let y=coords.get(1)?.as_f64()?;
    if !x.is_finite()||!y.is_finite(){return None;}
    let props=feature.get("properties")?;
    let text=props.get("tekst")?.as_str()?.trim();
    if text.is_empty(){return None;}
    let rotation=match props.get("hoek"){
        Some(Value::Number(v))=>v.as_f64().unwrap_or(0.),
        Some(Value::String(v))=>v.parse::<f64>().unwrap_or(0.),
        _=>0.
    };
    if !rotation.is_finite(){return None;}
    Some(CadLabel{
        layer_name:kind.layer_name().into(),
        text:text.to_owned(),position:(x,y),rotation
    })
}
fn fetch_kind(client:&Client,kind:CadastralKind,tiles:&[Bounds])
    ->Result<Vec<Value>,String>
{
    let mut all=Vec::new();
    let mut seen=HashSet::new();
    for bounds in tiles {
        let mut index=0;
        for page in 0..MAX_PAGE_COUNT {
            let url=build_url(kind,*bounds,index)?;
            let mut response=None;
            let mut problem=String::new();
            for attempt in 0..3 {
                match client.get(url.clone()).send() {
                    Ok(res) if res.status().is_success() =>{
                        response=Some(res);break;
                    }
                    Ok(res)=>problem=format!("WFS HTTP {}",res.status()),
                    Err(e)=>problem=e.to_string(),
                }
                if attempt<2{thread::sleep(Duration::from_millis(700*(attempt+1)));}
            }
            let Some(response)=response else {
                return Err(format!("{}: {problem}",kind.feature_type()));
            };
            if response.content_length().is_some_and(|len|len>MAX_BYTES){
                return Err("Kadaster WFS-pagina te groot".into());
            }
            let mut bytes=Vec::new();
            response.take(MAX_BYTES+1).read_to_end(&mut bytes)
                .map_err(|e|e.to_string())?;
            if bytes.len() as u64>MAX_BYTES {
                return Err("Kadaster WFS-pagina te groot".into());
            }
            let body=String::from_utf8(bytes)
                .map_err(|_|"Kadaster WFS stuurde geen UTF-8".to_owned())?;
            let values=parse_features(&body)?;
            let received=values.len();
            for value in values {
                let key=value.get("id").and_then(Value::as_str)
                    .map(str::to_owned).unwrap_or_else(||value.to_string());
                if seen.insert(key){all.push(value);}
                if all.len()>MAX_FEATURES {
                    return Err("Te veel kadastrale features in kaartgebied".into());
                }
            }
            if received<1000{break;}
            if page+1==MAX_PAGE_COUNT {
                return Err("Kadaster WFS heeft paginalimiet bereikt".into());
            }
            index+=1000;
        }
    }
    Ok(all)
}
pub fn fetch_cadastral(bounds:Bounds)->Result<CadastralLayer,String>{
    let bounds_tiles=tiles(bounds)?;
    let client=Client::builder()
        .timeout(Duration::from_secs(45))
        .redirect(redirect::Policy::none())
        .user_agent("SleufBase-Rust/0.1")
        .build().map_err(|e|e.to_string())?;
    let mut linework=Vec::new();
    let mut labels=Vec::new();
    for kind in CadastralKind::all() {
        let features=fetch_kind(&client,kind,&bounds_tiles)?;
        if kind.is_label() {
            labels.extend(features.iter().filter_map(|v|feature_label(v,kind)));
        }else{
            let mut paths=Vec::new();
            for feature in &features {
                if let Some(geometry)=feature.get("geometry"){
                    paths.extend(geometry_paths(geometry));
                }
            }
            if !paths.is_empty(){
                linework.push(CadLinework{layer_name:kind.layer_name().into(),paths});
            }
        }
    }
    Ok(CadastralLayer{visible:true,bounds,linework,labels})
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn wfs_parameters_and_tiles(){
        let bounds=Bounds{min_x:168000.,min_y:400000.,max_x:168040.,max_y:400030.};
        let url=build_url(CadastralKind::KadastraleGrens,bounds,1000).unwrap();
        assert!(url.as_str().contains("KadastraleGrens"));
        let query:std::collections::HashMap<_,_>=url.query_pairs().collect();
        assert_eq!(query["srsName"],"EPSG:28992");
        assert_eq!(query["startIndex"],"1000");
        assert_eq!(tiles(bounds).unwrap().len(),1);
        let big=Bounds{min_x:168000.,min_y:400000.,max_x:169000.,max_y:401000.};
        assert_eq!(tiles(big).unwrap().len(),9);
    }
    #[test]
    fn lines_and_text_labels(){
        let response=r#"{"features":[
            {"id":"1","geometry":{"type":"Polygon","coordinates":[[[0,0],[10,0],[10,10]]]}},
            {"id":"2","geometry":{"type":"Point","coordinates":[168000,401000]},
             "properties":{"tekst":"Dorpsstraat","hoek":35}}
        ]}"#;
        let features=parse_features(response).unwrap();
        let paths=geometry_paths(&features[0]["geometry"]);
        assert_eq!(paths[0].len(),4);
        assert_eq!(paths[0].first(),paths[0].last());
        let label=feature_label(&features[1],CadastralKind::OpenbareRuimteNaam).unwrap();
        assert_eq!(label.text,"Dorpsstraat");
        assert_eq!(label.rotation,35.0);
        assert!(parse_features("{}").is_err());
    }
}
