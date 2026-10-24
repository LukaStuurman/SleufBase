//! Native PDOK BGT OGC API: wegdeel / onbegroeidterreindeel.
//! Geometry preserved as RD (EPSG:28992) polylines without a Python process.
//! Results are bounded, pagination links stay on the exact PDOK origin.

use crate::model::Bounds;
use reqwest::{blocking::Client,redirect,Url};
use serde::{Serialize,Deserialize};
use serde_json::Value;
use std::collections::HashSet;
use std::io::Read;
use std::time::Duration;

const API_ORIGIN:&str="https://api.pdok.nl/lv/bgt/ogc/v1/";
const RD_URI:&str="http://www.opengis.net/def/crs/EPSG/0/28992";
const MAX_PAGES:usize=200;
const MAX_PATHS:usize=100_000;
const MAX_RESPONSE_BYTES:u64=24*1024*1024;

#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
pub enum BgtKind { Wegdeel, OnbegroeidTerreindeel }
impl BgtKind {
    pub fn collection(self)->&'static str {
        match self {Self::Wegdeel=>"wegdeel",Self::OnbegroeidTerreindeel=>"onbegroeidterreindeel"}
    }
    pub fn label(self)->&'static str {
        match self {Self::Wegdeel=>"BGT wegdelen",Self::OnbegroeidTerreindeel=>"BGT terreinranden"}
    }
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BgtLayer {
    pub kind:BgtKind,
    pub bounds:Bounds,
    pub visible:bool,
    pub paths:Vec<Vec<(f64,f64)>>,
}

fn append_path(out:&mut Vec<Vec<(f64,f64)>>,coords:&Value,close:bool) {
    let Some(items)=coords.as_array() else{return;};
    let mut path=Vec::with_capacity(items.len().min(10000));
    if items.len()>10000 {return;}
    for item in items {
        let Some(pair)=item.as_array() else {return;};
        let Some(x)=pair.first().and_then(Value::as_f64) else{return;};
        let Some(y)=pair.get(1).and_then(Value::as_f64) else{return;};
        if !x.is_finite()||!y.is_finite(){return;}
        path.push((x,y));
    }
    if close && path.len()>=3 && path.first()!=path.last() {
        path.push(path[0]);
    }
    if path.len()>=2 {out.push(path);}
}
pub fn geometry_paths(geometry:&Value) -> Vec<Vec<(f64,f64)>> {
    let mut result=Vec::new();
    let coordinates=&geometry["coordinates"];
    match geometry.get("type").and_then(Value::as_str).unwrap_or("") {
        "LineString"=>append_path(&mut result,coordinates,false),
        "MultiLineString"|"Polygon"=>{
            if let Some(parts)=coordinates.as_array() {
                let close=geometry["type"]=="Polygon";
                for part in parts {append_path(&mut result,part,close);}
            }
        },
        "MultiPolygon"=>{
            if let Some(polygons)=coordinates.as_array() {
                for polygon in polygons {
                    if let Some(rings)=polygon.as_array() {
                        for ring in rings {append_path(&mut result,ring,true);}
                    }
                }
            }
        }
        _=>{}
    }
    result
}

