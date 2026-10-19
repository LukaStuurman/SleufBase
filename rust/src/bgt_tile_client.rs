//! Shared native HTTP pool, bounded decoded LRU and ordered tile batches.
//! Tile parts remain unmerged; source-feature union/repair/clipping is separate.
use crate::{
    bgt_tiles::{self, Tile, TileId, MAX_TILE_BYTES},
    input, Bounds,
};
use reqwest::{blocking::Client as HttpClient, redirect};
use std::{
    collections::HashMap,
    io::Read,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
#[derive(Clone, Copy)]
pub struct Options {
    pub workers: usize,
    pub retries: usize,
    pub timeout_seconds: u64,
    pub cache_entries: usize,
    pub cache_bytes: usize,
    pub result_bytes: usize,
    pub backoff_millis: u64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            workers: 4,
            retries: 4,
            timeout_seconds: 45,
            cache_entries: 128,
            cache_bytes: 64 * 1024 * 1024,
            result_bytes: 256 * 1024 * 1024,
            backoff_millis: 400,
        }
    }
}
impl Options {
    fn validate(self) -> Result<(), String> {
        if !(1..=12).contains(&self.workers)
            || !(1..=4).contains(&self.retries)
            || !(5..=300).contains(&self.timeout_seconds)
            || self.cache_entries > 256
            || self.cache_bytes > 256 * 1024 * 1024
            || self.result_bytes == 0
            || self.result_bytes > 1024 * 1024 * 1024
            || self.backoff_millis > 2000
        {
            return Err("Ongeldige BGT-tegelinstellingen".into());
        }
        Ok(())
    }
}
pub trait Transport: Send + Sync {
    fn load(&self, id: TileId, cancel: &AtomicBool) -> Result<Vec<u8>, String>;
}
pub struct HttpTransport {
    client: HttpClient,
}
impl HttpTransport {
    pub fn new(options: Options) -> Result<Self, String> {
        options.validate()?;
        let client = HttpClient::builder()
            .timeout(Duration::from_secs(options.timeout_seconds))
            .connect_timeout(Duration::from_secs(10))
            .redirect(redirect::Policy::none())
            .user_agent("SleufBase-Rust/0.1")
            .build()
            .map_err(|e| format!("BGT-client starten: {e}"))?;
        Ok(Self { client })
    }
}
/// Both compressed input and inflated output are bounded; cancellation is
/// checked by the shared 64 KiB reader. Never buffer an unbounded HTTP body.
pub fn read_payload(
    reader: impl Read,
    encoding: &str,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let encoding = encoding.trim();
    let mut bounded = reader.take(MAX_TILE_BYTES as u64 + 1);
    let data = if encoding.eq_ignore_ascii_case("gzip") || encoding.eq_ignore_ascii_case("x-gzip") {
        input::read_chunks(
            &mut flate2::read::MultiGzDecoder::new(&mut bounded),
            MAX_TILE_BYTES as u64,
            "BGT-gziptegel",
            cancel,
        )?
    } else if encoding.is_empty() || encoding.eq_ignore_ascii_case("identity") {
        input::read_chunks(&mut bounded, MAX_TILE_BYTES as u64, "BGT-tegel", cancel)?
    } else {
        return Err("Onbekende BGT-contentcodering".into());
    };
    if bounded.limit() == 0 {
        return Err("Gecomprimeerde BGT-tegel te groot".into());
    }
    Ok(data)
}
impl Transport for HttpTransport {
    fn load(&self, id: TileId, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
        input::check(cancel)?;
        let response = self
            .client
            .get(id.url()?)
            .header("Accept-Encoding", "gzip")
            .send()
            .map_err(|e| format!("PDOK BGT ophalen: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("PDOK BGT HTTP {}", response.status().as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_TILE_BYTES as u64)
        {
            return Err("BGT-respons te groot".into());
        }
        let encoding = response
            .headers()
            .get("Content-Encoding")
            .map(|v| v.to_str().map_err(|_| "Ongeldige BGT-contentcodering"))
            .transpose()?
            .unwrap_or("")
            .to_owned();
        input::check(cancel)?;
        read_payload(response, &encoding, cancel)
    }
}
struct Entry {
    tile: Arc<Tile>,
    stamp: u64,
    bytes: usize,
}
#[derive(Default)]
struct Cache {
    entries: HashMap<TileId, Entry>,
    clock: u64,
    bytes: usize,
}
impl Cache {
    fn tick(&mut self) -> u64 {
        if self.clock == u64::MAX {
            let mut order = self
                .entries
                .iter()
                .map(|(&k, v)| (k, v.stamp))
                .collect::<Vec<_>>();
            order.sort_by_key(|(_, s)| *s);
            for (i, (id, _)) in order.iter().enumerate() {
                self.entries.get_mut(id).unwrap().stamp = i as u64;
            }
            self.clock = order.len() as u64;
        }
        self.clock += 1;
        self.clock
    }
    fn get(&mut self, id: TileId) -> Option<Arc<Tile>> {
        let stamp = self.tick();
        let entry = self.entries.get_mut(&id)?;
        entry.stamp = stamp;
        Some(entry.tile.clone())
    }
    fn insert(&mut self, id: TileId, tile: Arc<Tile>, options: Options) {
        let bytes = tile.memory_bytes + std::mem::size_of::<Tile>() + 64;
        if options.cache_entries == 0 || bytes > options.cache_bytes {
            return;
        }
        if let Some(old) = self.entries.remove(&id) {
            self.bytes -= old.bytes;
        }
        let stamp = self.tick();
        self.bytes += bytes;
        self.entries.insert(id, Entry { tile, stamp, bytes });
        while self.entries.len() > options.cache_entries || self.bytes > options.cache_bytes {
            let id = *self.entries.iter().min_by_key(|(_, e)| e.stamp).unwrap().0;
            self.bytes -= self.entries.remove(&id).unwrap().bytes;
        }
    }
}
pub struct Client<T = HttpTransport> {
    transport: T,
    options: Options,
    cache: Mutex<Cache>,
}
impl Client<HttpTransport> {
    pub fn new(options: Options) -> Result<Self, String> {
        Self::with_transport(HttpTransport::new(options)?, options)
    }
}
impl<T: Transport> Client<T> {
    pub fn with_transport(transport: T, options: Options) -> Result<Self, String> {
        options.validate()?;
        Ok(Self {
            transport,
            options,
            cache: Mutex::new(Cache::default()),
        })
    }
    pub fn tile(&self, id: TileId, cancel: &AtomicBool) -> Result<Arc<Tile>, String> {
        id.validate()?;
        input::check(cancel)?;
        if let Some(tile) = self
            .cache
            .lock()
            .map_err(|_| "BGT-cache vergrendeld")?
            .get(id)
        {
            return Ok(tile);
        }
        let mut last = String::new();
        for attempt in 0..self.options.retries {
            input::check(cancel)?;
            match self
                .transport
                .load(id, cancel)
                .and_then(|bytes| bgt_tiles::decode(&bytes, id, cancel))
            {
                Ok(tile) => {
                    input::check(cancel)?;
                    let tile = Arc::new(tile);
                    self.cache
                        .lock()
                        .map_err(|_| "BGT-cache vergrendeld")?
                        .insert(id, tile.clone(), self.options);
                    return Ok(tile);
                }
                Err(error) => last = error,
            }
            input::check(cancel)?;
            if attempt + 1 < self.options.retries {
                let deadline = Instant::now()
                    + Duration::from_millis(
                        (self.options.backoff_millis * (1 << attempt)).min(2000),
                    );
                while Instant::now() < deadline {
                    input::check(cancel)?;
                    std::thread::sleep(
                        deadline
                            .saturating_duration_since(Instant::now())
                            .min(Duration::from_millis(25)),
                    );
                }
            }
        }
        Err(format!("BGT-tegel 12/{}/{}: {last}", id.row, id.col))
    }
    /// Preserve tile enumeration order, even when HTTP requests finish in a
    /// different order. Any error/cancellation/budget failure returns no batch.
    pub fn fetch(
        &self,
        bounds: Bounds,
        cancel: &AtomicBool,
    ) -> Result<Vec<(TileId, Arc<Tile>)>, String> {
        input::check(cancel)?;
        let ids = bgt_tiles::tiles_for_bounds(bounds)?;
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let next = AtomicUsize::new(0);
        let bytes = AtomicUsize::new(0);
        let stopped = AtomicBool::new(false);
        let mut output = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..self.options.workers.min(ids.len()) {
                handles.push(
                    scope.spawn(|| -> Result<Vec<(usize, TileId, Arc<Tile>)>, String> {
                        let mut results = Vec::new();
                        while !stopped.load(Ordering::Relaxed) {
                            let i = next.fetch_add(1, Ordering::Relaxed);
                            let Some(&id) = ids.get(i) else {
                                break;
                            };
                            let loaded = catch_unwind(AssertUnwindSafe(|| self.tile(id, cancel)))
                                .map_err(|_| "BGT-worker gestopt".to_owned())
                                .and_then(|v| v);
                            match loaded {
                                Ok(tile) => {
                                    let weight = tile.memory_bytes + std::mem::size_of::<Tile>();
                                    if bytes.fetch_add(weight, Ordering::Relaxed) + weight
                                        > self.options.result_bytes
                                    {
                                        stopped.store(true, Ordering::Relaxed);
                                        return Err(
                                            "BGT-resultaat overschrijdt geheugenbudget".into()
                                        );
                                    }
                                    results.push((i, id, tile));
                                }
                                Err(error) => {
                                    stopped.store(true, Ordering::Relaxed);
                                    return Err(error);
                                }
                            }
                        }
                        Ok(results)
                    }),
                );
            }
            let mut all = Vec::new();
            let mut error = None;
            for handle in handles {
                match handle.join() {
                    Ok(Ok(results)) => all.extend(results),
                    Ok(Err(e)) => {
                        error.get_or_insert(e);
                    }
                    Err(_) => {
                        error.get_or_insert("BGT-worker gestopt".into());
                    }
                }
            }
            if let Some(error) = error {
                Err(error)
            } else {
                Ok(all)
            }
        })?;
        input::check(cancel)?;
        if output.len() != ids.len() {
            return Err("Onvolledige BGT-tegelbatch".into());
        }
        output.sort_by_key(|(i, _, _)| *i);
        Ok(output.into_iter().map(|(_, id, tile)| (id, tile)).collect())
    }
}
