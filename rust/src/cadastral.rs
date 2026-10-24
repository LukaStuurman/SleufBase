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
use std::sync::{Arc,atomic::{AtomicBool,AtomicUsize,Ordering},mpsc};

const ENDPOINT:&str="https://service.pdok.nl/kadaster/kadastralekaart/wfs/v5_0";
const DIRECT_LIMIT:f64=700.0;
const TILE:f64=350.0;
const OVERLAP:f64=8.0;
const MAX_TILES:usize=100;
const MAX_PAGE_COUNT:usize=100;
const MAX_FEATURES:usize=150_000;
const MAX_BYTES:u64=24*1024*1024;
static ACTIVE_REQUESTS:AtomicUsize=AtomicUsize::new(0);
struct RequestPermit;
impl Drop for RequestPermit{fn drop(&mut self){ACTIVE_REQUESTS.fetch_sub(1,Ordering::AcqRel);}}
struct CancelRequest(Arc<AtomicBool>);
impl Drop for CancelRequest{fn drop(&mut self){self.0.store(true,Ordering::Release);}}
/// At most four bounded HTTP tasks may outlive their cancelled callers. The
/// caller never waits for a blocked socket; queued callers remain cancellable.
pub(crate) fn cancellable_request<T:Send+'static>(cancel:&AtomicBool,task:impl FnOnce(Arc<AtomicBool>)->T+Send+'static)->Result<T,String>{
    loop{crate::input::check(cancel)?;let active=ACTIVE_REQUESTS.load(Ordering::Acquire);if active<4&&ACTIVE_REQUESTS.compare_exchange_weak(active,active+1,Ordering::AcqRel,Ordering::Acquire).is_ok(){break;}thread::sleep(Duration::from_millis(20));}
    let permit=RequestPermit;let request_cancel=Arc::new(AtomicBool::new(false));let stop=CancelRequest(request_cancel.clone());let(tx,rx)=mpsc::sync_channel(1);
    thread::Builder::new().name("sleufbase-pdok-request".into()).spawn(move||{let _permit=permit;let result=task(request_cancel);let _=tx.send(result);}).map_err(|e|format!("PDOK-verzoek starten: {e}"))?;
    loop{crate::input::check(cancel)?;match rx.recv_timeout(Duration::from_millis(20)){Ok(result)=>{crate::input::check(cancel)?;drop(stop);return Ok(result)},Err(mpsc::RecvTimeoutError::Timeout)=>{},Err(mpsc::RecvTimeoutError::Disconnected)=>return Err("PDOK-verzoek onverwacht gestopt".into())}}
}
pub(crate) fn cancel_sleep(cancel:&AtomicBool,duration:Duration)->Result<(),String>{let until=std::time::Instant::now()+duration;loop{crate::input::check(cancel)?;let remaining=until.saturating_duration_since(std::time::Instant::now());if remaining.is_zero(){return Ok(());}thread::sleep(remaining.min(Duration::from_millis(20)));}}

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
#[serde(deny_unknown_fields)]
pub struct CadLinework {
    pub layer_name:String,
    pub paths:Vec<Vec<(f64,f64)>>
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CadLabel {
    pub layer_name:String,
    pub text:String,
    pub position:(f64,f64),
    pub rotation:f64,
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
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
#[derive(Debug)]
enum PageError {Retry(String),Fatal(String)}
fn request_features(client:&Client,url:Url,cancel:&AtomicBool)->Result<Result<Vec<Value>,PageError>,String>{
    let client=client.clone();
    cancellable_request(cancel,move|stop|{
        crate::input::check(&stop).map_err(PageError::Fatal)?;
        let response=client.get(url).send().map_err(|e|PageError::Retry(e.to_string()))?;
        if !response.status().is_success(){return Err(PageError::Retry(format!("WFS HTTP {}",response.status())));}
        if response.content_length().is_some_and(|len|len>MAX_BYTES){return Err(PageError::Fatal("Kadaster WFS-pagina te groot".into()));}
        let mut reader=response.take(MAX_BYTES+1);let mut bytes=Vec::new();let mut chunk=[0u8;65536];
        loop{crate::input::check(&stop).map_err(PageError::Fatal)?;let n=reader.read(&mut chunk).map_err(|e|PageError::Fatal(e.to_string()))?;if n==0{break;}bytes.extend_from_slice(&chunk[..n]);if bytes.len() as u64>MAX_BYTES{return Err(PageError::Fatal("Kadaster WFS-pagina te groot".into()));}}
        crate::input::check(&stop).map_err(PageError::Fatal)?;
        let body=String::from_utf8(bytes).map_err(|_|PageError::Fatal("Kadaster WFS stuurde geen UTF-8".into()))?;
        let values=parse_features(&body).map_err(PageError::Fatal)?;crate::input::check(&stop).map_err(PageError::Fatal)?;Ok(values)
    })
}
fn fetch_kind(client:&Client,kind:CadastralKind,tiles:&[Bounds],cancel:&AtomicBool,endpoint:&str)->Result<Vec<Value>,String>{
    let mut all=Vec::new();let mut seen=HashSet::new();
    for bounds in tiles{
        crate::input::check(cancel)?;let mut index=0;
        for page in 0..MAX_PAGE_COUNT{
            crate::input::check(cancel)?;let mut url=build_url(kind,*bounds,index)?;
            if endpoint!=ENDPOINT{let mut local=Url::parse(endpoint).map_err(|e|e.to_string())?;local.set_query(url.query());url=local;}
            let mut values=None;let mut problem=String::new();
            for attempt in 0..3{
                match request_features(client,url.clone(),cancel)?{
                    Ok(page)=>{values=Some(page);break;},Err(PageError::Fatal(error))=>return Err(error),Err(PageError::Retry(error))=>problem=error,
                }
                if attempt<2{cancel_sleep(cancel,Duration::from_millis(700*(attempt+1)))?;}
            }
            let values=values.ok_or_else(||format!("{}: {problem}",kind.feature_type()))?;let received=values.len();
            for value in values{crate::input::check(cancel)?;let key=value.get("id").and_then(Value::as_str).map(str::to_owned).unwrap_or_else(||value.to_string());if seen.insert(key){all.push(value);}if all.len()>MAX_FEATURES{return Err("Te veel kadastrale features in kaartgebied".into());}}
            if received<1000{break;}if page+1==MAX_PAGE_COUNT{return Err("Kadaster WFS heeft paginalimiet bereikt".into());}index+=1000;
        }
    }
    crate::input::check(cancel)?;Ok(all)
}
pub fn fetch_cadastral(bounds:Bounds)->Result<CadastralLayer,String>{
    fetch_cadastral_with_cancel(bounds,&crate::input::NEVER_CANCEL)
}
pub fn fetch_cadastral_with_cancel(bounds:Bounds,cancel:&AtomicBool)->Result<CadastralLayer,String>{
    crate::input::check(cancel)?;
    let bounds_tiles=tiles(bounds)?;
    let client=Client::builder()
        .timeout(Duration::from_secs(45))
        .redirect(redirect::Policy::none())
        .user_agent("SleufBase-Rust/0.1")
        .build().map_err(|e|e.to_string())?;
    let mut linework=Vec::new();
    let mut labels=Vec::new();
    for kind in CadastralKind::all() {
        crate::input::check(cancel)?;
        let features=fetch_kind(&client,kind,&bounds_tiles,cancel,ENDPOINT)?;
        if kind.is_label() {
            for feature in &features{crate::input::check(cancel)?;if let Some(label)=feature_label(feature,kind){labels.push(label);}}
        }else{
            let mut paths=Vec::new();
            for feature in &features {
                crate::input::check(cancel)?;
                if let Some(geometry)=feature.get("geometry"){
                    paths.extend(geometry_paths(geometry));
                }
            }
            if !paths.is_empty(){
                linework.push(CadLinework{layer_name:kind.layer_name().into(),paths});
            }
        }
    }
    crate::input::check(cancel)?;Ok(CadastralLayer{visible:true,bounds,linework,labels})
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


#[cfg(test)]
pub(crate) mod http_test {
    use std::{io::{Read,Write},net::TcpListener,sync::mpsc,time::Duration};
    pub fn scripted(responses:Vec<(u16,String)>)->(String,mpsc::Receiver<String>,std::thread::JoinHandle<()>){
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();let url=format!("http://{}",listener.local_addr().unwrap());let(tx,rx)=mpsc::channel();
        let server=std::thread::spawn(move||for(status,body)in responses{
            let(mut socket,_)=listener.accept().unwrap();socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();let mut request=Vec::new();let mut data=[0u8;4096];loop{let n=socket.read(&mut data).unwrap();if n==0{break;}request.extend_from_slice(&data[..n]);if request.windows(4).any(|w|w==b"\r\n\r\n"){break;}}
            let _=tx.send(String::from_utf8(request).unwrap());let _=write!(socket,"HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
        });(url,rx,server)
    }
    pub fn stalled_body()->(String,mpsc::Receiver<()>,mpsc::Sender<()>,std::thread::JoinHandle<()>){
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();let url=format!("http://{}",listener.local_addr().unwrap());let(tx,rx)=mpsc::channel();let(release,wait)=mpsc::channel();
        let server=std::thread::spawn(move||{let(mut socket,_)=listener.accept().unwrap();socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();let mut buffer=[0u8;4096];let _=socket.read(&mut buffer);let body=r#"{"features":[]}"#;write!(socket,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();let _=tx.send(());let _=wait.recv_timeout(Duration::from_secs(3));let _=socket.write_all(body.as_bytes());});(url,rx,release,server)
    }
}
#[cfg(test)]
mod cancellation_tests{
    use super::*;
    fn bounds()->Bounds{Bounds{min_x:168000.,min_y:400000.,max_x:168040.,max_y:400030.}}
    fn client()->Client{Client::builder().no_proxy().timeout(Duration::from_secs(3)).redirect(redirect::Policy::none()).build().unwrap()}
    #[test]
    fn cancellation_returns_before_stalled_response_body_and_no_partial_layer(){
        let(url,started,release,server)=http_test::stalled_body();let cancel=Arc::new(AtomicBool::new(false));let trigger=cancel.clone();
        let cancelling=thread::spawn(move||{started.recv_timeout(Duration::from_secs(3)).unwrap();trigger.store(true,Ordering::Release);});
        let start=std::time::Instant::now();let result=fetch_kind(&client(),CadastralKind::Perceel,&[bounds()],&cancel,&url);let elapsed=start.elapsed();
        assert!(result.unwrap_err().contains("geannuleerd"));assert!(elapsed<Duration::from_secs(1),"cancellation took {elapsed:?}");release.send(()).unwrap();server.join().unwrap();cancelling.join().unwrap();
    }
    #[test]
    fn pagination_preserves_start_index_and_deduplicates_page_overlap(){
        let features=(0..1000).map(|i|serde_json::json!({"id":format!("p{i}")})).collect::<Vec<_>>();
        let(url,requests,server)=http_test::scripted(vec![(200,serde_json::json!({"features":features}).to_string()),(200,r#"{"features":[{"id":"p0"}]}"#.into())]);
        let rows=fetch_kind(&client(),CadastralKind::Perceel,&[bounds()],&AtomicBool::new(false),&url).unwrap();assert_eq!(rows.len(),1000);server.join().unwrap();let requests=requests.try_iter().collect::<Vec<_>>();assert_eq!(requests.len(),2);assert!(requests[0].contains("startIndex=0"));assert!(requests[1].contains("startIndex=1000"));
    }
    #[test]
    fn cancellation_interrupts_retry_backoff_before_second_http_request(){
        let(url,requests,server)=http_test::scripted(vec![(503,"temporary".into())]);let cancel=Arc::new(AtomicBool::new(false));let trigger=cancel.clone();let cancelling=thread::spawn(move||{requests.recv_timeout(Duration::from_secs(3)).unwrap();trigger.store(true,Ordering::Release);});
        let start=std::time::Instant::now();assert!(fetch_kind(&client(),CadastralKind::Perceel,&[bounds()],&cancel,&url).unwrap_err().contains("geannuleerd"));assert!(start.elapsed()<Duration::from_secs(1));server.join().unwrap();cancelling.join().unwrap();
    }
    #[test]
    fn pre_cancelled_public_wrapper_never_starts_network(){assert!(fetch_cadastral_with_cancel(bounds(),&AtomicBool::new(true)).unwrap_err().contains("geannuleerd"));}
}
