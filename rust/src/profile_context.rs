//! Original BRT centreline/BGT terrain inputs for automatic profile orientation.
use crate::{input, python_values, Bounds};
use serde_json::Value;
use std::{
    collections::{HashSet, VecDeque},
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
pub type GeometryPaths = Vec<Vec<[f64; 2]>>;
pub type Paths = Arc<GeometryPaths>;
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Roads,
    Terrain,
}
impl Kind {
    fn path(self) -> &'static str {
        match self {
            Self::Roads => "/brt/top10nl/ogc/v1/collections/wegdeel_hartlijn/items",
            Self::Terrain => "/lv/bgt/ogc/v1/collections/onbegroeidterreindeel/items",
        }
    }
}
fn bounds_valid(b: Bounds) -> Result<(), String> {
    if [b.min_x, b.min_y, b.max_x, b.max_y]
        .iter()
        .any(|v| !v.is_finite() || v.abs() > 1e12)
        || b.min_x >= b.max_x
        || b.min_y >= b.max_y
    {
        return Err("Ongeldige RD-contextuitsnede".into());
    }
    Ok(())
}
pub fn padded(b: Bounds) -> Result<Bounds, String> {
    bounds_valid(b)?;
    let padding = ((b.max_x - b.min_x).max(b.max_y - b.min_y) * 0.35).clamp(60., 180.);
    Ok(Bounds {
        min_x: b.min_x - padding,
        min_y: b.min_y - padding,
        max_x: b.max_x + padding,
        max_y: b.max_y + padding,
    })
}
pub fn url(kind: Kind, bounds: Bounds) -> Result<reqwest::Url, String> {
    bounds_valid(bounds)?;
    let mut url = reqwest::Url::parse(&format!("https://api.pdok.nl{}", kind.path()))
        .map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("f", "json")
        .append_pair(
            "bbox",
            &format!(
                "{:.3},{:.3},{:.3},{:.3}",
                bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y
            ),
        )
        .append_pair("bbox-crs", "http://www.opengis.net/def/crs/EPSG/0/28992")
        .append_pair("crs", "http://www.opengis.net/def/crs/EPSG/0/28992")
        .append_pair("limit", "1500");
    Ok(url)
}
pub fn next(kind: Kind, current: &reqwest::Url, href: &str) -> Result<reqwest::Url, String> {
    if href.len() > 8192 {
        return Err("Contextvervolg-URL is te lang".into());
    }
    let url = current.join(href).map_err(|e| e.to_string())?;
    if url.scheme() != "https"
        || url.host_str() != Some("api.pdok.nl")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.path() != kind.path()
    {
        return Err("Contextvervolgpagina valt buiten dezelfde PDOK-collectie".into());
    }
    Ok(url)
}
fn path(value: &Value, count: &mut usize, cancel: &AtomicBool) -> Result<Vec<[f64; 2]>, String> {
    let coords = value.as_array().ok_or("Contextpad is geen lijst")?;
    *count = count.saturating_add(coords.len());
    if *count > 2_000_000 {
        return Err("Profielcontext overschrijdt puntenbudget".into());
    }
    coords
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i % 1024 == 0 {
                input::check(cancel)?;
            }
            let xy = p
                .as_array()
                .filter(|p| p.len() >= 2)
                .ok_or("Ongeldige contextcoördinaat")?;
            let p = [
                python_values::number(xy.first()).ok_or("Ongeldige context X")?,
                python_values::number(xy.get(1)).ok_or("Ongeldige context Y")?,
            ];
            if p.iter().any(|v| !v.is_finite() || v.abs() > 1e12) {
                return Err("Niet-eindige contextcoördinaat".into());
            }
            Ok(p)
        })
        .collect()
}
fn rings(value: &Value) -> Result<Vec<&Value>, String> {
    let mut result = Vec::new();
    for part in value.as_array().ok_or("Contextdelen zijn geen lijst")? {
        let ring = part
            .as_array()
            .ok_or("Contextdeel is geen coördinatenlijst")?;
        if ring.len() >= 2 {
            result.push(part);
        }
        if result.len() > 100_000 {
            return Err("Te veel profielcontextdelen".into());
        }
    }
    Ok(result)
}
pub fn page(
    kind: Kind,
    payload: &Value,
    cancel: &AtomicBool,
) -> Result<(GeometryPaths, Option<String>), String> {
    input::check(cancel)?;
    let payload = payload
        .as_object()
        .ok_or("Contextantwoord is geen JSON-object")?;
    let empty = Vec::new();
    let features = payload.get("features").map_or(Ok(&empty), |v| {
        v.as_array().ok_or("Ongeldige contextobjecten")
    })?;
    let mut result = Vec::new();
    let mut count = 0;
    for (i, feature) in features.iter().enumerate() {
        if i % 256 == 0 {
            input::check(cancel)?;
        }
        let Some(g) = feature.get("geometry").filter(|v| python_values::truth(v)) else {
            continue;
        };
        if !g.is_object() {
            return Err("Contextgeometrie is geen object".into());
        }
        let Some(coords) = g.get("coordinates").filter(|v| !v.is_null()) else {
            continue;
        };
        let rings = match (kind, g.get("type").and_then(Value::as_str)) {
            (Kind::Roads, Some("LineString")) => vec![coords],
            (Kind::Roads, Some("MultiLineString")) | (Kind::Terrain, Some("Polygon")) => {
                rings(coords)?
            }
            (Kind::Terrain, Some("MultiPolygon")) => coords
                .as_array()
                .ok_or("Ongeldige contextpolygonen")?
                .iter()
                .map(rings)
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect(),
            _ => continue,
        };
        for ring in rings {
            if result.len() >= 100_000 {
                return Err("Te veel profielcontextpaden".into());
            }
            result.push(path(ring, &mut count, cancel)?);
        }
    }
    let mut next = None;
    for l in payload
        .get("links")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if l.get("rel")
            .and_then(Value::as_str)
            .is_some_and(|s| s.eq_ignore_ascii_case("next"))
        {
            let href = python_values::string(l.get("href"))?;
            let href = python_values::trim(&href);
            if href.len() > 8192 {
                return Err("Contextvervolg-URL is te lang".into());
            }
            if !href.is_empty() {
                next = Some(href.to_owned());
                break;
            }
        }
    }
    Ok((result, next))
}
pub fn fetch_with(
    kind: Kind,
    bounds: Bounds,
    cancel: &AtomicBool,
    mut fetch: impl FnMut(&reqwest::Url) -> Result<Value, String>,
) -> Result<Paths, String> {
    let mut url = url(kind, bounds)?;
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    let mut points = 0usize;
    for _ in 0..200 {
        input::check(cancel)?;
        if !seen.insert(url.as_str().to_owned()) {
            return Err("Contextserver herhaalt een vervolgpagina".into());
        }
        let response = fetch(&url)?;
        input::check(cancel)?;
        let (paths, following) = page(kind, &response, cancel)?;
        points = points.saturating_add(paths.iter().map(Vec::len).sum::<usize>());
        if points > 2_000_000 || output.len() + paths.len() > 100_000 {
            return Err("Contextbatch overschrijdt het geometriebudget".into());
        }
        output.extend(paths);
        let Some(href) = following else {
            return Ok(Arc::new(output));
        };
        url = next(kind, &url, &href)?;
    }
    Err("Contextserver overschrijdt de paginalimiet".into())
}
struct Cached {
    kind: Kind,
    key: String,
    paths: Paths,
    points: usize,
}
pub struct Client {
    http: reqwest::blocking::Client,
    cache: VecDeque<Cached>,
    points: usize,
}
impl Client {
    pub fn new() -> Result<Self, String> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("SleufBase-Rust/0.1")
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            http,
            cache: VecDeque::new(),
            points: 0,
        })
    }
    pub fn fetch(
        &mut self,
        kind: Kind,
        bounds: Bounds,
        cancel: &AtomicBool,
    ) -> Result<Paths, String> {
        input::check(cancel)?;
        let key = url(kind, bounds)?.query().unwrap_or_default().to_owned();
        if let Some(index) = self
            .cache
            .iter()
            .position(|e| e.kind == kind && e.key == key)
        {
            let entry = self.cache.remove(index).unwrap();
            let paths = entry.paths.clone();
            self.cache.push_back(entry);
            return Ok(paths);
        }
        let paths = fetch_with(kind, bounds, cancel, |url| {
            let mut error = "Contextaanvraag mislukt".to_owned();
            for attempt in 0..4 {
                input::check(cancel)?;
                let result = (|| {
                    let mut response = self
                        .http
                        .get(url.clone())
                        .send()
                        .map_err(|e| e.to_string())?
                        .error_for_status()
                        .map_err(|e| e.to_string())?;
                    if response
                        .content_length()
                        .is_some_and(|n| n > 24 * 1024 * 1024)
                    {
                        return Err("Contextpagina overschrijdt 24 MiB".into());
                    }
                    if response
                        .headers()
                        .get("content-crs")
                        .and_then(|v| v.to_str().ok())
                        .is_some_and(|s| !s.contains("/28992"))
                    {
                        return Err("Contextserver levert geen RD-coördinaten".into());
                    }
                    let bytes = input::read_chunks(
                        &mut response,
                        24 * 1024 * 1024,
                        "Profielcontext",
                        cancel,
                    )?;
                    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
                })();
                match result {
                    Ok(v) => return Ok(v),
                    Err(e) => error = e,
                }
                if attempt < 3 {
                    for _ in 0..(8 << attempt) {
                        input::check(cancel)?;
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
            }
            Err(error)
        })?;
        let count = paths.iter().map(Vec::len).sum();
        while self.cache.len() >= 24 || self.points + count > 2_000_000 {
            if let Some(old) = self.cache.pop_front() {
                self.points -= old.points;
            } else {
                break;
            }
        }
        self.points += count;
        self.cache.push_back(Cached {
            kind,
            key,
            paths: paths.clone(),
            points: count,
        });
        Ok(paths)
    }
}
