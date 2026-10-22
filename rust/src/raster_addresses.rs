//! Original TIFF-address rows/copy text, evaluated without raster pixel copies.
use crate::{
    geotiff::GeoTiffInfo,
    location_search::{Client, ReverseLocation},
};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
const MAX_ROWS: usize = 5000;
const MAX_TEXT: usize = 8 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub name: String,
    pub center: [f64; 2],
    pub epsg: Option<u32>,
}
impl Source {
    pub fn from_raster(raster: &GeoTiffInfo) -> Self {
        let b = raster.bounds;
        Self {
            name: raster
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            center: [b.min_x * 0.5 + b.max_x * 0.5, b.min_y * 0.5 + b.max_y * 0.5],
            epsg: raster.epsg,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub name: String,
    pub address: String,
    pub distance: String,
    pub copy_text: String,
}
impl Record {
    pub fn from_result(name: String, result: Result<Vec<ReverseLocation>, String>) -> Self {
        let (address, distance, copy_text) = match result {
            Err(error) => (
                format!("Adres ophalen mislukt: {error}"),
                "-".into(),
                format!("{name}: adres ophalen mislukt"),
            ),
            Ok(rows) => match rows.into_iter().next() {
                None => (
                    "Geen adres gevonden".into(),
                    "-".into(),
                    format!("{name}: geen adres gevonden"),
                ),
                Some(best) => {
                    let distance = format!("{:.1} m", best.distance_m);
                    let copy_text = format!("{name}: {}", best.label);
                    (best.label, distance, copy_text)
                }
            },
        };
        Self {
            name,
            address,
            distance,
            copy_text,
        }
    }
    pub fn copied(&self, include_name: bool) -> &str {
        if include_name {
            &self.copy_text
        } else {
            &self.address
        }
    }
    fn bytes(&self) -> usize {
        self.name.len() + self.address.len() + self.distance.len() + self.copy_text.len()
    }
}
pub fn lookup(
    sources: Vec<Source>,
    cancel: &AtomicBool,
    progress: &AtomicUsize,
    mut reverse: impl FnMut(f64, f64, &AtomicBool) -> Result<Vec<ReverseLocation>, String>,
) -> Result<Vec<Record>, String> {
    validate(&sources)?;
    crate::input::check(cancel)?;
    let mut output = Vec::with_capacity(sources.len());
    let mut bytes = 0usize;
    for (i, source) in sources.into_iter().enumerate() {
        crate::input::check(cancel)?;
        let result = if source.center.iter().any(|v| !v.is_finite()) {
            Err("Ongeldige GeoTIFF-coördinaten".into())
        } else if source.epsg.is_some_and(|epsg| epsg != 28992) {
            Err("GeoTIFF gebruikt geen RD (EPSG:28992)".into())
        } else {
            reverse(source.center[0], source.center[1], cancel)
        };
        crate::input::check(cancel)?;
        let record = Record::from_result(source.name, result);
        bytes = bytes.saturating_add(record.bytes());
        if bytes > MAX_TEXT {
            return Err("TIFF-adressenoverzicht overschrijdt tekstbudget".into());
        }
        output.push(record);
        progress.store(i + 1, Ordering::Relaxed);
    }
    Ok(output)
}
fn validate(sources: &[Source]) -> Result<(), String> {
    if sources.len() > MAX_ROWS
        || sources.iter().any(|s| s.name.len() > 32768)
        || sources.iter().map(|s| s.name.len()).sum::<usize>() > MAX_TEXT
    {
        return Err("Te veel of te grote TIFF-adresbronnen".into());
    }
    Ok(())
}
pub type Ready = crate::latest_job::Ready<Vec<Record>>;
pub struct Ticket {
    pub id: u64,
    pub progress: Arc<AtomicUsize>,
}
pub struct Worker {
    inner: crate::latest_job::Worker<Vec<Source>, Vec<Record>>,
}
impl Worker {
    pub fn new() -> Result<Self, String> {
        let client = Client::new()?;
        Self::with_lookup(move |x, y, cancel| client.reverse(x, y, 1, cancel))
    }
    pub fn with_lookup(
        mut reverse: impl FnMut(f64, f64, &AtomicBool) -> Result<Vec<ReverseLocation>, String>
            + Send
            + 'static,
    ) -> Result<Self, String> {
        Ok(Self {
            inner: crate::latest_job::Worker::new(
                "SleufBase-TIFF-addresses",
                move |sources, cancel, progress| lookup(sources, cancel, progress, &mut reverse),
            )?,
        })
    }
    pub fn start(&self, sources: Vec<Source>) -> Result<Ticket, String> {
        validate(&sources)?;
        let ticket = self.inner.submit(sources)?;
        Ok(Ticket {
            id: ticket.id,
            progress: ticket.progress,
        })
    }
    pub fn cancel(&self) {
        self.inner.cancel();
    }
    pub fn poll(&self) -> Option<Ready> {
        self.inner.poll()
    }
}
