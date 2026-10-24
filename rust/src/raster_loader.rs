//! One bounded background decoder per desktop session. Project changes cancel
//! pending work; generation checks prevent late pixels entering a new project.
use crate::raster::{decode_preview_with_cancel, PreviewOptions, RasterPreview};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
        Arc,
    },
};

pub const RASTER_TEXTURE_BUDGET: usize = 256 * 1024 * 1024;
pub fn preview_options_for_layers(count: usize) -> PreviewOptions {
    let pixels = (RASTER_TEXTURE_BUDGET / count.max(1) / 4).max(1) as u64;
    PreviewOptions {
        max_pixels: PreviewOptions::default().max_pixels.min(pixels),
        ..Default::default()
    }
}
struct Request {
    generation: u64,
    id: usize,
    path: PathBuf,
    options: PreviewOptions,
    cancel: Arc<AtomicBool>,
}
pub struct RasterReady {
    pub id: usize,
    pub path: PathBuf,
    pub result: Result<RasterPreview, String>,
}
struct Finished {
    generation: u64,
    ready: RasterReady,
}
pub struct RasterLoader {
    requests: SyncSender<Request>,
    results: Receiver<Finished>,
    generation: u64,
    cancel: Arc<AtomicBool>,
    options: PreviewOptions,
    pending: VecDeque<(usize, PathBuf)>,
    in_flight: Option<(usize, PathBuf)>,
}
impl RasterLoader {
    pub fn new() -> Result<Self, String> {
        let (requests, rx) = mpsc::sync_channel::<Request>(1);
        let (tx, results) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("sleufbase-raster".into())
            .spawn(move || {
                while let Ok(request) = rx.recv() {
                    let result = std::panic::catch_unwind(|| {
                        decode_preview_with_cancel(&request.path, request.options, &request.cancel)
                    })
                    .unwrap_or_else(|_| {
                        Err("Rasterdecoder is onverwacht gestopt bij dit bestand".into())
                    });
                    if tx
                        .send(Finished {
                            generation: request.generation,
                            ready: RasterReady {
                                id: request.id,
                                path: request.path,
                                result,
                            },
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| format!("Rasterachtergrondtaak kon niet starten: {e}"))?;
        Ok(Self {
            requests,
            results,
            generation: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            options: PreviewOptions::default(),
            pending: VecDeque::new(),
            in_flight: None,
        })
    }
    pub fn replace(&mut self, paths: Vec<(usize, PathBuf)>, options: PreviewOptions) {
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.generation = self.generation.wrapping_add(1);
        self.options = options;
        self.pending = paths.into();
        self.in_flight = None;
        self.pump();
    }
    pub fn remaining(&self) -> usize {
        self.pending.len() + usize::from(self.in_flight.is_some())
    }
    fn pump(&mut self) {
        if self.in_flight.is_some() {
            return;
        }
        let Some((id, path)) = self.pending.front() else {
            return;
        };
        let request = Request {
            generation: self.generation,
            id: *id,
            path: path.clone(),
            options: self.options,
            cancel: self.cancel.clone(),
        };
        match self.requests.try_send(request) {
            Ok(()) => {
                self.in_flight = self.pending.pop_front();
            }
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
        }
    }
    pub fn poll(&mut self) -> Vec<RasterReady> {
        let mut ready = Vec::new();
        loop {
            match self.results.try_recv() {
                Ok(result) if result.generation == self.generation => {
                    self.in_flight = None;
                    ready.push(result.ready);
                }
                Ok(_) => {} // Already cancelled; dropping the bounded result releases its pixels.
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if let Some((id, path)) = self.in_flight.take() {
                        ready.push(RasterReady {
                            id,
                            path,
                            result: Err("Rasterachtergrondtaak is gestopt".into()),
                        });
                    }
                    ready.extend(self.pending.drain(..).map(|(id, path)| RasterReady {
                        id,
                        path,
                        result: Err("Rasterachtergrondtaak is gestopt".into()),
                    }));
                    break;
                }
            }
        }
        self.pump();
        ready
    }
}
impl Drop for RasterLoader {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::Path,
        time::{Duration, Instant},
    };
    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/rasters")
            .join(name)
    }
    fn drain(loader: &mut RasterLoader) -> Vec<RasterReady> {
        let until = Instant::now() + Duration::from_secs(10);
        let mut results = Vec::new();
        while loader.remaining() > 0 {
            results.extend(loader.poll());
            assert!(Instant::now() < until, "raster loader did not finish");
            std::thread::sleep(Duration::from_millis(2));
        }
        results
    }
    #[test]
    fn replacement_never_returns_an_old_projects_pixels() {
        let mut loader = RasterLoader::new().unwrap();
        loader.replace(
            vec![
                (0, fixture("large-striped.tiff")),
                (1, fixture("rgb8-strips.tiff")),
            ],
            PreviewOptions::default(),
        );
        loader.replace(
            vec![(9, fixture("orientation6.tiff"))],
            PreviewOptions::default(),
        );
        let results = drain(&mut loader);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, 9);
        assert_eq!(
            (
                results[0].result.as_ref().unwrap().width,
                results[0].result.as_ref().unwrap().height
            ),
            (2, 3)
        );
    }
    #[test]
    fn a_bad_file_does_not_block_later_previews() {
        let mut loader = RasterLoader::new().unwrap();
        loader.replace(
            vec![
                (0, fixture("does-not-exist.tiff")),
                (1, fixture("rgb8-strips.tiff")),
            ],
            PreviewOptions::default(),
        );
        let results = drain(&mut loader);
        assert_eq!(results.len(), 2);
        assert!(results[0].result.is_err());
        assert!(results[1].result.is_ok());
    }
    #[test]
    fn aggregate_texture_budget_scales_to_many_rasters() {
        for count in [1, 16, 17, 100, 1000, 100_000] {
            let options = preview_options_for_layers(count);
            assert!(options.max_pixels * 4 * count as u64 <= RASTER_TEXTURE_BUDGET as u64);
            assert!(options.max_pixels > 0);
        }
    }
}
