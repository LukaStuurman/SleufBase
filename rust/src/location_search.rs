//! Native PDOK address search and reverse lookup, following location_search.py.
//! One reusable client, bounded responses/retries and cooperative cancellation.
use crate::python_values;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    sync::{atomic::AtomicBool, OnceLock},
    time::{Duration, Instant},
};
mod coordinates;
pub mod worker;
pub use coordinates::{parse_coordinate, parse_rd, wgs84_to_rd};

const SEARCH_URL: &str = "https://api.pdok.nl/bzk/locatieserver/search/v3_1/free";
const REVERSE_URL: &str = "https://api.pdok.nl/bzk/locatieserver/search/v3_1/reverse";
const MAX_RESPONSE: u64 = 2 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Location {
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub location_type: String,
}
impl Location {
    pub fn meters_per_pixel(&self) -> f64 {
        match self.location_type.as_str() {
            "rd" | "wgs84" => 0.2,
            "adres" => 0.5,
            "weg" => 2.,
            "woonplaats" => 20.,
            _ => 5.,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReverseLocation {
    pub label: String,
    pub distance_m: f64,
    pub location_type: String,
}
fn bounded_query(query: &str) -> Result<&str, String> {
    if query.len() > 4096 {
        return Err("Locatiezoektekst is te lang".into());
    }
    Ok(python_values::trim(query))
}
pub fn search_url(query: &str, rows: i32) -> Result<reqwest::Url, String> {
    let query = bounded_query(query)?;
    let mut url = reqwest::Url::parse(SEARCH_URL).map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("rows", &rows.clamp(1, 50).to_string())
        .append_pair("fq", "type:(adres OR weg OR woonplaats)")
        .append_pair("fl", "weergavenaam,centroide_rd,type");
    Ok(url)
}
pub fn reverse_url(x: f64, y: f64, rows: i32) -> Result<reqwest::Url, String> {
    if !x.is_finite() || !y.is_finite() {
        return Err("Ongeldige RD-coördinaten".into());
    }
    let mut url = reqwest::Url::parse(REVERSE_URL).map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("X", &format!("{x:.3}"))
        .append_pair("Y", &format!("{y:.3}"))
        .append_pair("rows", &rows.clamp(1, 50).to_string())
        .append_pair("fl", "weergavenaam,type,afstand");
    Ok(url)
}
fn documents(payload: &Value) -> Result<&[Value], String> {
    if !payload.is_object() {
        return Err("PDOK locatieserver gaf geen JSON-object terug".into());
    }
    let Some(response) = payload.get("response") else {
        return Ok(&[]);
    };
    if !response.is_object() {
        return Err("PDOK locatieserver gaf geen response-object terug".into());
    }
    let Some(docs) = response.get("docs") else {
        return Ok(&[]);
    };
    let docs = docs
        .as_array()
        .ok_or("PDOK locatieserver gaf geen resultatenlijst terug")?;
    if docs.len() > 1000 {
        return Err("PDOK locatieserver gaf te veel resultaten terug".into());
    }
    Ok(docs)
}
fn text(doc: &Value, key: &str, default: &str) -> Result<String, String> {
    doc.get(key).map_or_else(
        || Ok(default.to_owned()),
        |v| python_values::string(Some(v)),
    )
}
pub fn parse_search(payload: &Value, query: &str) -> Result<Vec<Location>, String> {
    let query = bounded_query(query)?;
    static POINT: OnceLock<regex::Regex> = OnceLock::new();
    let pattern =
        POINT.get_or_init(|| regex::Regex::new(r"^POINT\(([-0-9.]+)\s+([-0-9.]+)\)").unwrap());
    let mut output = Vec::new();
    for doc in documents(payload)?.iter().filter(|v| v.is_object()) {
        let value = text(doc, "centroide_rd", "")?;
        let Some(m) = pattern.captures(&value) else {
            continue;
        };
        let (Ok(x), Ok(y)) = (m[1].parse::<f64>(), m[2].parse::<f64>()) else {
            continue;
        };
        // Invalid service records never publish NaN/Inf into the map camera.
        if !x.is_finite() || !y.is_finite() {
            continue;
        }
        output.push(Location {
            label: text(doc, "weergavenaam", query)?,
            x,
            y,
            location_type: text(doc, "type", "")?,
        });
    }
    Ok(output)
}
pub fn parse_reverse(payload: &Value) -> Result<Vec<ReverseLocation>, String> {
    let mut output = Vec::new();
    for doc in documents(payload)?.iter().filter(|v| v.is_object()) {
        let distance_m = python_values::number(doc.get("afstand"))
            .filter(|v| v.is_finite())
            .unwrap_or(0.);
        output.push(ReverseLocation {
            label: text(doc, "weergavenaam", "")?,
            distance_m,
            location_type: text(doc, "type", "")?,
        });
    }
    Ok(output)
}
pub struct Client {
    http: reqwest::blocking::Client,
    retries: u8,
}
impl Client {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            http: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(20))
                .connect_timeout(Duration::from_secs(10))
                .user_agent("SleufBase/1.4")
                .build()
                .map_err(|e| e.to_string())?,
            retries: 3,
        })
    }
    fn request(&self, url: reqwest::Url, cancel: &AtomicBool) -> Result<Value, String> {
        retry(self.retries, cancel, || {
            let mut response = self
                .http
                .get(url.clone())
                .send()
                .and_then(|r| r.error_for_status())
                .map_err(|e| e.to_string())?;
            if response.content_length().is_some_and(|n| n > MAX_RESPONSE) {
                return Err("PDOK locatieantwoord is te groot".into());
            }
            let bytes = crate::input::read_chunks(
                &mut response,
                MAX_RESPONSE,
                "PDOK locatieantwoord",
                cancel,
            )?;
            let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if !value.is_object() {
                return Err("PDOK locatieserver gaf geen JSON-object terug".into());
            }
            Ok(value)
        })
    }
    pub fn search(
        &self,
        query: &str,
        rows: i32,
        cancel: &AtomicBool,
    ) -> Result<Vec<Location>, String> {
        crate::input::check(cancel)?;
        if bounded_query(query)?.is_empty() {
            return Ok(Vec::new());
        }
        parse_search(&self.request(search_url(query, rows)?, cancel)?, query)
    }
    pub fn reverse(
        &self,
        x: f64,
        y: f64,
        rows: i32,
        cancel: &AtomicBool,
    ) -> Result<Vec<ReverseLocation>, String> {
        parse_reverse(&self.request(reverse_url(x, y, rows)?, cancel)?)
    }
}
fn retry<T>(
    attempts: u8,
    cancel: &AtomicBool,
    mut request: impl FnMut() -> Result<T, String>,
) -> Result<T, String> {
    let mut last = "PDOK locatieserver gaf geen antwoord".into();
    for attempt in 0..attempts.clamp(1, 10) {
        crate::input::check(cancel)?;
        match request() {
            Ok(value) => {
                crate::input::check(cancel)?;
                return Ok(value);
            }
            Err(error) => last = error,
        }
        if attempt + 1 < attempts {
            let until = Instant::now() + Duration::from_millis((200u64 << attempt).min(1500));
            while Instant::now() < until {
                crate::input::check(cancel)?;
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    #[test]
    fn transient_failure_retries_and_cancellation_stops_retries() {
        let cancel = AtomicBool::new(false);
        let mut calls = 0;
        assert_eq!(
            retry(2, &cancel, || {
                calls += 1;
                if calls == 1 {
                    Err("temporary".into())
                } else {
                    Ok(7)
                }
            })
            .unwrap(),
            7
        );
        assert_eq!(calls, 2);
        calls = 0;
        assert!(retry::<()>(3, &cancel, || {
            calls += 1;
            cancel.store(true, Ordering::Relaxed);
            Err("cancel".into())
        })
        .is_err());
        assert_eq!(calls, 1);
    }
    #[test]
    fn real_http_status_retry_and_declared_size_limits_use_the_same_bounded_client() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        fn server(replies: Vec<String>) -> (reqwest::Url, std::thread::JoinHandle<()>) {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let url = reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap()))
                .unwrap();
            let thread = std::thread::spawn(move || {
                for reply in replies {
                    let deadline = Instant::now() + Duration::from_secs(5);
                    let mut stream = loop {
                        match listener.accept() {
                            Ok((s, _)) => break s,
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                assert!(Instant::now() < deadline);
                                std::thread::sleep(Duration::from_millis(1));
                            }
                            Err(e) => panic!("{e}"),
                        }
                    };
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    let mut header = Vec::new();
                    while !header.ends_with(b"\r\n\r\n") {
                        let mut byte = [0];
                        assert_eq!(stream.read(&mut byte).unwrap(), 1);
                        header.push(byte[0]);
                        assert!(header.len() < 16384);
                    }
                    stream.write_all(reply.as_bytes()).unwrap();
                }
            });
            (url, thread)
        }
        let (url, thread) = server(vec![
            "HTTP/1.1 503 Temporary\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".into(),
        ]);
        let mut client = Client::new().unwrap();
        client.retries = 2;
        assert_eq!(
            client.request(url, &AtomicBool::new(false)).unwrap(),
            serde_json::json!({})
        );
        thread.join().unwrap();
        let (url, thread) = server(vec![format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            MAX_RESPONSE + 1
        )]);
        client.retries = 1;
        assert!(client
            .request(url, &AtomicBool::new(false))
            .unwrap_err()
            .contains("te groot"));
        thread.join().unwrap();
    }
}
