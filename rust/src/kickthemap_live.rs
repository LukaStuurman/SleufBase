//! KickTheMap live account + jobs API in Rust. No Python, no external browser
//! cookies, no password storage and no anti-bot bypass. Authenticated requests
//! are restricted to fixed HTTPS endpoints at www.my.kickthemap.com.
//! Recaptcha/WebView2 interactive authentication still requires a native UI port.

use crate::jobs_cache::Job;
use reqwest::blocking::{Client, Response};
use reqwest::{header, redirect, StatusCode};
use regex::Regex;
use serde_json::Value;
use std::collections::HashSet;
use std::io::Read;
use std::thread;
use std::time::Duration;

const ORIGIN: &str = "https://www.my.kickthemap.com";
const MAX_HTML: u64 = 8 * 1024 * 1024;
const MAX_JOBS_JSON: u64 = 24 * 1024 * 1024;

fn response_text(response: Response, max_bytes: u64) -> Result<String, String> {
    if response.content_length().is_some_and(|size| size > max_bytes) {
        return Err("KickTheMap-antwoord is te groot".into());
    }
    let mut bytes = Vec::new();
    response.take(max_bytes + 1).read_to_end(&mut bytes)
        .map_err(|e| format!("KickTheMap-antwoord lezen mislukt: {e}"))?;
    if bytes.len() as u64 > max_bytes {
        return Err("KickTheMap-antwoord is te groot".into());
    }
    String::from_utf8(bytes).map_err(|_| "KickTheMap gaf geen UTF-8 terug".into())
}

fn html_attribute(tag: &str, attribute: &str) -> Option<String> {
    // Either attribute order; avoids interpreting script content as a login token.
    let pattern = format!(r#"(?i)\b{}\s*=\s*(?:"([^"]*)"|'([^']*)')"#,
        regex::escape(attribute));
    let regex = Regex::new(&pattern).ok()?;
    let caps = regex.captures(tag)?;
    caps.get(1).or_else(|| caps.get(2)).map(|m| m.as_str().to_owned())
}
fn html_token(html: &str, element: &str, name: &str) -> Option<String> {
    let tag_pattern = format!(r"(?is)<{}\b[^>]*>", regex::escape(element));
    let tags = Regex::new(&tag_pattern).ok()?;
    for tag in tags.find_iter(html) {
        let s = tag.as_str();
        let token_name = if element.eq_ignore_ascii_case("meta") {
            html_attribute(s, "name")
        } else {
            html_attribute(s, "name")
        };
        if token_name.as_deref().is_some_and(|v| v.eq_ignore_ascii_case(name)) {
            let value_attr = if element.eq_ignore_ascii_case("meta") { "content" } else { "value" };
            if let Some(v) = html_attribute(s, value_attr).filter(|v| !v.is_empty()) {
                return Some(v);
            }
        }
    }
    None
}

