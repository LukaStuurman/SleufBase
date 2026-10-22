//! Actual native coordinate/client audit. Omitted payload requests live PDOK;
//! supplied service JSON supports deterministic independent/offline verification.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sleufbase_rust::location_search::{self as location, Client};
use sleufbase_rust::{location_search::ReverseLocation, raster_addresses};
use std::sync::atomic::{AtomicBool, AtomicUsize};
#[path = "support/atomic_json.rs"]
mod atomic_json;
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Coordinate {
        query: String,
    },
    Search {
        query: String,
        rows: i32,
        payload: Option<Value>,
    },
    Reverse {
        x: f64,
        y: f64,
        rows: i32,
        payload: Option<Value>,
    },
    RasterAddresses {
        sources: Vec<raster_addresses::Source>,
        replies: Option<Vec<Reply>>,
    },
}
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Reply {
    Rows(Vec<ReverseLocation>),
    Error(String),
}
fn serialize(value: impl Serialize) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}
fn main() -> Result<(), String> {
    let mut client = None;
    let cancel = AtomicBool::new(false);
    atomic_json::run(2048, |r: Request, out| {
        let result = (|| match r {
            Request::Coordinate { query } => {
                let coordinate = location::parse_coordinate(&query);
                Ok(
                    json!({"rd":location::parse_rd(&query),"scale":coordinate.as_ref().map(|v|v.meters_per_pixel()),"coordinate":coordinate}),
                )
            }
            Request::Search {
                query,
                rows,
                payload,
            } => {
                let results = if let Some(value) = payload {
                    location::parse_search(&value, &query)?
                } else {
                    if client.is_none() {
                        client = Some(Client::new()?);
                    }
                    client.as_ref().unwrap().search(&query, rows, &cancel)?
                };
                serialize(results)
            }
            Request::Reverse {
                x,
                y,
                rows,
                payload,
            } => {
                let results = if let Some(value) = payload {
                    location::parse_reverse(&value)?
                } else {
                    if client.is_none() {
                        client = Some(Client::new()?);
                    }
                    client.as_ref().unwrap().reverse(x, y, rows, &cancel)?
                };
                serialize(results)
            }
            Request::RasterAddresses { sources, replies } => {
                let mut replies = replies.map(|r| r.into_iter());
                let results = raster_addresses::lookup(
                    sources,
                    &cancel,
                    &AtomicUsize::new(0),
                    |x, y, cancel| {
                        if let Some(replies) = &mut replies {
                            match replies.next().ok_or("Missing address audit reply")? {
                                Reply::Rows(rows) => Ok(rows),
                                Reply::Error(error) => Err(error),
                            }
                        } else {
                            if client.is_none() {
                                client = Some(Client::new()?);
                            }
                            client.as_ref().unwrap().reverse(x, y, 1, cancel)
                        }
                    },
                )?;
                serialize(results)
            }
        })();
        let value = result.unwrap_or_else(|e| json!({"error":e}));
        serde_json::to_writer(out, &value).map_err(|e| e.to_string())
    })
}
