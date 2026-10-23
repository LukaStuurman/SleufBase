//! Original Kadaster WMTS zoom, Mercator crop and Pillow bilinear composition.
//! Only requested tiles are fetched. Tile rows go straight into the crop, rather
//! than keeping both a complete stitched mosaic and its second cropped copy.
use crate::{input, Bounds};
use image::{ImageDecoder, ImageFormat, Rgba, RgbaImage};
use proj4rs::proj::Proj;
use std::{
    collections::VecDeque,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc, Mutex, OnceLock},
    time::{Duration, SystemTime},
};

pub const BASE_URL: &str = "https://service.pdok.nl/kadaster/brk-kadastrale-kaart/wmts/v5_0";
const HALF: f64 = 20037508.342789244;
const RESOLUTION: f64 = (2. * HALF) / 256.;
const MAX_BYTES: u64 = 2 * 1024 * 1024;
const TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const MEMORY_TILES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct TileId {
    pub zoom: u8,
    pub x: u32,
    pub y: u32,
}
impl TileId {
    pub fn url(self) -> String {
        format!(
            "{BASE_URL}/Kadastralekaart/EPSG:3857/{:02}/{}/{}.png",
            self.zoom, self.x, self.y
        )
    }
    fn path(self, cache: &Path) -> PathBuf {
        cache.join(format!("{}_{}_{}.png", self.zoom, self.x, self.y))
    }
}