fn login_requires_browser(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    lower.contains("recaptcha/api.js")
        || lower.contains("grecaptcha.execute")
        || lower.contains("grecaptcha.render")
}
fn login_page(html: &str) -> bool {
    html.contains("signin-form") || html.contains("name=\"password\"")
}
fn first_string<'a>(obj: &'a Value, names: &[&str]) -> Option<&'a str> {
    for &key in names {
        if let Some(value) = obj.get(key).and_then(Value::as_str).filter(|v| !v.trim().is_empty()) {
            return Some(value.trim());
        }
    }
    None
}
fn value_text(value: Option<&Value>) -> String {
    value.filter(|v|!v.is_null()).and_then(|v|crate::python_values::string(Some(v)).ok()).map(|s|crate::python_values::trim(&s).to_owned()).unwrap_or_default()
}
fn field_text(obj: &Value, names: &[&str]) -> String {
    for name in names {
        let value = value_text(obj.get(*name));
        if !value.is_empty() { return value; }
    }
    String::new()
}
fn parse_positive_id(value: Option<&Value>) -> Option<u64> {
    match value? {
        Value::Number(n) => n.as_u64().filter(|n| *n > 0),
        Value::String(s) => s.trim().parse::<u64>().ok().filter(|n| *n > 0),
        _ => None
    }
}
fn as_i32(value: Option<&Value>) -> i32 {
    match value {
        Some(Value::Number(n)) => n.as_i64().and_then(|v| i32::try_from(v).ok()).unwrap_or(0),
        Some(Value::String(s)) => s.trim().parse::<i32>().unwrap_or(0),
        _ => 0
    }
}
fn as_bool(value: Option<&Value>) -> bool {
    match value {
        Some(Value::String(s)) => crate::python_values::trim(s).eq_ignore_ascii_case("true"),
        Some(v)=>crate::python_values::truth(v),None=>false,
    }
}
fn job_address(obj: &Value) -> String {
    let nested = &obj["address"];
    let part=|key:&str| {
        value_text(obj.get(format!("address_{key}").as_str()).filter(|v|crate::python_values::truth(v)).or_else(||nested.get(key).filter(|v|crate::python_values::truth(v))))
    };
    let rows = [
        [
            part("road"),part("number")
        ].iter().filter(|v| !v.is_empty()).cloned().collect::<Vec<_>>().join(" "),
        [
            part("postcode"),part("town")
        ].iter().filter(|v| !v.is_empty()).cloned().collect::<Vec<_>>().join(" "),
        part("district"),part("region"),part("country")
    ];
    let address = rows.into_iter().filter(|v| !v.is_empty())
        .collect::<Vec<_>>().join(", ");
    if address.is_empty() { "-".into() } else { address }
}
fn coordinates(obj: &Value) -> String {
    for (x, y) in [
        ("x", "y"), ("X", "Y"), ("rd_x", "rd_y"), ("RD_X", "RD_Y"),
        ("longitude", "latitude"), ("lng", "lat"), ("Lon", "Lat")
    ] {
        let first = value_text(obj.get(x));
        let second = value_text(obj.get(y));
        if !first.is_empty() && !second.is_empty() {
            return format!("{first}, {second}");
        }
    }
    let value = field_text(obj, &["coordinates", "coord", "location", "gps", "latlng"]);
    if value.is_empty() { "-".into() } else { value }
}
fn jobs_list(value: &Value) -> Option<&Vec<Value>> {
    if let Some(items) = value.as_array() {return Some(items);}
    if value.get("status") == Some(&Value::Bool(false)) {return None;}
    for key in ["data", "projects", "jobs", "results"] {
        if let Some(list) = value.get(key).and_then(Value::as_array) {return Some(list);}
        if let Some(container) = value.get(key) {
            for sub in ["data", "projects", "jobs", "results", "items"] {
                if let Some(list) = container.get(sub).and_then(Value::as_array) {
                    return Some(list);
                }
            }
        }
    }
    None
}
/// Parse the current API's JSON project list without altering any cached state.
/// Reject an unexpectedly nonempty unparseable list instead of showing no jobs.
pub fn parse_live_jobs(payload: &str) -> Result<Vec<Job>, String> {
    let raw: Value = serde_json::from_str(payload)
        .map_err(|e| format!("Ongeldige KickTheMap JSON: {e}"))?;
    let entries = jobs_list(&raw)
        .ok_or("KickTheMap projectlijst heeft een onbekend formaat")?;
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for obj in entries {
        let Some(id) = parse_positive_id(obj.get("Id").or_else(|| obj.get("id"))) else {
            continue;
        };
        if !seen.insert(id) {continue;}
        let title = field_text(obj, &["AboutProject", "name", "title"]);
        let municipality = field_text(obj, &[
            "commune", "address_town", "address_city", "municipality",
            "gemeente", "address_municipality", "address_district"
        ]);
        let available = if obj.get("can_download_cloud").is_some() {
            as_bool(obj.get("can_download_cloud"))
        } else { as_bool(obj.get("Download")) };
        result.push(Job {
            job_id: id,
            title: if title.is_empty() {format!("Job {id}")} else {title},
            address:job_address(obj),
            municipality:if municipality.is_empty() {"-".into()} else {municipality},
            coordinates:coordinates(obj),
            client_date:field_text(obj,&["ClientDate","created_at"]),
            status:as_i32(obj.get("status")),
            downloadable:available,
            prefix:field_text(obj,&["Prefix","s3_root_dir"])
        });
    }
    if !entries.is_empty() && result.is_empty() {
        return Err("KickTheMap bevat projecten maar geen valide IDs".into());
    }
    Ok(result)
}

/// Parse the original inline-array fallback without executing page scripts.
pub fn parse_inline_jobs(html:&str)->Result<Vec<Job>,String>{
    if html.len() as u64>MAX_HTML{return Err("KickTheMap-pagina is te groot".into());}
    for name in ["projects","projectList","project_list","projectData","project_data","jobs","jobList","job_list","jobData","job_data"]{
        let expression=Regex::new(&format!(r"(?i)(?:^|[^\w$])(?:(?:var|let|const)\s+)?(?:window\s*\.\s*)?{}\s*=",regex::escape(name))).map_err(|e|e.to_string())?;
        for hit in expression.find_iter(html){
            let tail=html[hit.end()..].trim_start();if !tail.starts_with('['){continue;}
            if let Some(Ok(value))=serde_json::Deserializer::from_str(tail).into_iter::<Value>().next(){if value.is_array(){return parse_live_jobs(&value.to_string());}}
        }
    }
    Err("KickTheMap-pagina bevat geen geldige ingebedde projectlijst".into())
}

/// Session cookies are private to this in-memory reqwest client and never
/// written to disk or exported. The caller should drop it on logout.
pub struct KickTheMapLiveClient {
    client: Client,
    csrf: String,
    api_origin:String,
}

