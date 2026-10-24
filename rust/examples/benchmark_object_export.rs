//! Exact same R2018 byte stream, buffered versus streaming sinks.
//! Additional tracked Rust heap only; excludes input ownership, GUI and OS RSS.
use sleufbase_rust::{object_dxf, Dataset, Feature, Geometry, Vertex, RULES};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    io::{self, Write},
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
    time::Instant,
};
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
struct Track;
fn add(n: usize) {
    let used = LIVE.fetch_add(n, Relaxed) + n;
    PEAK.fetch_max(used, Relaxed);
    ALLOCS.fetch_add(1, Relaxed);
}
unsafe impl GlobalAlloc for Track {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = System.alloc(l);
        if !p.is_null() {
            add(l.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = System.alloc_zeroed(l);
        if !p.is_null() {
            add(l.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Relaxed);
        System.dealloc(p, l);
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let result = System.realloc(p, l, new);
        if !result.is_null() {
            if new >= l.size() {
                add(new - l.size());
            } else {
                LIVE.fetch_sub(l.size() - new, Relaxed);
                ALLOCS.fetch_add(1, Relaxed);
            }
        }
        result
    }
}
#[global_allocator]
static ALLOCATOR: Track = Track;
struct Sink<W> {
    out: W,
    crc: crc32fast::Hasher,
    bytes: u64,
}
impl<W: Write> Write for Sink<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.out.write(bytes)?;
        self.crc.update(&bytes[..n]);
        self.bytes += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}
struct ResultRow {
    us: u128,
    allocs: usize,
    peak: usize,
    bytes: u64,
    crc: u32,
}
fn run<W: Write>(data: &Dataset, out: W) -> ResultRow {
    let baseline = LIVE.load(Relaxed);
    PEAK.store(baseline, Relaxed);
    ALLOCS.store(0, Relaxed);
    let mut sink = Sink {
        out,
        crc: crc32fast::Hasher::new(),
        bytes: 0,
    };
    let start = Instant::now();
    object_dxf::encode(
        &mut sink,
        std::slice::from_ref(data),
        &[],
        None,
        &[],
        RULES,
        Default::default(),
    )
    .unwrap();
    let us = start.elapsed().as_micros();
    let allocs = ALLOCS.load(Relaxed);
    let peak = PEAK.load(Relaxed).saturating_sub(baseline);
    std::hint::black_box(&sink.out);
    ResultRow {
        us,
        allocs,
        peak,
        bytes: sink.bytes,
        crc: sink.crc.finalize(),
    }
}
fn main() {
    let count = 100_000usize;
    let data = Dataset {
        job_id: 42,
        title: "PS42".into(),
        source_path: "bench.json".into(),
        features: (0..count)
            .map(|i| {
                let p = Vertex {
                    x: 168000. + i as f64 * 0.01,
                    y: 401000. + (i % 17) as f64 * 0.01,
                    z: Some(2.),
                };
                let geometry = if i % 2 == 0 {
                    Geometry::Point(p)
                } else {
                    Geometry::Polyline(vec![
                        p,
                        Vertex {
                            x: p.x + 0.02,
                            y: p.y + 0.03,
                            ..p
                        },
                        Vertex {
                            x: p.x + 0.04,
                            y: p.y + 0.01,
                            ..p
                        },
                    ])
                };
                Feature {
                    object_name: "Water".into(),
                    source_name: "water".into(),
                    geometry,
                    attributes: Default::default(),
                }
            })
            .collect(),
    };
    let mode = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    println!("mode,round,features,total_us,allocation_calls,additional_peak_tracked_heap_bytes,output_bytes,crc32,build_mode");
    for round in 1..=3 {
        let buffered = run(&data, Vec::<u8>::new());
        let stream = run(&data, io::sink());
        assert_eq!((buffered.bytes, buffered.crc), (stream.bytes, stream.crc));
        assert!(buffered.bytes > 10_000_000);
        assert!(
            stream.peak < buffered.peak / 10,
            "streaming must not retain the complete drawing"
        );
        for (name, r) in [("buffered", buffered), ("streaming", stream)] {
            println!(
                "{name},{round},{count},{},{},{},{},{:08x},{mode}",
                r.us, r.allocs, r.peak, r.bytes, r.crc
            );
        }
    }
}