#[derive(Debug, serde::Serialize)]
pub struct Plan {
    mercator_bounds: Bounds,
    zoom: u8,
    resolution: f64,
    tile_span: f64,
    min_tile_x: u32,
    max_tile_x: u32,
    min_tile_y: u32,
    max_tile_y: u32,
    stitched_size: [u32; 2],
    target_size: [u32; 2],
    crop: [u32; 4],
}
fn mercator(x: f64, y: f64) -> Result<[f64; 2], String> {
    static PROJECTIONS: OnceLock<Result<(Proj, Proj), String>> = OnceLock::new();
    let (rd, wgs) = PROJECTIONS
        .get_or_init(|| {
            Ok((
                Proj::from_proj_string(crate::rd_projection::RD).map_err(|e| e.to_string())?,
                Proj::from_proj_string("+proj=longlat +ellps=WGS84 +datum=WGS84 +no_defs")
                    .map_err(|e| e.to_string())?,
            ))
        })
        .as_ref()
        .map_err(Clone::clone)?;
    let mut p = (x, y, 0.);
    proj4rs::transform::transform(rd, wgs, &mut p).map_err(|e| e.to_string())?;
    let result = [
        6378137. * p.0,
        6378137. * (std::f64::consts::FRAC_PI_4 + p.1 * 0.5).tan().ln(),
    ];
    if result.iter().any(|v| !v.is_finite()) {
        return Err("Kadasterprojectie is niet eindig".into());
    }
    Ok(result.map(|v| v.clamp(-HALF, HALF)))
}
impl Plan {
    pub fn new(bounds: Bounds, size: [u32; 2]) -> Result<Self, String> {
        if size.contains(&0)
            || size.iter().any(|&n| n > 10_000)
            || u64::from(size[0]) * u64::from(size[1]) > 50_000_000
        {
            return Err("Kadasterkaart overschrijdt het pixelbudget".into());
        }
        if [bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y]
            .iter()
            .any(|v| !v.is_finite())
            || bounds.min_x >= bounds.max_x
            || bounds.min_y >= bounds.max_y
            || bounds.min_x < -50_000.
            || bounds.max_x > 350_000.
            || bounds.min_y < 250_000.
            || bounds.max_y > 650_000.
        {
            return Err("Kadaster vereist geldige Nederlandse RD-grenzen".into());
        }
        let points = [
            (bounds.min_x, bounds.min_y),
            (bounds.min_x, bounds.max_y),
            (bounds.max_x, bounds.min_y),
            (bounds.max_x, bounds.max_y),
        ]
        .map(|(x, y)| mercator(x, y))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
        let b = Bounds {
            min_x: points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min),
            min_y: points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min),
            max_x: points
                .iter()
                .map(|p| p[0])
                .fold(f64::NEG_INFINITY, f64::max),
            max_y: points
                .iter()
                .map(|p| p[1])
                .fold(f64::NEG_INFINITY, f64::max),
        };
        let mpp = ((b.max_x - b.min_x) / f64::from(size[0]))
            .max((b.max_y - b.min_y) / f64::from(size[1]))
            .max(0.01);
        let zoom = (RESOLUTION / mpp).log2().round_ties_even().clamp(0., 22.) as u8;
        let resolution = RESOLUTION / f64::from(1u32 << zoom);
        let tile_span = resolution * 256.;
        let side = f64::from((1u32 << zoom) - 1);
        let index = |v: f64| v.floor().clamp(0., side) as u32;
        let min_tile_x = index((b.min_x + HALF) / tile_span);
        let max_tile_x = index((b.max_x + HALF - 1e-9) / tile_span);
        let min_tile_y = index((HALF - b.max_y) / tile_span);
        let max_tile_y = index((HALF - b.min_y - 1e-9) / tile_span);
        let stitched_size = [
            (max_tile_x - min_tile_x + 1) * 256,
            (max_tile_y - min_tile_y + 1) * 256,
        ];
        let origin_x = f64::from(min_tile_x) * tile_span - HALF;
        let origin_y = HALF - f64::from(min_tile_y) * tile_span;
        let crop = [
            (b.min_x - origin_x) / resolution,
            (origin_y - b.max_y) / resolution,
            (b.max_x - origin_x) / resolution,
            (origin_y - b.min_y) / resolution,
        ]
        .map(|v| v.round_ties_even() as u32);
        if crop[2] <= crop[0]
            || crop[3] <= crop[1]
            || crop[2] > stitched_size[0]
            || crop[3] > stitched_size[1]
            || u64::from(crop[2] - crop[0]) * u64::from(crop[3] - crop[1]) > 100_000_000
        {
            return Err("Kadasteruitsnede is leeg of overschrijdt het pixelbudget".into());
        }
        let plan = Self {
            mercator_bounds: b,
            zoom,
            resolution,
            tile_span,
            min_tile_x,
            max_tile_x,
            min_tile_y,
            max_tile_y,
            stitched_size,
            target_size: size,
            crop,
        };
        if plan.tiles().len() > 2048 {
            return Err("Kadasterkaart vraagt te veel tegels".into());
        }
        Ok(plan)
    }
    pub fn tiles(&self) -> Vec<TileId> {
        (self.min_tile_y..=self.max_tile_y)
            .flat_map(|y| {
                (self.min_tile_x..=self.max_tile_x).map(move |x| TileId {
                    zoom: self.zoom,
                    x,
                    y,
                })
            })
            .collect()
    }
    /// Bounded batches let the production client fetch four tiles concurrently;
    /// independent image oracles can supply synthetic tiles through the same path.
    pub fn render_with(
        &self,
        cancel: &AtomicBool,
        mut fetch: impl FnMut(&[TileId]) -> Result<Vec<Arc<RgbaImage>>, String>,
    ) -> Result<RgbaImage, String> {
        input::check(cancel)?;
        let [left, top, right, bottom] = self.crop;
        let mut crop = crate::raster_rotation::allocate(right - left, bottom - top)?;
        for p in crop.pixels_mut() {
            *p = Rgba([243, 243, 243, 255]);
        }
        for ids in self.tiles().chunks(4) {
            input::check(cancel)?;
            let images = fetch(ids)?;
            if images.len() != ids.len() {
                return Err("Kadastertegelbatch is onvolledig".into());
            }
            for (id, tile) in ids.iter().zip(images) {
                input::check(cancel)?;
                if tile.dimensions() != (256, 256) {
                    return Err("Kadastertegel heeft geen 256x256 pixels".into());
                }
                let ox = (id.x - self.min_tile_x) * 256;
                let oy = (id.y - self.min_tile_y) * 256;
                for y in top.max(oy)..bottom.min(oy + 256) {
                    input::check(cancel)?;
                    for x in left.max(ox)..right.min(ox + 256) {
                        crate::map_canvas::blend(
                            &mut crop.get_pixel_mut(x - left, y - top).0,
                            tile.get_pixel(x - ox, y - oy).0,
                        );
                    }
                }
            }
        }
        if crop.dimensions() == (self.target_size[0], self.target_size[1]) {
            return Ok(crop);
        }
        crate::map_raster::resize(
            &crop,
            [0, 0, crop.width(), crop.height()],
            self.target_size,
            cancel,
        )
    }
}

