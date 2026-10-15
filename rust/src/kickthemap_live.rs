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
    match value {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new()
    }
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
        Some(Value::Bool(v)) => *v,
        Some(Value::Number(n)) => n.as_i64().is_some_and(|v| v != 0),
        Some(Value::String(s)) => s.trim().eq_ignore_ascii_case("true") || s.trim() == "1",
        _ => false
    }
}
fn job_address(obj: &Value) -> String {
    let nested = &obj["address"];
    let rows = [
        [
            field_text(obj, &["address_road"]).trim().to_owned(),
            field_text(obj, &["address_number"]).trim().to_owned()
        ].iter().filter(|v| !v.is_empty()).cloned().collect::<Vec<_>>().join(" "),
        [
            field_text(obj, &["address_postcode"]),
            field_text(obj, &["address_town"])
        ].iter().filter(|v| !v.is_empty()).cloned().collect::<Vec<_>>().join(" "),
        field_text(obj, &["address_district"]),
        field_text(obj, &["address_region"]),
        field_text(obj, &["address_country"])
    ];
    let mut address = rows.into_iter().filter(|v| !v.is_empty())
        .collect::<Vec<_>>().join(", ");
    if address.is_empty() && nested.is_object() {
        address = ["road", "number", "postcode", "town"].into_iter()
            .map(|k| value_text(nested.get(k)))
            .filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
    }
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

/// Session cookies are private to this in-memory reqwest client and never
/// written to disk or exported. The caller should drop it on logout.
pub struct KickTheMapLiveClient {
    client: Client,
    csrf: String
}

impl KickTheMapLiveClient {
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
        Ok(Self{client,csrf})
    }

    pub fn fetch_jobs(&mut self) -> Result<Vec<Job>, String> {
        for attempt in 0..3 {
            if attempt>0 {
                thread::sleep(Duration::from_millis(if attempt==1 {500} else {1500}));
            }
            let response = self.client.post(format!("{ORIGIN}/jobs/get-user-jobs"))
                .header(header::ACCEPT,"application/json")
                .header("X-CSRF-TOKEN",self.csrf.as_str())
                .json(&serde_json::json!({}))
                .send();
            match response {
                Ok(response) => {
                    let status=response.status();
                    if [StatusCode::UNAUTHORIZED,StatusCode::FORBIDDEN,StatusCode::from_u16(419).unwrap()].contains(&status) {
                        if attempt==0 {
                            if let Ok(home)=self.client.get(format!("{ORIGIN}/")).send() {
                                if let Ok(html)=response_text(home,MAX_HTML) {
                                    if let Some(csrf)=html_token(&html,"meta","csrf-token") {
                                        self.csrf=csrf;
                                        continue;
                                    }
                                }
                            }
                        }
                        return Err(format!("KickTheMap-sessie verlopen (HTTP {status})"));
                    }
                    if status.is_server_error() || status==StatusCode::TOO_MANY_REQUESTS {
                        continue;
                    }
                    if !status.is_success() {
                        return Err(format!("KickTheMap jobs API HTTP {status}"));
                    }
                    let body=response_text(response,MAX_JOBS_JSON)?;
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
}