pub fn parse_page(content:&str)->Result<(Vec<Vec<(f64,f64)>>,Option<String>),String>{
    let payload:Value=serde_json::from_str(content)
        .map_err(|e|format!("Ongeldige PDOK GeoJSON: {e}"))?;
    let features=payload.get("features").and_then(Value::as_array)
        .ok_or("PDOK gaf geen features-lijst terug")?;
    let mut paths=Vec::new();
    for item in features {
        if let Some(geometry)=item.get("geometry") {
            paths.extend(geometry_paths(geometry));
        }
        if paths.len()>MAX_PATHS {
            return Err("PDOK gaf te veel geometrieën in één pagina terug".into());
        }
    }
    let next=payload.get("links").and_then(Value::as_array).and_then(|items|{
        items.iter().find(|item|item.get("rel").and_then(Value::as_str)
            .is_some_and(|s|s.eq_ignore_ascii_case("next")))
            .and_then(|item|item.get("href")).and_then(Value::as_str)
            .map(str::to_owned)
    });
    Ok((paths,next))
}
fn safe_next(base:&Url,href:&str)->Result<Url,String>{
    let next=base.join(href).map_err(|_|"Ongeldige PDOK vervolg-URL".to_owned())?;
    if next.scheme()!="https" || next.host_str()!=Some("api.pdok.nl")
        || next.port_or_known_default()!=Some(443)
        || !next.path().starts_with("/lv/bgt/ogc/v1/collections/")
        || !next.username().is_empty() || next.password().is_some()
    {
        return Err("PDOK vervolg-URL heeft onveilige bestemming".into());
    }
    Ok(next)
}
pub fn build_url(kind:BgtKind,bounds:Bounds)->Result<Url,String>{
    let span_x=bounds.max_x-bounds.min_x;
    let span_y=bounds.max_y-bounds.min_y;
    if !span_x.is_finite() || !span_y.is_finite() ||
        span_x<=0.0 || span_y<=0.0 ||
        span_x>2000.0 || span_y>2000.0 {
        return Err("BGT-download vereist een RD-uitsnede van maximaal 2×2 km".into());
    }
    let mut url=Url::parse(API_ORIGIN).map_err(|e|e.to_string())?
        .join(&format!("collections/{}/items",kind.collection()))
        .map_err(|e|e.to_string())?;
    url.query_pairs_mut()
        .append_pair("f","json")
        .append_pair("bbox",&format!("{:.3},{:.3},{:.3},{:.3}",
            bounds.min_x,bounds.min_y,bounds.max_x,bounds.max_y))
        .append_pair("bbox-crs",RD_URI)
        .append_pair("crs",RD_URI)
        .append_pair("limit","1500");
    Ok(url)
}
#[derive(Debug)]
enum PageError{Retry(String),Fatal(String)}
fn fetch_page(client:&Client,url:Url,cancel:&std::sync::atomic::AtomicBool)->Result<(Vec<Vec<(f64,f64)>>,Option<String>),String>{
    let mut problem=String::new();
    for attempt in 0..3{
        let client=client.clone();let url=url.clone();
        let result=crate::cadastral::cancellable_request(cancel,move|stop|{
            crate::input::check(&stop).map_err(PageError::Fatal)?;
            let response=client.get(url).send().map_err(|e|PageError::Retry(format!("PDOK BGT ophalen: {e}")))?;
            if !response.status().is_success(){return Err(PageError::Retry(format!("PDOK BGT HTTP: {}",response.status())));}
            if response.content_length().is_some_and(|n|n>MAX_RESPONSE_BYTES){return Err(PageError::Fatal("BGT-pagina is te groot".into()));}
            let mut reader=response.take(MAX_RESPONSE_BYTES+1);let mut data=Vec::new();let mut chunk=[0u8;65536];
            loop{crate::input::check(&stop).map_err(PageError::Fatal)?;let n=reader.read(&mut chunk).map_err(|e|PageError::Fatal(e.to_string()))?;if n==0{break;}data.extend_from_slice(&chunk[..n]);if data.len() as u64>MAX_RESPONSE_BYTES{return Err(PageError::Fatal("BGT-pagina is te groot".into()));}}
            crate::input::check(&stop).map_err(PageError::Fatal)?;
            let content=String::from_utf8(data).map_err(|_|PageError::Fatal("Ongeldige PDOK UTF-8".into()))?;let result=parse_page(&content).map_err(PageError::Fatal)?;crate::input::check(&stop).map_err(PageError::Fatal)?;Ok(result)
        })?;
        match result{Ok(page)=>return Ok(page),Err(PageError::Fatal(error))=>return Err(error),Err(PageError::Retry(error))=>problem=error}
        if attempt<2{crate::cadastral::cancel_sleep(cancel,Duration::from_millis(700*(attempt+1)))?;}
    }
    Err(problem)
}
pub fn request_bgt(kind:BgtKind,bounds:Bounds)->Result<BgtLayer,String>{request_bgt_with_cancel(kind,bounds,&crate::input::NEVER_CANCEL)}
pub fn request_bgt_with_cancel(kind:BgtKind,bounds:Bounds,cancel:&std::sync::atomic::AtomicBool)->Result<BgtLayer,String>{
    crate::input::check(cancel)?;let url=build_url(kind,bounds)?;
    let client=Client::builder().timeout(Duration::from_secs(45)).redirect(redirect::Policy::none()).user_agent("SleufBase-Rust/0.1").build().map_err(|e|e.to_string())?;
    request_bgt_using(kind,bounds,cancel,&client,url)
}
fn request_bgt_using(kind:BgtKind,bounds:Bounds,cancel:&std::sync::atomic::AtomicBool,client:&Client,mut url:Url)->Result<BgtLayer,String>{
    let mut paths=Vec::new();
    let mut visited=HashSet::new();
    for _ in 0..MAX_PAGES {
        crate::input::check(cancel)?;
        if !visited.insert(url.to_string()){
            return Err("BGT API herhaalt dezelfde vervolgpagina".into());
        }
        let (parts,next)=fetch_page(client,url.clone(),cancel)?;
        paths.extend(parts);
        if paths.len()>MAX_PATHS {
            return Err("Te veel BGT-lijnen in deze uitsnede".into());
        }
        match next {
            Some(href)=>url=safe_next(&url,&href)?,
            None=> {
                if kind==BgtKind::Wegdeel {
                    let mut seen=HashSet::new();
                    let mut unique=Vec::new();
                    for p in paths{
                        crate::input::check(cancel)?;
                        let rounded:Vec<_>=p.iter().map(|(x,y)|
                            ((*x*1000.0).round() as i64,(*y*1000.0).round() as i64)
                        ).collect();
                        let reverse:Vec<_>=rounded.iter().rev().copied().collect();
                        let key=if rounded<reverse {rounded}else{reverse};
                        if seen.insert(key){unique.push(p);}
                    }
                    paths=unique;
                }
                crate::input::check(cancel)?;return Ok(BgtLayer{kind,bounds,visible:true,paths});
            }
        }
    }
    Err("BGT API paginalimiet bereikt".into())
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn multipolygon_rings_and_closure(){
        let geom=serde_json::json!({"type":"MultiPolygon","coordinates":[[
            [[1.,2.],[3.,2.],[3.,4.]],[[1.5,2.5],[2.,2.5],[2.,3.]]
        ]]});
        let paths=geometry_paths(&geom);
        assert_eq!(paths.len(),2);
        assert_eq!(paths[0].len(),4);
        assert_eq!(paths[0][0],paths[0][3]);
    }
    #[test]
    fn valid_rd_query_and_safe_next(){
        let b=Bounds{min_x:168000.,min_y:401000.,max_x:168200.,max_y:401100.};
        let url=build_url(BgtKind::Wegdeel,b).unwrap();
        assert!(url.as_str().contains("bbox-crs="));
        assert!(url.as_str().contains("168000.000"));
        assert!(safe_next(&url,"?offset=1500").is_ok());
        assert!(safe_next(&url,"https://attacker.invalid/").is_err());
        assert!(build_url(BgtKind::Wegdeel,Bounds{max_x:172000.,..b}).is_err());
    }
    #[test]
    fn response_parser_and_next(){
        let json=r#"{"features":[{"geometry":{"type":"LineString","coordinates":[[1,2],[3,4]]}}],
             "links":[{"rel":"next","href":"?offset=2"}]}"#;
        let (paths,next)=parse_page(json).unwrap();
        assert_eq!(paths[0],vec![(1.,2.),(3.,4.)]);
        assert_eq!(next.as_deref(),Some("?offset=2"));
        assert!(parse_page("{}").is_err());
    }
}