type Memory = VecDeque<(TileId, SystemTime, Arc<RgbaImage>)>;
pub struct Client {
    cache: PathBuf,
    http: reqwest::blocking::Client,
    memory: Mutex<Memory>,
    #[cfg(test)]
    endpoint: Option<String>,
}
impl Client {
    pub fn new(cache: PathBuf) -> Result<Self, String> {
        static HTTP: OnceLock<Result<reqwest::blocking::Client, String>> = OnceLock::new();
        let http = HTTP
            .get_or_init(|| {
                reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(20))
                    .connect_timeout(Duration::from_secs(8))
                    .user_agent("SleufBase-Rust/0.1 (+https://github.com/LukaStuurman/SleufBase)")
                    .build()
                    .map_err(|e| e.to_string())
            })
            .as_ref()
            .map_err(Clone::clone)?
            .clone();
        Ok(Self {
            cache,
            http,
            memory: Mutex::new(VecDeque::new()),
            #[cfg(test)]
            endpoint: None,
        })
    }
    pub fn request(
        &self,
        bounds: Bounds,
        size: [u32; 2],
        cancel: &AtomicBool,
    ) -> Result<RgbaImage, String> {
        input::check(cancel)?;
        let plan = Plan::new(bounds, size)?;
        plan.render_with(cancel, |ids| {
            std::thread::scope(|scope| {
                let handles = ids
                    .iter()
                    .map(|&id| scope.spawn(move || self.tile(id, cancel)))
                    .collect::<Vec<_>>();
                handles
                    .into_iter()
                    .map(|h| {
                        h.join()
                            .map_err(|_| "Kadastertegelworker is gestopt".to_owned())?
                    })
                    .collect()
            })
        })
    }
    fn remember(&self, id: TileId, modified: SystemTime, image: Arc<RgbaImage>) {
        if let Ok(mut memory) = self.memory.lock() {
            memory.retain(|(old, _, _)| *old != id);
            memory.push_back((id, modified, image));
            while memory.len() > MEMORY_TILES {
                memory.pop_front();
            }
        }
    }
    fn tile(&self, id: TileId, cancel: &AtomicBool) -> Result<Arc<RgbaImage>, String> {
        input::check(cancel)?;
        let fresh = |modified: SystemTime| modified.elapsed().is_ok_and(|elapsed| elapsed <= TTL);
        if let Ok(mut memory) = self.memory.lock() {
            if let Some(index) = memory
                .iter()
                .position(|(old, t, _)| *old == id && fresh(*t))
            {
                let hit = memory.remove(index).unwrap();
                let image = Arc::clone(&hit.2);
                memory.push_back(hit);
                return Ok(image);
            }
        }
        let path = id.path(&self.cache);
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() <= MAX_BYTES {
                if let Ok(modified) = meta.modified() {
                    if fresh(modified) {
                        if let Ok(bytes) = input::read(&path, MAX_BYTES, "Kadastertegel", cancel) {
                            if let Ok(image) = decode(&bytes) {
                                let image = Arc::new(image);
                                self.remember(id, modified, Arc::clone(&image));
                                return Ok(image);
                            }
                        }
                    }
                }
            }
        }
        let mut last = "Geen geldig antwoord".to_owned();
        for attempt in 0..3 {
            input::check(cancel)?;
            let result = (|| {
                let url = id.url();
                #[cfg(test)]
                let url = self
                    .endpoint
                    .as_ref()
                    .map(|base| format!("{base}/{}/{}/{}.png", id.zoom, id.x, id.y))
                    .unwrap_or(url);
                let mut response = self
                    .http
                    .get(url)
                    .send()
                    .map_err(|e| format!("Kadastertegel onbereikbaar: {e}"))?
                    .error_for_status()
                    .map_err(|e| format!("Kadaster HTTP-fout: {e}"))?;
                if !response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|v| v.to_ascii_lowercase().starts_with("image/"))
                {
                    return Err("Kadaster leverde geen afbeelding".into());
                }
                if response.content_length().is_some_and(|n| n > MAX_BYTES) {
                    return Err("Kadastertegel overschrijdt 2 MiB".into());
                }
                let bytes = input::read_chunks(&mut response, MAX_BYTES, "Kadastertegel", cancel)?;
                let image = Arc::new(decode(&bytes)?);
                input::check(cancel)?;
                if std::fs::create_dir_all(&self.cache).is_ok() {
                    if let Ok(mut file) = tempfile::NamedTempFile::new_in(&self.cache) {
                        use std::io::Write;
                        if file
                            .write_all(&bytes)
                            .and_then(|_| file.as_file_mut().sync_all())
                            .is_ok()
                        {
                            let _ = file.persist(&path);
                        }
                    }
                }
                self.remember(id, SystemTime::now(), Arc::clone(&image));
                Ok(image)
            })();
            match result {
                Ok(image) => return Ok(image),
                Err(error) => last = error,
            }
            // Cancellation is checked while backing off, rather than sleeping
            // for a complete retry interval inside the export worker.
            if attempt < 2 {
                for _ in 0..5 * (attempt + 1) {
                    input::check(cancel)?;
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
        input::check(cancel)?;
        Err(last)
    }
}
fn decode(bytes: &[u8]) -> Result<RgbaImage, String> {
    if bytes.len() as u64 > MAX_BYTES || image::guess_format(bytes).ok() != Some(ImageFormat::Png) {
        return Err("Kadastertegel is geen geldige begrensde PNG".into());
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(256);
    limits.max_image_height = Some(256);
    limits.max_alloc = Some(4 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .map_err(|e| format!("Ongeldige Kadastertegel: {e}"))?;
    if decoder.dimensions() != (256, 256) {
        return Err("Kadastertegel heeft geen 256x256 pixels".into());
    }
    image::DynamicImage::from_decoder(decoder)
        .map(|i| i.into_rgba8())
        .map_err(|e| format!("Ongeldige Kadasterpixels: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Instant,
    };
    fn png(size: u32, color: [u8; 4]) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        RgbaImage::from_pixel(size, size, Rgba(color))
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }
    fn response(status: &str, body: &[u8], length: Option<u64>) -> Vec<u8> {
        let mut bytes=format!("HTTP/1.1 {status}\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",length.unwrap_or(body.len() as u64)).into_bytes();
        bytes.extend_from_slice(body);
        bytes
    }
    // For batches >1 every request must arrive before any response is sent.
    // A sequential downloader would time out here rather than silently pass.
    fn server(replies: Vec<Vec<u8>>, batch: usize) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for replies in replies.chunks(batch) {
                let mut streams = Vec::new();
                let until = Instant::now() + Duration::from_secs(5);
                for _ in replies {
                    let mut stream = loop {
                        match listener.accept() {
                            Ok((s, _)) => break s,
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                assert!(Instant::now() < until, "missing concurrent tile requests");
                                thread::sleep(Duration::from_millis(2));
                            }
                            Err(e) => panic!("{e}"),
                        }
                    };
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut request = Vec::new();
                    let mut buffer = [0; 1024];
                    while !request.windows(4).any(|v| v == b"\r\n\r\n") {
                        let n = stream.read(&mut buffer).unwrap();
                        assert!(n > 0);
                        request.extend_from_slice(&buffer[..n]);
                        assert!(request.len() < 8192);
                    }
                    requests.push(String::from_utf8(request).unwrap());
                    streams.push(stream);
                }
                for (mut stream, reply) in streams.into_iter().zip(replies) {
                    let _ = stream.write_all(reply);
                }
            }
            requests
        });
        (url, handle)
    }
    #[test]
    fn retries_validate_then_share_pixels_and_reopen_fresh_disk_without_network() {
        let dir = tempfile::tempdir().unwrap();
        let id = TileId {
            zoom: 19,
            x: 270000,
            y: 170000,
        };
        let bytes = png(256, [33, 88, 122, 255]);
        let (url, server) = server(
            vec![
                response("503 Unavailable", b"unavailable", None),
                response("200 OK", b"not PNG", None),
                response("200 OK", &bytes, None),
            ],
            1,
        );
        let mut client = Client::new(dir.path().to_owned()).unwrap();
        client.endpoint = Some(url);
        let image = client.tile(id, &AtomicBool::new(false)).unwrap();
        assert_eq!(image.get_pixel(0, 0).0, [33, 88, 122, 255]);
        let again = client.tile(id, &AtomicBool::new(false)).unwrap();
        assert!(Arc::ptr_eq(&image, &again));
        assert_eq!(server.join().unwrap().len(), 3);
        assert_eq!(std::fs::read(id.path(dir.path())).unwrap(), bytes);
        // A closed local endpoint makes unintended refreshes fail immediately.
        client.memory.lock().unwrap().clear();
        assert_eq!(*client.tile(id, &AtomicBool::new(false)).unwrap(), *image);
        assert!(client.tile(id, &AtomicBool::new(true)).is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn expired_cache_and_wrong_dimensions_cannot_silently_publish_or_poison_tiles() {
        let dir = tempfile::tempdir().unwrap();
        let id = TileId {
            zoom: 19,
            x: 270000,
            y: 170000,
        };
        let path = id.path(dir.path());
        let old = png(256, [9, 9, 9, 255]);
        std::fs::write(&path, &old).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(SystemTime::now() - TTL * 2))
            .unwrap();
        let wrong = png(512, [1, 2, 3, 255]);
        let (url, server) = server(vec![response("200 OK", &wrong, None); 3], 1);
        let mut client = Client::new(dir.path().to_owned()).unwrap();
        client.endpoint = Some(url);
        assert!(client.tile(id, &AtomicBool::new(false)).is_err());
        assert_eq!(server.join().unwrap().len(), 3);
        assert_eq!(std::fs::read(&path).unwrap(), old);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        assert!(client.memory.lock().unwrap().is_empty());
    }
    #[test]
    fn excessive_responses_and_corrupt_fresh_cache_are_rejected_before_decode() {
        let dir = tempfile::tempdir().unwrap();
        let id = TileId {
            zoom: 19,
            x: 270000,
            y: 170000,
        };
        let path = id.path(dir.path());
        std::fs::write(&path, b"corrupt cache").unwrap();
        let (url, server) = server(vec![response("200 OK", b"", Some(MAX_BYTES + 1)); 3], 1);
        let mut client = Client::new(dir.path().to_owned()).unwrap();
        client.endpoint = Some(url);
        assert!(client
            .tile(id, &AtomicBool::new(false))
            .unwrap_err()
            .contains("2 MiB"));
        assert_eq!(server.join().unwrap().len(), 3);
        assert_eq!(std::fs::read(path).unwrap(), b"corrupt cache");
        assert!(decode(&png(512, [1, 2, 3, 255])).is_err());
    }
    #[test]
    fn real_client_fetches_four_at_once_and_reuses_cache_between_whole_maps() {
        let bounds = Bounds {
            min_x: 154800.,
            min_y: 462800.,
            max_x: 155200.,
            max_y: 463200.,
        };
        let size = [320, 240];
        let plan = Plan::new(bounds, size).unwrap();
        let count = plan.tiles().len();
        assert!(count >= 4);
        let bytes = png(256, [11, 77, 102, 255]);
        let (url, server) = server(vec![response("200 OK", &bytes, None); count], 4);
        let dir = tempfile::tempdir().unwrap();
        let mut client = Client::new(dir.path().to_owned()).unwrap();
        client.endpoint = Some(url);
        let image = client
            .request(bounds, size, &AtomicBool::new(false))
            .unwrap();
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), count);
        assert!(requests.iter().all(|r| r.contains("SleufBase-Rust")));
        assert!(image.pixels().all(|p| p.0 == [11, 77, 102, 255]));
        assert_eq!(
            client
                .request(bounds, size, &AtomicBool::new(false))
                .unwrap(),
            image
        );
        assert!(client.memory.lock().unwrap().len() <= MEMORY_TILES);
    }
}
