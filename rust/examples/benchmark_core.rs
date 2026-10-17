//! Focused microbenchmarks against the retired preview implementation (1de19f4).
//! Run `cargo run --release --example benchmark_core`; no external files/accounts.
//! These do not establish end-to-end Python parity or total application memory.
use sleufbase_rust::{project::NativeProject, Dataset, Feature, Geometry, Vertex};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Instant,
};

struct CountedAllocator;
static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
// Forward every allocation unchanged; count calls only during measured operations.
unsafe impl GlobalAlloc for CountedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountedAllocator = CountedAllocator;

fn measure(name: &str, rounds: u32, mut action: impl FnMut()) -> u64 {
    action();
    ALLOCATIONS.store(0, Ordering::Relaxed);
    COUNTING.store(true, Ordering::Relaxed);
    let start = Instant::now();
    for _ in 0..rounds {
        action();
    }
    let elapsed = start.elapsed();
    COUNTING.store(false, Ordering::Relaxed);
    let allocations = ALLOCATIONS.load(Ordering::Relaxed);
    println!(
        "{name},{rounds},{},{},{}",
        elapsed.as_micros(),
        allocations,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    allocations
}

fn main() {
    let mut project = NativeProject::default();
    let features = (0..100_000)
        .map(|i| Feature {
            object_name: format!("Cable {i}"),
            source_name: "Laagspanning ls".into(),
            geometry: Geometry::Point(Vertex {
                x: 168000.0 + (i % 1000) as f64 * 0.01,
                y: 401000.0 + (i / 1000) as f64 * 0.01,
                z: Some(3.12),
            }),
            attributes: ["measured".into(), "supplier".into(), format!("object-{i}")],
        })
        .collect();
    project.datasets.push(Dataset {
        job_id: 123,
        title: "PS benchmark".into(),
        source_path: "synthetic.json".into(),
        features,
    });
    println!("operation,rounds,total_us,allocation_calls,build_mode");
    measure("camera_input_retired_clone", 20, || {
        black_box(vec![black_box(&project.datasets[0]).clone()]);
    });
    let borrowed = measure("camera_input_borrowed", 20, || {
        black_box(std::slice::from_ref(black_box(&project.datasets[0])));
    });
    assert_eq!(borrowed, 0, "borrowed camera input allocated memory");
    // Original per-feature temporary Vec allocation, copied from project.rs at
    // 1de19f4. Compare the unchanged numeric acceptance contract on this dataset.
    measure("validation_retired_temporary_vectors", 20, || {
        for ds in &black_box(&project).datasets {
            for feature in &ds.features {
                let points: Vec<&Vertex> = match &feature.geometry {
                    Geometry::Point(p) => vec![p],
                    Geometry::Polyline(p) => p.iter().collect(),
                };
                assert!(!points.iter().any(|p| !p.x.is_finite()
                    || !p.y.is_finite()
                    || p.z.is_some_and(|z| !z.is_finite())));
            }
        }
    });
    let validated = measure("validation_current", 20, || {
        black_box(black_box(&project).validate()).unwrap();
    });
    assert_eq!(
        validated, 0,
        "successful validation allocated per-feature buffers"
    );
}