#[cfg(test)]
mod cancellation_tests{
    use super::*;
    use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
    fn bounds()->Bounds{Bounds{min_x:168000.,min_y:400000.,max_x:168040.,max_y:400030.}}
    fn client()->Client{Client::builder().no_proxy().timeout(Duration::from_secs(3)).redirect(redirect::Policy::none()).build().unwrap()}
    #[test]
    fn cancellation_returns_before_stalled_bgt_body_and_no_partial_result(){
        let(url,started,release,server)=crate::cadastral::http_test::stalled_body();let cancel=Arc::new(AtomicBool::new(false));let trigger=cancel.clone();
        let cancelling=std::thread::spawn(move||{started.recv_timeout(Duration::from_secs(3)).unwrap();trigger.store(true,Ordering::Release);});
        let start=std::time::Instant::now();let result=request_bgt_using(BgtKind::Wegdeel,bounds(),&cancel,&client(),Url::parse(&url).unwrap());let elapsed=start.elapsed();
        assert!(result.unwrap_err().contains("geannuleerd"));assert!(elapsed<Duration::from_secs(1),"cancellation took {elapsed:?}");release.send(()).unwrap();server.join().unwrap();cancelling.join().unwrap();
    }
    #[test]
    fn bgt_retries_transient_http_then_preserves_geometry(){
        let(url,requests,server)=crate::cadastral::http_test::scripted(vec![(503,"temporary".into()),(200,r#"{"features":[{"geometry":{"type":"LineString","coordinates":[[1,2],[3,4]]}}]}"#.into())]);
        let result=request_bgt_using(BgtKind::Wegdeel,bounds(),&AtomicBool::new(false),&client(),Url::parse(&url).unwrap()).unwrap();assert_eq!(result.paths,vec![vec![(1.,2.),(3.,4.)]]);server.join().unwrap();assert_eq!(requests.try_iter().count(),2);
    }
    #[test]
    fn cancellation_stops_bgt_retry_backoff_and_pre_cancelled_wrapper(){
        let(url,requests,server)=crate::cadastral::http_test::scripted(vec![(503,"temporary".into())]);let cancel=Arc::new(AtomicBool::new(false));let trigger=cancel.clone();let cancelling=std::thread::spawn(move||{requests.recv_timeout(Duration::from_secs(3)).unwrap();trigger.store(true,Ordering::Release);});
        let start=std::time::Instant::now();assert!(request_bgt_using(BgtKind::Wegdeel,bounds(),&cancel,&client(),Url::parse(&url).unwrap()).unwrap_err().contains("geannuleerd"));assert!(start.elapsed()<Duration::from_secs(1));server.join().unwrap();cancelling.join().unwrap();
        assert!(request_bgt_with_cancel(BgtKind::Wegdeel,bounds(),&AtomicBool::new(true)).unwrap_err().contains("geannuleerd"));
    }
    #[test]
    fn untrusted_pagination_is_rejected_before_followup_request(){
        let(url,requests,server)=crate::cadastral::http_test::scripted(vec![(200,r#"{"features":[],"links":[{"rel":"next","href":"http://untrusted.invalid/next"}]}"#.into())]);
        assert!(request_bgt_using(BgtKind::Wegdeel,bounds(),&AtomicBool::new(false),&client(),Url::parse(&url).unwrap()).unwrap_err().contains("onveilige bestemming"));server.join().unwrap();assert_eq!(requests.try_iter().count(),1);
    }
}
