//! Interactive KickTheMap browser sign-in using a *new*, temporary Microsoft
//! Edge profile. The operator handles reCAPTCHA in the real browser. CDP is
//! limited to the locally launched browser, never attached to existing tabs.
//! Credentials and session cookies are never saved to SleufBase project files.
use reqwest::blocking::Client;
use serde_json::{Value,json};
use std::{fs, path::{Path,PathBuf},process::{Child,Command,Stdio}, time::{Duration,Instant}};
use tungstenite::{connect,Message,stream::MaybeTlsStream};

const ORIGIN:&str="https://www.my.kickthemap.com";
const MAX_CDP_RESPONSE:usize=24*1024*1024;
#[derive(Clone,Debug)]
pub struct BrowserCookie {
    pub name:String,
    pub value:String,
    pub domain:String,
    pub path:String,
    pub secure:bool,
}
fn allowed_domain(raw:&str)->bool {
    let domain=raw.trim_start_matches('.').to_ascii_lowercase();
    domain=="my.kickthemap.com" || domain=="www.my.kickthemap.com"
}
fn parse_cookie(value:&Value)->Option<BrowserCookie>{
    let name=value["name"].as_str()?.to_owned();
    let content=value["value"].as_str()?.to_owned();
    let domain=value["domain"].as_str()?.to_owned();
    let path=value["path"].as_str()?.to_owned();
    let secure=value["secure"].as_bool()?;
    if !allowed_domain(&domain) || !secure || name.is_empty() || name.len()>256
        || !name.bytes().all(|c|c.is_ascii_alphanumeric()||b"!#$%&'*+-.^_`|~".contains(&c))
        || content.len()>8192 || content.contains(['\r','\n',';'])
        || !path.starts_with('/') || path.len()>1024 || path.contains(['\r','\n',';'])
    {return None;}
    Some(BrowserCookie{name,value:content,domain,path,secure})
}
pub fn parse_browser_cookies(payload:&Value)->Result<Vec<BrowserCookie>,String>{
    let items=payload.get("cookies").and_then(Value::as_array)
        .ok_or("Edge stuurde geen CDP-cookielijst terug")?;
    if items.len()>4096 {return Err("Browser-cookielijst is te groot".into());}
    let cookies=items.iter().filter_map(parse_cookie).collect::<Vec<_>>();
    if cookies.is_empty(){return Err("Nog geen KickTheMap-aanmelding gevonden in de geopende browser".into());}
    Ok(cookies)
}
pub fn validated_ws_url(raw:&str,port:u16)->Result<String,String>{
    let url=reqwest::Url::parse(raw).map_err(|_|"Ongeldige Edge DevTools-verbinding")?;
    if url.scheme()!="ws" || !url.username().is_empty() || url.password().is_some()
        || url.port_or_known_default()!=Some(port)
        || !matches!(url.host_str(),Some("localhost"|"127.0.0.1"|"[::1]"|"::1"))
        || !url.path().starts_with("/devtools/")
        || url.query().is_some() || url.fragment().is_some()
    {return Err("Edge DevTools-verbinding is niet lokaal of onverwacht".into());}
    Ok(raw.to_owned())
}
fn find_edge()->Result<PathBuf,String>{
    #[cfg(target_os="windows")] {
        let mut locations=Vec::new();
        for name in ["PROGRAMFILES(X86)","PROGRAMFILES","LOCALAPPDATA"] {
            if let Some(dir)=std::env::var_os(name) {
                locations.push(PathBuf::from(dir).join("Microsoft/Edge/Application/msedge.exe"));
            }
        }
        if let Some(path)=locations.into_iter().find(|p|p.is_file()){return Ok(path);}
        Err("Microsoft Edge is niet gevonden; installeer Edge met WebView2-ondersteuning".into())
    }
    #[cfg(not(target_os="windows"))] {
        Err("KickTheMap browseraanmelding wordt alleen op Windows met Microsoft Edge ondersteund".into())
    }
}
fn read_port(profile:&Path)->Result<(u16,String),String>{
    let contents=fs::read_to_string(profile.join("DevToolsActivePort"))
        .map_err(|e|format!("Edge DevTools-port nog niet beschikbaar: {e}"))?;
    let mut lines=contents.lines();
    let port=lines.next().ok_or("Edge DevTools-port ontbreekt")?.trim()
        .parse::<u16>().map_err(|_|"Edge DevTools-port is ongeldig")?;
    if port==0 {return Err("Edge DevTools-port is ongeldig".into());}
    let path=lines.next().ok_or("Edge DevTools-socket ontbreekt")?.trim();
    if !path.starts_with("/devtools/browser/") || path.len()>512
        || path.bytes().any(|c|c.is_ascii_whitespace())
    {return Err("Edge DevTools-socket heeft een ongeldig formaat".into());}
    Ok((port,format!("ws://127.0.0.1:{port}{path}")))
}
/// Owns the only browser from which a KickTheMap session can be adopted.
pub struct Browser {
    process:Child,
    _profile:tempfile::TempDir,
    port:u16,
    websocket:String,
}
impl Browser {
    pub fn launch()->Result<Self,String>{
        let edge=find_edge()?;
        let profile=tempfile::Builder::new().prefix("sleufbase-kickthemap-")
            .tempdir().map_err(|e|format!("Tijdelijk Edge-profiel: {e}"))?;
        let mut command=Command::new(edge);
        command.arg("--remote-debugging-port=0")
            .arg("--remote-debugging-address=127.0.0.1")
            .arg(format!("--user-data-dir={}",profile.path().display()))
            .arg("--no-first-run").arg("--no-default-browser-check")
            .arg("--new-window").arg(format!("{ORIGIN}/signin"))
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        let process=command.spawn().map_err(|e|format!("Edge starten mislukt: {e}"))?;
        let mut owned=Self{process,_profile:profile,port:0,websocket:String::new()};
        let deadline=Instant::now()+Duration::from_secs(25);
        while Instant::now()<deadline {
            if let Ok((port,websocket))=read_port(owned._profile.path()){
                owned.port=port;
                owned.websocket=websocket;
                return Ok(owned);
            }
            if owned.process.try_wait().map_err(|e|e.to_string())?.is_some() {
                return Err("Edge is gestopt voordat het browservenster klaar was".into());
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        Err("Edge-browser start niet met lokale DevTools-verbinding. Controleer bedrijfsbeleid voor remote debugging".into())
    }
    fn command(&self,method:&str,params:Value,browser_target:Option<&str>)->Result<Value,String>{
        let ws=browser_target.unwrap_or(&self.websocket);
        validated_ws_url(ws,self.port)?;
        let (mut client,_)=connect(ws).map_err(|e|format!("Edge CDP-verbindingsfout: {e}"))?;
        if let MaybeTlsStream::Plain(stream)=client.get_mut() {
            stream.set_read_timeout(Some(Duration::from_secs(12))).map_err(|e|e.to_string())?;
            stream.set_write_timeout(Some(Duration::from_secs(12))).map_err(|e|e.to_string())?;
        }
        let request=json!({"id":1,"method":method,"params":params}).to_string();
        client.send(Message::Text(request.into()))
            .map_err(|e|format!("Edge CDP-opdracht mislukt: {e}"))?;
        loop {
            let reply=client.read().map_err(|e|format!("Edge CDP-antwoord ontbreekt: {e}"))?;
            if let Message::Text(text)=reply {
                if text.len()>MAX_CDP_RESPONSE {return Err("Edge antwoord is te groot".into());}
                let body:Value=serde_json::from_str(&text)
                    .map_err(|_|"Edge gaf ongeldige CDP-gegevens terug")?;
                if body["id"]==1 {
                    if body.get("error").is_some() {
                        return Err(format!("Edge CDP {} niet beschikbaar",method));
                    }
                    return body.get("result").cloned()
                        .ok_or_else(||"Edge CDP-resultaat ontbreekt".into());
                }
            }
        }
    }
    fn cdp(&self,method:&str,params:Value)->Result<Value,String>{
        self.command(method,params,None)
    }
    pub fn cookies(&self)->Result<Vec<BrowserCookie>,String>{
        parse_browser_cookies(&self.cdp("Storage.getCookies",json!({}))?)
    }
    pub fn browser_jobs(&self)->Result<Vec<crate::jobs_cache::Job>,String>{
        let client=Client::builder().timeout(Duration::from_secs(8))
            .build().map_err(|e|e.to_string())?;
        let uri=format!("http://127.0.0.1:{}/json/list",self.port);
        let mut response=client.get(uri).send().map_err(|e|e.to_string())?
            .error_for_status().map_err(|e|e.to_string())?;
        let mut data=Vec::new();
        std::io::Read::by_ref(&mut response).take(1024*1024)
            .read_to_end(&mut data).map_err(|e|e.to_string())?;
        if data.len()>=1024*1024 {return Err("Te veel Edge-tabgegevens".into());}
        let targets:Vec<Value>=serde_json::from_slice(&data).map_err(|e|e.to_string())?;
        let current=targets.iter().find(|t|{
            t["type"]=="page" &&
            t["url"].as_str().is_some_and(|raw|{
                reqwest::Url::parse(raw).ok().is_some_and(|url|
                    url.scheme()=="https" && url.host_str()==Some("www.my.kickthemap.com"))
            })
        }).ok_or("Open de KickTheMap-website in het aangemaakte Edge-venster")?;
        let ws=current["webSocketDebuggerUrl"].as_str()
            .ok_or("Edge tab heeft geen DevTools-verbinding")?;
        let script=r#"(async()=>{let csrf=document.querySelector('meta[name="csrf-token"]')?.content||'';let r=await fetch('/jobs/get-user-jobs',{method:'POST',credentials:'same-origin',headers:{'Accept':'application/json','Content-Type':'application/json','X-CSRF-TOKEN':csrf},body:'{}'});if(!r.ok)throw new Error('HTTP '+r.status);return await r.text();})()"#;
        let result=self.command("Runtime.evaluate",
            json!({"expression":script,"awaitPromise":true,"returnByValue":true}),Some(ws))?;
        if result.get("exceptionDetails").is_some() {return Err("KickTheMap job-API kon niet in het browservenster worden opgevraagd".into());}
        let content=result["result"]["value"].as_str()
            .ok_or("KickTheMap-browser heeft geen jobgegevens geretourneerd")?;
        if content.len()>MAX_CDP_RESPONSE{return Err("KickTheMap browser-resultaat te groot".into());}
        crate::kickthemap_live::parse_live_jobs(content)
    }
}
impl Drop for Browser {
    fn drop(&mut self) {
        let _=self.process.kill();
        let _=self.process.wait();
        // A private temporary profile is dropped after the owned browser.
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_private_origin_cookies_are_adopted() {
        let cookie=json!({"cookies":[
            {"name":"laravel_session","value":"ok_abc","domain":".my.kickthemap.com","path":"/","secure":true},
            {"name":"stolen","value":"x","domain":".google.com","path":"/","secure":true},
            {"name":"bad","value":"x\r\nInjected: 1","domain":"my.kickthemap.com","path":"/","secure":true},
            {"name":"not-secure","value":"x","domain":"www.my.kickthemap.com","path":"/","secure":false}
        ]});
        let accepted=parse_browser_cookies(&cookie).unwrap();
        assert_eq!(accepted.len(),1);
        assert_eq!(accepted[0].name,"laravel_session");
        assert!(parse_browser_cookies(&json!({"cookies":[]})).is_err());
    }
    #[test]
    fn cdp_sockets_reject_nonlocal_and_crossport_urls() {
        assert!(validated_ws_url("ws://127.0.0.1:17293/devtools/browser/abc",17293).is_ok());
        assert!(validated_ws_url("ws://localhost:17293/devtools/page/abc",17293).is_ok());
        for candidate in [
            "ws://attacker.com:17293/devtools/page/x",
            "ws://localhost:17294/devtools/page/x",
            "wss://localhost:17293/devtools/page/x",
            "ws://user@localhost:17293/devtools/page/x",
            "ws://localhost:17293/private",
        ] {assert!(validated_ws_url(candidate,17293).is_err(),"{candidate}");}
    }
}
