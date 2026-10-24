//! Gemma reads three numbered map markers into editable profile texts. It does
//! not infer numeric AHN elevations or mutate a project on the network thread.
use crate::{Bounds,vllm_runtime};
use image::{RgbaImage,RgbImage};
use serde_json::{Value,json};
use std::{collections::BTreeMap,io::{Cursor,Read},sync::atomic::AtomicBool,time::Duration};
pub const DEFAULT_MAAIVELD_COLOR:&str="#808080";
pub const PROMPT:&str=concat!("Lees de afbeelding van een proefsleuf. Er staan drie witte genummerde markers op: ",
    "1 = begin, 2 = midden, 3 = einde. Bepaal per marker het maaiveld/N.A.P.-niveau dat bij die plek hoort. ",
    "Geef alleen geldig JSON terug met exact deze vorm: ",
    "{\"1\":\"Maaiveld: 0.00 N.A.P.\",\"2\":\"Maaiveld: 0.00 N.A.P.\",\"3\":\"Maaiveld: 0.00 N.A.P.\"}. ",
    "Gebruik een komma als decimaalteken alleen als die zo in de afbeelding staat; anders punt. ",
    "Als je een waarde niet betrouwbaar kunt bepalen, zet dan INVULLEN.");
#[derive(Clone,Debug,serde::Serialize,serde::Deserialize)]
pub struct Point {pub key:String,pub label:String,pub x:f64,pub y:f64}
#[derive(Clone,Debug,serde::Serialize,serde::Deserialize)]
pub struct Result {pub text_by_key:BTreeMap<String,String>,pub raw_response:String}
#[derive(Clone)]
pub struct Config {pub runtime:vllm_runtime::Config,pub api_key:String,pub timeout:Duration}
impl Config {
    pub fn from_env(root:std::path::PathBuf)->Self{Self{runtime:vllm_runtime::Config::from_env(root),api_key:std::env::var("SLEUFBASE_VLLM_API_KEY").unwrap_or_else(|_|"EMPTY".into()),timeout:Duration::from_secs(180)}}
}
pub fn number_key_to_segment_key(key:&str)->Option<&'static str>{match crate::python_values::trim(key){"1"=>Some("start"),"2"=>Some("middle"),"3"=>Some("end"),_=>None}}
pub fn parse_maaiveld_json(raw:&str)->std::result::Result<BTreeMap<String,String>,String>{
    if raw.len()>1024*1024{return Err("vLLM-tekst is groter dan 1 MB".into());}
    let mut candidate=crate::python_values::trim(raw);
    let regex=regex::Regex::new(r"(?is)```(?:json)?\s*(.*?)```").unwrap();
    if let Some(c)=regex.captures(candidate){candidate=c.get(1).unwrap().as_str().trim();}
    if !candidate.starts_with('{'){
        if let (Some(start),Some(end))=(candidate.find('{'),candidate.rfind('}')){if end>start{candidate=&candidate[start..=end];}}
    }
    let value:Value=serde_json::from_str(candidate).map_err(|_|format!("vLLM gaf geen bruikbare maaiveld-JSON terug: {}",raw.chars().take(300).collect::<String>()))?;
    let object=value.as_object().ok_or("vLLM-JSON moet een object met keys 1, 2 en 3 zijn.")?;
    let mut result=BTreeMap::new();
    for key in ["1","2","3"]{
        let value=object.get(key).filter(|v|crate::python_values::truth(v));
        let text=crate::python_values::string(value)?;let text=crate::python_values::trim(&text);
        result.insert(key.into(),if text.is_empty(){"INVULLEN".into()}else{text.into()});
    }
    Ok(result)
}
pub fn parse_response(value:&Value)->std::result::Result<Result,String>{
    let content=value.get("choices").and_then(Value::as_array).and_then(|v|v.first()).and_then(|v|v.get("message")).and_then(|v|v.get("content")).ok_or("vLLM-response bevat geen chatbericht.")?;
    let raw=if let Some(parts)=content.as_array(){let mut text=String::new();for part in parts{let value=if part.is_object(){part.get("text")}else{Some(part)};text.push_str(&match value{Some(Value::String(s))=>s.clone(),_=>crate::python_values::string(value)?});}text}else if crate::python_values::truth(content){crate::python_values::string(Some(content))?}else{String::new()};
    let raw=crate::python_values::trim(&raw).to_owned();Ok(Result{text_by_key:parse_maaiveld_json(&raw)?,raw_response:raw})
}
fn base64(bytes:&[u8])->String{
    const T:&[u8]=b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out=String::with_capacity(bytes.len().div_ceil(3)*4);
    for chunk in bytes.chunks(3){let a=chunk[0];let b=chunk.get(1).copied().unwrap_or(0);let c=chunk.get(2).copied().unwrap_or(0);out.push(T[(a>>2)as usize]as char);out.push(T[(((a&3)<<4)|(b>>4))as usize]as char);out.push(if chunk.len()>1{T[(((b&15)<<2)|(c>>6))as usize]as char}else{'='});out.push(if chunk.len()>2{T[(c&63)as usize]as char}else{'='});}out
}
pub fn payload(image:&RgbImage,model:&str)->std::result::Result<Value,String>{
    if image.width()==0||image.height()==0||u64::from(image.width())*u64::from(image.height())>20_000_000{return Err("Ongeldige of te grote maaiveldafbeelding".into());}
    let mut bytes=Cursor::new(Vec::new());image.write_to(&mut bytes,image::ImageFormat::Png).map_err(|e|e.to_string())?;
    Ok(json!({"model":model,"messages":[{"role":"user","content":[{"type":"text","text":PROMPT},{"type":"image_url","image_url":{"url":format!("data:image/png;base64,{}",base64(bytes.get_ref()))}}]}],"temperature":0,"max_tokens":256}))
}
#[derive(Default)]
pub struct Client {runtime:vllm_runtime::Runtime}
impl Client {
    pub fn request(&self,image:&RgbImage,config:&Config,cancel:&AtomicBool)->std::result::Result<Result,String>{
        vllm_runtime::check(cancel)?;
        if config.runtime.model.trim().is_empty(){return Err("Geen vLLM-model ingesteld.".into());}
        let root=self.runtime.ensure(&config.runtime,cancel)?;
        let body=payload(image,&config.runtime.model)?;let token=config.api_key.clone();let timeout=config.timeout;
        // Share the bounded HTTP task permits with map/context requests. A
        // cancelled socket can finish in the background, but repeated cancel
        // and restart cannot create an unbounded number of network threads.
        crate::cadastral::cancellable_request(cancel,move|stop|{
                vllm_runtime::check(&stop)?;
                let client=reqwest::blocking::Client::builder().timeout(timeout).redirect(reqwest::redirect::Policy::none()).build().map_err(|e|e.to_string())?;
                let mut request=client.post(format!("{}/chat/completions",root.trim_end_matches('/'))).json(&body);
                if !token.is_empty(){request=request.bearer_auth(token);}
                let response=request.send().and_then(|r|r.error_for_status()).map_err(|e|format!("vLLM-aanroep mislukt: {e}"))?;
                vllm_runtime::check(&stop)?;
                let mut bytes=Vec::new();response.take(1024*1024+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
                if bytes.len()>1024*1024{return Err("vLLM-response is groter dan 1 MB".into());}
                let value=serde_json::from_slice(&bytes).map_err(|_|"vLLM gaf geen geldige JSON-response terug.")?;
                vllm_runtime::check(&stop)?;
                parse_response(&value)
        })?
    }
}
pub fn numbered_layout(layer:Bounds,points:&[Point],size:[u32;2])->std::result::Result<Bounds,String>{
    if points.is_empty(){return Err("Geen maaiveldpunten beschikbaar om te markeren.".into());}
    if size.contains(&0)||size.iter().any(|v|*v>8192){return Err("Ongeldig maaiveldkaartformaat".into());}
    let mut b=layer;
    for p in points{if !p.x.is_finite()||!p.y.is_finite()||p.label.len()>4096{return Err("Ongeldig maaiveldpunt".into());}b.min_x=b.min_x.min(p.x);b.min_y=b.min_y.min(p.y);b.max_x=b.max_x.max(p.x);b.max_y=b.max_y.max(p.y);}
    if [b.min_x,b.min_y,b.max_x,b.max_y].iter().any(|v|!v.is_finite())||b.min_x>b.max_x||b.min_y>b.max_y{return Err("Ongeldige maaiveldkaartgrenzen".into());}
    let padding=((b.max_x-b.min_x).max(b.max_y-b.min_y)*0.18).clamp(2.,10.);
    b.min_x-=padding;b.max_x+=padding;b.min_y-=padding;b.max_y+=padding;
    let aspect=size[0]as f64/size[1]as f64;let w=b.max_x-b.min_x;let h=b.max_y-b.min_y;
    if w/h<aspect{let extra=(h*aspect-w)/2.;b.min_x-=extra;b.max_x+=extra;}else{let extra=(w/aspect-h)/2.;b.min_y-=extra;b.max_y+=extra;}
    Ok(b)
}
/// The caller renders the real map/TIFF/CAD/background into `base` at these
/// bounds. This function adds the original 1/2/3 markers and their labels.
pub fn build_numbered_image(base:&RgbaImage,map_bounds:Bounds,points:&[Point],cancel:&AtomicBool)->std::result::Result<RgbImage,String>{
    if points.len()!=3{return Err("Gemma verwacht begin, midden en einde (drie punten)".into());}
    if base.width()==0||base.height()==0||u64::from(base.width())*u64::from(base.height())>20_000_000{return Err("Ongeldige maaiveldafbeelding".into());}
    let width=map_bounds.max_x-map_bounds.min_x;let height=map_bounds.max_y-map_bounds.min_y;
    if !width.is_finite()||!height.is_finite()||width<=0.||height<=0.{return Err("Ongeldige maaiveldkaartgrenzen".into());}
    let folder=std::env::var_os("WINDIR").map(std::path::PathBuf::from).unwrap_or_else(||"C:/Windows".into()).join("Fonts");
    let paths=crate::map_text::Paths{regular:folder.join("arial.ttf"),bold:folder.join("arialbd.ttf")};
    let mut result=base.clone();let mut fonts=crate::map_text::Fonts::load(Some(&paths),cancel).or_else(|_|crate::map_text::Fonts::load(None,cancel))?;let mut budget=30_000_000;
    for (i,p) in points.iter().enumerate(){
        vllm_runtime::check(cancel)?;
        if !p.x.is_finite()||!p.y.is_finite()||p.label.len()>4096{return Err("Ongeldig maaiveldpunt".into());}
        let x=((p.x-map_bounds.min_x)/width*base.width()as f64).round_ties_even();
        let y=((map_bounds.max_y-p.y)/height*base.height()as f64).round_ties_even();
        crate::map_canvas::ellipse(&mut result,[x-34.,y-34.,x+34.,y+34.],[255,255,255,245],None,cancel,&mut budget)?;
        crate::map_canvas::ellipse(&mut result,[x-34.,y-34.,x+34.,y+34.],[0,0,0,255],Some(5),cancel,&mut budget)?;
        let text=(i+1).to_string();let bounds=fonts.measure(&text,48.,true)?;
        fonts.draw(&mut result,&text,[x-(bounds[2]-bounds[0])/2.,y-(bounds[3]-bounds[1])/2.-4.],48.,true,"la",[0,0,0,255],None,0.,cancel)?;
        let bounds=fonts.measure(&p.label,24.,true)?;let w=bounds[2]-bounds[0];let h=bounds[3]-bounds[1];
        let lx=(x-w/2.).min(base.width()as f64-w-8.).max(8.);let ly=(y+42.).min(base.height()as f64-h-8.);
        // Rounded label backing, drawn once at full map resolution.
        rounded_backing(&mut result,[lx-8.,ly-4.,lx+w+8.,ly+h+6.],6.);
        fonts.draw(&mut result,&p.label,[lx,ly],24.,true,"la",[0,0,0,255],None,0.,cancel)?;
    }
    vllm_runtime::check(cancel)?;Ok(image::DynamicImage::ImageRgba8(result).into_rgb8())
}
fn rounded_backing(image:&mut RgbaImage,b:[f64;4],r:f64){
    let [l,t,right,bottom]=b;
    for y in (t as i32).max(0)..=(bottom as i32).min(image.height()as i32-1){for x in (l as i32).max(0)..=(right as i32).min(image.width()as i32-1){let cx=(x as f64).clamp(l+r,right-r);let cy=(y as f64).clamp(t+r,bottom-r);if (x as f64-cx).powi(2)+(y as f64-cy).powi(2)<=r*r{image.put_pixel(x as u32,y as u32,image::Rgba([255,255,255,225]));}}}
}
#[cfg(test)] mod tests{
    use super::*;
    #[test]fn fenced_embedded_missing_and_non_string_original_values(){
        let got=parse_maaiveld_json("Tekst ```JSON\n{\"1\":\"  NAP: -2,31  \",\"2\":false,\"3\":[1,true]}\n``` suffix").unwrap();assert_eq!(got["1"],"NAP: -2,31");assert_eq!(got["2"],"INVULLEN");assert_eq!(got["3"],"[1, True]");
        assert_eq!(parse_maaiveld_json("prefix {\"1\":5} trailing").unwrap()["1"],"5");assert!(parse_maaiveld_json("[]").is_err());assert!(parse_response(&json!({"choices":[]})).is_err());
        assert_eq!(base64(b"hello"),"aGVsbG8=");assert_eq!(number_key_to_segment_key(" 2 "),Some("middle"));
        let body=payload(&RgbImage::new(2,2),"gemma").unwrap();assert_eq!(body["messages"][0]["content"][0]["text"],PROMPT);assert!(body["messages"][0]["content"][1]["image_url"]["url"].as_str().unwrap().starts_with("data:image/png;base64,iVBOR"));
    }
    #[test]fn local_fake_openai_http_protocol_and_response_without_runtime_start(){
        use std::io::{Read,Write};
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let address=listener.local_addr().unwrap();
        let server=std::thread::spawn(move||{
            for expected in ["GET /v1/models ","POST /v1/chat/completions "]{
                let(mut stream,_)=listener.accept().unwrap();stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();let mut bytes=Vec::new();let mut buf=[0;8192];
                loop{let n=stream.read(&mut buf).unwrap();if n==0{break;}bytes.extend_from_slice(&buf[..n]);if let Some(at)=bytes.windows(4).position(|p|p==b"\r\n\r\n"){let header=String::from_utf8_lossy(&bytes[..at]);let length=header.lines().find_map(|l|l.to_lowercase().strip_prefix("content-length:").and_then(|v|v.trim().parse::<usize>().ok())).unwrap_or(0);if bytes.len()>=at+4+length{break;}}}
                let request=String::from_utf8(bytes).unwrap();assert!(request.starts_with(expected));
                if expected.starts_with("POST"){assert!(request.to_lowercase().contains("authorization: bearer fixture-token"));assert!(request.contains("data:image/png;base64,"));assert!(request.contains("gemma-fixture"));}
                let body=if expected.starts_with("GET"){"{}"}else{r#"{"choices":[{"message":{"content":[{"text":"{\"1\":\"NAP: -1,25\","},{"text":"\"2\":\"INVULLEN\",\"3\":\"steen\"}"}]}}]}"#};
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
        });
        let mut runtime=vllm_runtime::Config::from_environment(std::path::PathBuf::new(),BTreeMap::new());runtime.base_url=format!("http://{address}/v1");runtime.model="gemma-fixture".into();runtime.autostart=false;
        let config=Config{runtime,api_key:"fixture-token".into(),timeout:Duration::from_secs(3)};
        let result=Client::default().request(&RgbImage::new(2,2),&config,&AtomicBool::new(false)).unwrap();assert_eq!(result.text_by_key["1"],"NAP: -1,25");assert_eq!(result.text_by_key["2"],"INVULLEN");server.join().unwrap();
    }
    #[test]fn cancelling_a_stalled_gemma_response_returns_before_the_socket(){
        use std::{io::{Read,Write},sync::{Arc,mpsc,atomic::Ordering},time::Instant};
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let address=listener.local_addr().unwrap();
        let(accepted_tx,accepted_rx)=mpsc::channel();let(release_tx,release_rx)=mpsc::channel();
        let server=std::thread::spawn(move||{
            let(mut models,_)=listener.accept().unwrap();let mut data=[0;4096];let _=models.read(&mut data);models.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").unwrap();drop(models);
            let(mut chat,_)=listener.accept().unwrap();let _=chat.read(&mut data);chat.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 1024\r\n\r\n").unwrap();accepted_tx.send(()).unwrap();let _=release_rx.recv_timeout(Duration::from_secs(5));
        });
        let cancel=Arc::new(AtomicBool::new(false));let worker_cancel=cancel.clone();let(tx,rx)=mpsc::channel();
        let mut runtime=vllm_runtime::Config::from_environment(std::path::PathBuf::new(),BTreeMap::new());runtime.base_url=format!("http://{address}/v1");runtime.autostart=false;
        let worker=std::thread::spawn(move||{let config=Config{runtime,api_key:String::new(),timeout:Duration::from_secs(3)};let result=Client::default().request(&RgbImage::new(2,2),&config,&worker_cancel);let _=tx.send(result);});
        accepted_rx.recv_timeout(Duration::from_secs(5)).unwrap();let start=Instant::now();cancel.store(true,Ordering::Release);assert!(rx.recv_timeout(Duration::from_secs(1)).unwrap().is_err());assert!(start.elapsed()<Duration::from_secs(1));release_tx.send(()).unwrap();server.join().unwrap();worker.join().unwrap();
    }
}