impl KickTheMapLiveClient {
    fn request_download_url(&mut self,fields:&[(&str,String)],cancel:&std::sync::atomic::AtomicBool)->Result<Option<reqwest::Url>,String>{
        for mode in ["multipart","form","json"]{
            for auth_attempt in 0..2{
                crate::input::check(cancel)?;
                let mut request=self.client.post(format!("{}/jobs/get-file-url",self.api_origin)).header("X-CSRF-TOKEN",self.csrf.as_str());
                request=match mode{
                    "multipart"=>{let mut form=reqwest::blocking::multipart::Form::new();for (key,value) in fields{form=form.text((*key).to_owned(),value.clone());}request.multipart(form)},
                    "form"=>request.form(fields),
                    _=>request.json(&fields.iter().map(|(key,value)|((*key).to_owned(),value.clone())).collect::<std::collections::BTreeMap<_,_>>()),
                };
                let response=match request.send(){Ok(response)=>response,Err(_)=>break};
                crate::input::check(cancel)?;
                if [401,403,419].contains(&response.status().as_u16()){
                    if auth_attempt>0{return Err("KickTheMap-sessie verlopen; opnieuw inloggen".into());}
                    let home=self.client.get(format!("{}/",self.api_origin)).send().map_err(|e|format!("KickTheMap-sessie vernieuwen: {e}"))?.error_for_status().map_err(|_|"KickTheMap-sessie verlopen; opnieuw inloggen".to_owned())?;
                    let html=response_text(home,MAX_HTML)?;crate::input::check(cancel)?;
                    if login_page(&html){return Err("KickTheMap-sessie verlopen; opnieuw inloggen".into());}
                    self.csrf=html_token(&html,"meta","csrf-token").ok_or("KickTheMap-sessie verlopen; geen vernieuwd CSRF-token")?;
                    continue;
                }
                if response.status().is_success(){if let Ok(text)=response_text(response,512*1024){if let Ok(url)=extract_signed_url(&text){return Ok(Some(url));}}}
                break;
            }
        }
        crate::input::check(cancel)?;Ok(None)
    }
    pub fn login(email: &str, password: &str) -> Result<Self, String> {
        if email.trim().is_empty() || password.is_empty() {
            return Err("E-mailadres en wachtwoord zijn verplicht".into());
        }
        let client = Client::builder()
            .cookie_store(true)
            .redirect(redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .user_agent("SleufBase-Rust/0.1")
            .build().map_err(|e| format!("KickTheMap HTTP-client: {e}"))?;
        let signin = client.get(format!("{ORIGIN}/signin")).send()
            .map_err(|e| format!("KickTheMap aanmelden: {e}"))?
            .error_for_status().map_err(|e| format!("KickTheMap aanmelden: {e}"))?;
        let html = response_text(signin, MAX_HTML)?;
        if login_requires_browser(&html) {
            return Err("KickTheMap vereist interactieve reCAPTCHA-aanmelding; browserlogin moet nog in Rust worden geïntegreerd".into());
        }
        let token = html_token(&html, "input", "_token")
            .ok_or("Loginformulier bevat geen geldig _token")?;
        let post = client.post(format!("{ORIGIN}/login"))
            .form(&[
                ("_token", token.as_str()),
                ("email", email.trim()),
                ("password", password),
                ("g-recaptcha-response", "")
            ]).send().map_err(|e| format!("KickTheMap login: {e}"))?;
        if !post.status().is_success() && !post.status().is_redirection() {
            return Err(format!("KickTheMap aanmelden mislukt (HTTP {})",post.status()));
        }
        let home = client.get(format!("{ORIGIN}/")).send()
            .map_err(|e| format!("KickTheMap sessie ophalen: {e}"))?
            .error_for_status().map_err(|e| format!("KickTheMap sessie: {e}"))?;
        let homepage = response_text(home, MAX_HTML)?;
        if login_page(&homepage) {
            return Err("KickTheMap login mislukt of sessie verlopen".into());
        }
        let csrf = html_token(&homepage,"meta","csrf-token")
            .ok_or("KickTheMap sessie bevat geen CSRF-token")?;
        Ok(Self{client,csrf,api_origin:ORIGIN.into()})
    }

    /// Adopt cookies only from the Edge profile launched by this Rust process.
    /// CSRF is obtained from the signed-in KickTheMap home page, never copied
    /// from an unrelated browser tab or kept on disk.
    pub fn from_browser_cookies(cookies:&[crate::kickthemap_browser::BrowserCookie])
        ->Result<Self,String>
    {
        use reqwest::cookie::Jar;
        use std::sync::Arc;
        let base=reqwest::Url::parse(ORIGIN).map_err(|e|e.to_string())?;
        let jar=Arc::new(Jar::default());
        let mut accepted=0usize;
        for cookie in cookies {
            let domain=cookie.domain.trim_start_matches('.').to_ascii_lowercase();
            if !matches!(domain.as_str(),"www.my.kickthemap.com"|"my.kickthemap.com")
                || !cookie.secure || cookie.name.len()>256
                || cookie.value.len()>8192 || cookie.path.len()>1024
                || !cookie.path.starts_with('/')
                || cookie.value.chars().any(|c|matches!(c,'\r'|'\n'|';'))
            {continue;}
            jar.add_cookie_str(&format!("{}={}; Domain={}; Path={}; Secure",
                cookie.name,cookie.value,domain,cookie.path),&base);
            accepted+=1;
        }
        if accepted==0{return Err("Edge bevat nog geen geldige KickTheMap-sessiecookies".into());}
        let client=Client::builder()
            .cookie_provider(jar)
            .redirect(redirect::Policy::none())
            .timeout(Duration::from_secs(25))
            .user_agent("SleufBase-Rust/0.1 (interactive Edge session)")
            .build().map_err(|e|e.to_string())?;
        let home=client.get(format!("{ORIGIN}/")).send()
            .map_err(|e|format!("KickTheMap browser-sessie valideren: {e}"))?
            .error_for_status().map_err(|e|format!("KickTheMap sessie: {e}"))?;
        let page=response_text(home,MAX_HTML)?;
        if login_page(&page) {
            return Err("De Edge-browser is nog niet bij KickTheMap ingelogd".into());
        }
        let csrf=html_token(&page,"meta","csrf-token")
            .ok_or("KickTheMap-browser heeft geen actieve CSRF-sessie")?;
        Ok(Self{client,csrf,api_origin:ORIGIN.into()})
    }

    pub fn fetch_jobs(&mut self) -> Result<Vec<Job>, String> {
        self.fetch_jobs_with_cancel(&crate::input::NEVER_CANCEL)
    }
    /// The worker can stop retries/backoff immediately; a request already in
    /// flight remains bounded by the established client timeout.
    pub fn fetch_jobs_with_cancel(&mut self,cancel:&std::sync::atomic::AtomicBool)->Result<Vec<Job>,String>{
        let api=self.fetch_jobs_api_with_cancel(cancel);
        match api {
            Ok(jobs)=>Ok(jobs),Err(error)=>{
                crate::input::check(cancel)?;
                if error.contains("sessie verlopen"){return Err(error);}
                let fallback=(||{
                    let home=self.client.get(format!("{}/",self.api_origin)).send().map_err(|e|format!("KickTheMap-pagina ophalen: {e}"))?.error_for_status().map_err(|e|format!("KickTheMap-pagina: {e}"))?;
                    let html=response_text(home,MAX_HTML)?;crate::input::check(cancel)?;
                    if login_page(&html){return Err("KickTheMap-sessie verlopen; opnieuw inloggen".into());}
                    if let Some(csrf)=html_token(&html,"meta","csrf-token"){self.csrf=csrf;}
                    parse_inline_jobs(&html)
                })();
                fallback.map_err(|fallback:String|format!("{error}; {fallback}"))
            }
        }
    }
    fn fetch_jobs_api_with_cancel(&mut self,cancel:&std::sync::atomic::AtomicBool)->Result<Vec<Job>,String>{
        for attempt in 0..3 {
            crate::input::check(cancel)?;
            if attempt>0 {
                for _ in 0..if attempt==1 {25}else{75}{crate::input::check(cancel)?;thread::sleep(Duration::from_millis(20));}
            }
            let response = self.client.post(format!("{}/jobs/get-user-jobs",self.api_origin))
                .header(header::ACCEPT,"application/json")
                .header("X-CSRF-TOKEN",self.csrf.as_str())
                .json(&serde_json::json!({}))
                .send();
            match response {
                Ok(response) => {
                    let status=response.status();
                    if [StatusCode::UNAUTHORIZED,StatusCode::FORBIDDEN,StatusCode::from_u16(419).unwrap()].contains(&status) {
                        if attempt==0 {
                            if let Ok(home)=self.client.get(format!("{}/",self.api_origin)).send() {
                                if home.status().is_success(){if let Ok(html)=response_text(home,MAX_HTML) {
                                    if login_page(&html){return Err("KickTheMap-sessie verlopen; opnieuw inloggen".into());}
                                    if let Some(csrf)=html_token(&html,"meta","csrf-token") {
                                        self.csrf=csrf;
                                        continue;
                                    }
                                }}
                            }
                        }
                        return Err(format!("KickTheMap-sessie verlopen (HTTP {status})"));
                    }
                    if status.is_server_error() || matches!(status.as_u16(),408|425|429) {
                        continue;
                    }
                    if !status.is_success() {
                        return Err(format!("KickTheMap jobs API HTTP {status}"));
                    }
                    let body=response_text(response,MAX_JOBS_JSON)?;
                    crate::input::check(cancel)?;
                    return parse_live_jobs(&body);
                }
                Err(err) if attempt==2 => {
                    return Err(format!("KickTheMap jobs ophalen mislukt: {err}"));
                }
                Err(_) => continue
            }
        }
        Err("KickTheMap jobs API geeft na 3 pogingen geen resultaat".into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn html_tokens_support_attribute_order_and_single_quotes() {
        assert_eq!(html_token(r#"<input value="hello" name="_token">"#,"input","_token"),Some("hello".into()));
        assert_eq!(html_token(r#"<meta content='abc' name='csrf-token'>"#,"meta","csrf-token"),Some("abc".into()));
        assert_eq!(html_token("<script>var _token='no'</script>","input","_token"),None);
        assert!(login_requires_browser("<script src='/recaptcha/api.js'></script>"));
    }
    #[test]
    fn live_json_parsing_and_nested_shape() {
        let payload=r#"{"status":true,"data":{"projects":[
            {"Id":12,"AboutProject":"Sleuf 12","address_road":"Nieuwstraat",
             "address_number":"5","commune":"Made","x":168100,"y":400100,
             "can_download_cloud":true,"status":1},
            {"id":"13","name":"Andere","municipality":"Breda","Download":"true"},
            {"Id":12,"title":"Duplicaat"},{"Id":0,"name":"ongeldig"}
        ]}}"#;
        let jobs=parse_live_jobs(payload).unwrap();
        assert_eq!(jobs.len(),2);
        assert_eq!(jobs[0].address,"Nieuwstraat 5");
        assert_eq!(jobs[0].coordinates,"168100, 400100");
        assert!(jobs[1].downloadable);
        assert_eq!(jobs[0].status,1);
    }
    #[test]
    fn empty_or_malformed_refused() {
        assert_eq!(parse_live_jobs(r#"{"jobs":[]}"#).unwrap().len(),0);
        assert!(parse_live_jobs(r#"{"jobs":[{"unknown":"abc"}]}"#).is_err());
        assert!(parse_live_jobs(r#"{"status":false,"data":[]}"#).is_err());
    }
    #[test]
    fn live_job_fields_match_original_pure_parser(){
        let fixture:Value=serde_json::from_str(include_str!("../tests/fixtures/live-jobs.json")).unwrap();
        for case in fixture["cases"].as_array().unwrap(){let jobs=parse_live_jobs(&case["raw"].to_string()).unwrap();let j=&jobs[0];
            assert_eq!(serde_json::json!({"job_id":j.job_id,"title":j.title,"address":j.address,"municipality":j.municipality,"coordinates":j.coordinates,"client_date":j.client_date,"status":j.status,"prefix":j.prefix,"downloadable":j.downloadable}),case["expected"],"{}",case["raw"]);
        }
        for case in fixture["inline"].as_array().unwrap(){let result=parse_inline_jobs(case["html"].as_str().unwrap());match case["expected"].as_u64(){Some(0)=>assert!(result.unwrap().is_empty()),Some(id)=>assert_eq!(result.unwrap()[0].job_id,id),None=>assert!(result.is_err())};}
    }
    fn scripted(responses:Vec<(u16,&'static str)>)->(KickTheMapLiveClient,std::sync::mpsc::Receiver<String>,std::thread::JoinHandle<()>) {
        use std::net::TcpListener;
        use std::io::{Read,Write};
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();let origin=format!("http://{}",listener.local_addr().unwrap());
        let (tx,rx)=std::sync::mpsc::channel();
        let handle=std::thread::spawn(move||for (status,body) in responses{
            let (mut socket,_)=listener.accept().unwrap();socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut request=Vec::new();let mut buffer=[0u8;4096];loop{let n=socket.read(&mut buffer).unwrap();if n==0{break;}request.extend_from_slice(&buffer[..n]);if let Some(end)=request.windows(4).position(|w|w==b"\r\n\r\n"){
                let headers=String::from_utf8_lossy(&request[..end]).to_lowercase();let length=headers.lines().find_map(|l|l.strip_prefix("content-length:").and_then(|n|n.trim().parse::<usize>().ok())).unwrap_or(0);if request.len()>=end+4+length{break;}
            }}
            let _=tx.send(String::from_utf8(request).unwrap());
            write!(socket,"HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        });
        let client=Client::builder().redirect(redirect::Policy::none()).timeout(Duration::from_secs(3)).build().unwrap();
        (KickTheMapLiveClient{client,csrf:"old-token".into(),api_origin:origin},rx,handle)
    }
    #[test]
    fn download_link_refreshes_csrf_and_retries_same_request_mode(){
        let (mut client,requests,server)=scripted(vec![(419,"expired"),(200,"<meta name='csrf-token' content='download-fresh'>"),(200,r#"{"status":true,"data":{"url":"https://s3.amazonaws.com/test/file"}}"#)]);
        let fields=[("projectId","42".into()),("folder","ortho".into())];
        let url=client.request_download_url(&fields,&std::sync::atomic::AtomicBool::new(false)).unwrap().unwrap();assert_eq!(url.as_str(),"https://s3.amazonaws.com/test/file");server.join().unwrap();
        let requests=requests.try_iter().collect::<Vec<_>>();assert_eq!(requests.len(),3);assert!(requests[0].starts_with("POST /jobs/get-file-url "));assert!(requests[1].starts_with("GET / "));assert!(requests[2].starts_with("POST /jobs/get-file-url "));
        assert!(requests[0].to_lowercase().contains("multipart/form-data"));assert!(requests[2].to_lowercase().contains("multipart/form-data"));assert!(requests[2].to_lowercase().contains("x-csrf-token: download-fresh"));
    }
    #[test]
    fn download_link_keeps_encoding_fallback_after_refresh_refactor(){
        let (mut client,requests,server)=scripted(vec![(415,"no multipart"),(415,"no form"),(200,r#"{"status":true,"data":{"url":"https://s3.amazonaws.com/test/file"}}"#)]);
        let fields=[("projectId","42".into()),("folder","ortho".into())];
        assert!(client.request_download_url(&fields,&std::sync::atomic::AtomicBool::new(false)).unwrap().is_some());server.join().unwrap();
        let requests=requests.try_iter().map(|request|request.to_lowercase()).collect::<Vec<_>>();assert_eq!(requests.len(),3);
        assert!(requests[0].contains("multipart/form-data"));assert!(requests[1].contains("application/x-www-form-urlencoded"));assert!(requests[2].contains("application/json"));assert!(requests[2].contains(r#""projectid":"42""#));
    }
    #[test]
    fn download_link_does_not_retry_other_modes_after_confirmed_logout(){
        let (mut client,requests,server)=scripted(vec![(403,"expired"),(200,"<input name=\"password\">")]);
        assert!(client.request_download_url(&[("projectId","42".into())],&std::sync::atomic::AtomicBool::new(false)).unwrap_err().contains("opnieuw inloggen"));server.join().unwrap();assert_eq!(requests.try_iter().count(),2);
    }
    #[test]
    fn actual_http_refreshes_expired_csrf_then_retries_with_new_token(){
        let (mut client,requests,server)=scripted(vec![(419,"expired"),(200,"<meta name='csrf-token' content='fresh-token'>"),(200,r#"{"data":[{"Id":12,"AboutProject":"PS12","Download":true}]}"#)]);
        let jobs=client.fetch_jobs().unwrap();assert_eq!(jobs[0].job_id,12);server.join().unwrap();
        let requests=requests.try_iter().collect::<Vec<_>>();assert_eq!(requests.len(),3);assert!(requests[0].to_lowercase().contains("x-csrf-token: old-token"));assert!(requests[1].starts_with("GET / "));assert!(requests[2].to_lowercase().contains("x-csrf-token: fresh-token"));
    }
    #[test]
    fn actual_http_rejects_logged_out_session_and_malformed_jobs(){
        let (mut client,_,server)=scripted(vec![(401,"expired"),(200,"<input name=\"password\"><meta name='csrf-token' content='login-token'>")]);
        assert!(client.fetch_jobs().unwrap_err().contains("opnieuw inloggen"));server.join().unwrap();
        let (mut client,_,server)=scripted(vec![(200,"<html>unavailable</html>")]);assert!(client.fetch_jobs().is_err());server.join().unwrap();
    }
    #[test]
    fn actual_http_retries_transient_statuses_before_returning_jobs(){
        let (mut client,requests,server)=scripted(vec![(429,"rate limited"),(503,"temporarily unavailable"),(200,r#"{"data":[{"Id":19}]}"#)]);
        assert_eq!(client.fetch_jobs().unwrap()[0].job_id,19);server.join().unwrap();
        let requests=requests.try_iter().collect::<Vec<_>>();assert_eq!(requests.len(),3);
        assert!(requests.iter().all(|request|request.starts_with("POST /jobs/get-user-jobs ")));
    }
    #[test]
    fn cancellation_interrupts_retry_backoff_before_second_http_request(){
        let (mut client,requests,server)=scripted(vec![(503,"temporarily unavailable")]);
        let cancelled=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let trigger=cancelled.clone();
        let cancel=std::thread::spawn(move||{requests.recv_timeout(Duration::from_secs(3)).unwrap();trigger.store(true,std::sync::atomic::Ordering::Relaxed);});
        assert!(client.fetch_jobs_with_cancel(&cancelled).unwrap_err().contains("geannuleerd"));cancel.join().unwrap();server.join().unwrap();
    }
    #[test]
    fn actual_http_recovers_jobs_from_inline_page_when_api_is_unavailable(){
        let (mut client,requests,server)=scripted(vec![(404,"endpoint missing"),(200,r#"<meta name='csrf-token' content='fresh'><script>window.jobData=[{"Id":42,"AboutProject":"PS42"}];</script>"#)]);
        assert_eq!(client.fetch_jobs().unwrap()[0].job_id,42);server.join().unwrap();assert_eq!(requests.try_iter().count(),2);
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteFileKind {
    GeoTiff,
    JobFeatures,
}
impl RemoteFileKind {
    fn remote_name(self,job:&Job)->Result<String,String> {
        match self {
            Self::GeoTiff if !job.prefix.trim().is_empty() =>
                Ok(format!("{}.tiff", job.prefix.trim())),
            Self::GeoTiff => Err("KickTheMap-job heeft geen bestands-prefix".into()),
            Self::JobFeatures => Ok("jobFeatures.json".into())
        }
    }
    fn extension(self)->&'static str {
        match self {Self::GeoTiff=>"tiff",Self::JobFeatures=>"json"}
    }
    fn folder(self)->&'static str {
        match self {Self::GeoTiff=>"cloud",Self::JobFeatures=>"cloud"}
    }
}
fn job_sidecar_path(path:&std::path::Path)->Result<std::path::PathBuf,String>{
    let ext=path.extension().and_then(|v|v.to_str())
        .ok_or("Downloadbestand mist extensie")?;
    Ok(path.with_extension(format!("{ext}.job.json")))
}

fn safe_file_stem(title:&str,id:u64)->String {
    let mut stem=title.trim().chars().map(|c|
        if c.is_ascii_alphanumeric() || "._-".contains(c) {c} else {'_'}
    ).collect::<String>();
    stem=stem.trim_matches(|c| c=='.'||c=='-'||c=='_').to_owned();
    if stem.is_empty(){format!("kickthemap_job_{id}")}else{stem}
}
/// Signed URL must be HTTPS, public cloud storage or KickTheMap origin,
/// without embedded user credentials. Never send session cookies to this URL.
pub fn validate_signed_url(raw:&str)->Result<reqwest::Url,String> {
    let url=reqwest::Url::parse(raw)
        .map_err(|_|"KickTheMap gaf een ongeldige downloadlink".to_owned())?;
    if url.scheme()!="https" || !url.username().is_empty() || url.password().is_some() {
        return Err("Onveilige KickTheMap-downloadlink".into());
    }
    let host=url.host_str().ok_or("Downloadlink heeft geen host")?
        .to_ascii_lowercase();
    let allowed=[
        "amazonaws.com","cloudfront.net","my.kickthemap.com","storage.googleapis.com",
        "r2.cloudflarestorage.com"
    ];
    if !allowed.iter().any(|d|host==*d || host.ends_with(&format!(".{d}"))) {
        return Err("Downloadlink gebruikt een niet-toegestaan domein".into());
    }
    if url.port_or_known_default()!=Some(443) {
        return Err("Downloadlink vereist HTTPS op poort 443".into());
    }
    Ok(url)
}

fn extract_signed_url(content:&str)->Result<reqwest::Url,String> {
    let payload:Value=serde_json::from_str(content)
        .map_err(|_|"KickTheMap gaf geen downloadlink-JSON terug".to_owned())?;
    if payload.get("status")!=Some(&Value::Bool(true)) {
        return Err("KickTheMap meldt dat het bestand niet beschikbaar is".into());
    }
    let data=payload.get("data");
    let candidate=data.and_then(Value::as_str)
        .or_else(||data.and_then(|x|x.get("url")).and_then(Value::as_str))
        .or_else(||data.and_then(|x|x.get("downloadUrl")).and_then(Value::as_str))
        .or_else(||data.and_then(|x|x.get("download_url")).and_then(Value::as_str))
        .or_else(||payload.get("url").and_then(Value::as_str))
        .ok_or("KickTheMap-downloadantwoord bevat geen URL")?;
    validate_signed_url(candidate)
}

fn verify_download(path:&std::path::Path,kind:RemoteFileKind)->Result<(),String>{
    use std::io::Read;
    let mut file=std::fs::File::open(path).map_err(|e|e.to_string())?;
    match kind {
        RemoteFileKind::GeoTiff=>{
            let mut magic=[0u8;4];
            file.read_exact(&mut magic).map_err(|_|"GeoTIFF-download te klein".to_owned())?;
            if !matches!(magic,[b'I',b'I',42,0] | [b'M',b'M',0,42] |
                [b'I',b'I',43,0] | [b'M',b'M',0,43]) {
                return Err("Download is geen TIFF/GeoTIFF-bestand".into());
            }
        }
        RemoteFileKind::JobFeatures=>{
            let mut bytes=Vec::new();
            file.take(64*1024*1024+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
            if bytes.len()>64*1024*1024 {return Err("Objectbestand is te groot".into());}
            let payload:Value=serde_json::from_slice(&bytes)
                .map_err(|e|format!("Ongeldig jobFeatures.json: {e}"))?;
            if !payload.get("features").is_some_and(Value::is_array) {
                return Err("JobFeatures bevat geen features-lijst".into());
            }
        }
    }
    Ok(())
}
impl KickTheMapLiveClient {
    /// Download only via the established account session and signed HTTPS URL.
    /// Writes atomically with no overwrite of existing Python/Rust source files.
    pub fn download_job(
        &mut self,job:&Job,kind:RemoteFileKind,root:&std::path::Path
    )->Result<std::path::PathBuf,String> {
        self.download_job_with_cancel(job,kind,root,
            &std::sync::atomic::AtomicBool::new(false))
    }
    /// Checked streaming download; cancellation prevents publishing temporary
    /// files and never damages existing downloaded jobs.
    pub fn download_job_with_cancel(
        &mut self,job:&Job,kind:RemoteFileKind,root:&std::path::Path,
        cancel:&std::sync::atomic::AtomicBool
    )->Result<std::path::PathBuf,String> {
        self.download_job_with_browser(job,kind,root,cancel,None)
    }
    pub fn download_job_with_browser(
        &mut self,job:&Job,kind:RemoteFileKind,root:&std::path::Path,
        cancel:&std::sync::atomic::AtomicBool,
        browser:Option<&crate::kickthemap_browser::BrowserChannel>,
    )->Result<std::path::PathBuf,String> {
        use std::{io::{Read,Write},sync::atomic::Ordering};
        let check=||if cancel.load(Ordering::Relaxed) {
            Err("KickTheMap-download geannuleerd".to_owned())
        } else {Ok(())};
        check()?;
        let remote=kind.remote_name(job)?;
        let name=safe_file_stem(&job.title,job.job_id);
        let export_name=match kind {
            RemoteFileKind::GeoTiff=>format!("{name}.tiff"),
            RemoteFileKind::JobFeatures=>format!("{name}_jobFeatures.json")
        };
        let filename=match kind {
            RemoteFileKind::GeoTiff=>
                format!("{name}_{}.tiff",job.job_id),
            RemoteFileKind::JobFeatures=>
                format!("{name}_{}_jobFeatures.json",job.job_id)
        };
        let destination=root.join(filename);
        if destination.exists() {
            // Reusing an identical validated file allows a safe interrupted
            // batch to resume; never trust an existing different job file.
            let sidecar=job_sidecar_path(&destination)?;
            let metadata=std::fs::read(&sidecar)
                .map_err(|_|"Bestand bestaat al zonder vertrouwde job-sidecar".to_owned())?;
            let identity:Value=serde_json::from_slice(&metadata)
                .map_err(|_|"Bestaande download heeft ongeldige jobmetadata".to_owned())?;
            if identity["job_id"].as_u64()!=Some(job.job_id) ||
                identity["source"]!="kickthemap-rust" {
                return Err("Bestand bestaat al voor andere of onbekende job".into());
            }
            verify_download(&destination,kind)?;
            return Ok(destination);
        }
        std::fs::create_dir_all(root).map_err(|e|e.to_string())?;
        let fields=[
            ("projectId",job.job_id.to_string()),
            ("folder",kind.folder().to_string()),
            ("fileName",remote),
            ("exportName",export_name)
        ];
        let signed=self.request_download_url(&fields,cancel)?;
        let signed=match signed {
            Some(url)=>url,
            None=>{
                check()?;
                let browser=browser.ok_or("KickTheMap gaf geen bruikbare downloadlink terug; meld je aan via het privé Edge-venster voor de browserfallback")?;
                browser.signed_download_url(job,kind)?
            }
        };
        let download_client=Client::builder()
            .redirect(redirect::Policy::custom(|attempt|{
                match validate_signed_url(attempt.url().as_str()) {
                    Ok(_) if attempt.previous().len()<4=>attempt.follow(),
                    _=>attempt.stop(),
                }
            }))
            .timeout(Duration::from_secs(180))
            .user_agent("SleufBase-Rust/0.1")
            .build().map_err(|e|e.to_string())?;
        let mut response=download_client.get(signed).send()
            .map_err(|e|format!("KickTheMap-download mislukt: {e}"))?
            .error_for_status().map_err(|e|format!("KickTheMap-download HTTP-fout: {e}"))?;
        let max_bytes=match kind{
            RemoteFileKind::GeoTiff=>2_000_000_000u64,
            RemoteFileKind::JobFeatures=>64*1024*1024u64
        };
        if response.content_length().is_some_and(|len|len>max_bytes) {
            return Err("Downloadbestand is groter dan de toegestane limiet".into());
        }
        let mut temporary=tempfile::NamedTempFile::new_in(root).map_err(|e|e.to_string())?;
        let mut downloaded=0u64;
        let mut buffer=[0u8;128*1024];
        loop {
            check()?;
            let n=response.read(&mut buffer)
                .map_err(|e|format!("Download lezen mislukt: {e}"))?;
            if n==0 {break;}
            downloaded=downloaded.saturating_add(n as u64);
            if downloaded>max_bytes {return Err("Download is te groot".into());}
            temporary.write_all(&buffer[..n])
                .map_err(|e|format!("Download schrijven mislukt: {e}"))?;
        }
        temporary.as_file_mut().flush().map_err(|e|e.to_string())?;
        temporary.as_file_mut().sync_all().map_err(|e|e.to_string())?;
        verify_download(temporary.path(),kind)?;
        // Every downloaded JSON/TIFF file gets a sidecar so the independently
        // running Rust app can identify its original job even when multiple
        // jobs are downloaded in a single directory. Both files are staged,
        // and an incomplete newly created main file is rolled back on error.
        let sidecar_path=job_sidecar_path(&destination)?;
        if sidecar_path.exists() {
            return Err("KickTheMap job metadata bestaat al; niets overschreven".into());
        }
        let mut metadata=tempfile::NamedTempFile::new_in(root)
            .map_err(|e|format!("Job metadata aanmaken mislukt: {e}"))?;
        serde_json::to_writer_pretty(metadata.as_file_mut(),
            &serde_json::json!({
                "version":1,
                "job_id":job.job_id,
                "title":job.title,
                "source":"kickthemap-rust",
            })).map_err(|e|e.to_string())?;
        metadata.as_file_mut().write_all(b"\n").map_err(|e|e.to_string())?;
        metadata.as_file_mut().sync_all().map_err(|e|e.to_string())?;
        check()?;
        temporary.persist_noclobber(&destination)
            .map_err(|e|format!("Downloadbestand niet opgeslagen: {}",e.error))?;
        if let Err(e)=metadata.persist_noclobber(&sidecar_path) {
            let _=std::fs::remove_file(&destination);
            return Err(format!("KickTheMap sidecar kon niet worden opgeslagen: {}",e.error));
        }
        Ok(destination)
    }
}
#[cfg(test)]
mod download_tests {
    use super::*;
    #[test]
    fn sidecar_suffix_matches_geotiff_and_job_features() {
        let tiff=std::path::Path::new("Sleuf_42.tiff");
        let json=std::path::Path::new("Sleuf_42_jobFeatures.json");
        assert_eq!(job_sidecar_path(tiff).unwrap(),
            std::path::PathBuf::from("Sleuf_42.tiff.job.json"));
        assert_eq!(job_sidecar_path(json).unwrap(),
            std::path::PathBuf::from("Sleuf_42_jobFeatures.json.job.json"));
    }
    #[test]
    fn validates_download_domains() {
        assert!(validate_signed_url("https://my-bucket.s3.eu-central-1.amazonaws.com/abc?signature=x").is_ok());
        assert!(validate_signed_url("http://localhost:80/").is_err());
        assert!(validate_signed_url("https://169.254.169.254/").is_err());
        assert!(validate_signed_url("https://amazonaws.com.evil.test/").is_err());
        assert!(validate_signed_url("https://person:pass@s3.amazonaws.com/key").is_err());
    }
    #[test]
    fn signed_response_and_name_sanitize() {
        let response=r#"{"status":true,"data":{"url":"https://s3.amazonaws.com/test/file"}}"#;
        assert!(extract_signed_url(response).is_ok());
        assert!(extract_signed_url(r#"{"status":false,"data":null}"#).is_err());
        assert_eq!(safe_file_stem("../../PS 1",12),"PS_1");
    }
}
