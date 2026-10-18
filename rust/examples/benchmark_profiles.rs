//! Largest individual allocation during profile construction, excluding inputs.
//! This is neither total application heap nor OS/process/GPU memory measurement.
use sleufbase_rust::{
    profile::{self, Options},
    Dataset, Feature, Geometry, Vertex, RULES,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
    time::Instant,
};
struct Tracked;
static COUNT: AtomicBool = AtomicBool::new(false);
static MAX: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);
fn record(bytes: usize) {
    if COUNT.load(Relaxed) {
        MAX.fetch_max(bytes, Relaxed);
        CALLS.fetch_add(1, Relaxed);
    }
}
unsafe impl GlobalAlloc for Tracked {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            record(layout.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            record(layout.size());
        }
        p
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(p, layout, size) };
        if !p.is_null() {
            record(size);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) }
    }
}
#[global_allocator]
static ALLOC: Tracked = Tracked;
fn main() {
    println!("vertices,round,microseconds,allocation_calls,largest_allocation,profile_points");
    for count in [1_000, 100_000, 1_000_000] {
        let endpoint = |x| Feature {
            object_name: "endpoint".into(),
            source_name: "punt".into(),
            geometry: Geometry::Point(Vertex {
                x,
                y: 0.,
                z: Some(10.),
            }),
            attributes: Default::default(),
        };
        let data = Dataset {
            job_id: 1,
            title: "profile benchmark".into(),
            source_path: "fixture.json".into(),
            features: vec![
                endpoint(0.),
                endpoint(10.),
                Feature {
                    object_name: "pipe".into(),
                    source_name: "water".into(),
                    geometry: Geometry::Polyline(
                        (0..count)
                            .map(|i| Vertex {
                                x: 1. + 8. * i as f64 / (count - 1) as f64,
                                y: 0.,
                                z: Some(8.),
                            })
                            .collect(),
                    ),
                    attributes: Default::default(),
                },
            ],
        };
        let options = Options {
            forced_start: Some([0., 0.]),
            ..Default::default()
        };
        // Warm lazy rule caches before measuring the geometry kernel.
        std::hint::black_box(profile::build(&data, RULES, options).unwrap());
        for round in 1..=3 {
            MAX.store(0, Relaxed);
            CALLS.store(0, Relaxed);
            COUNT.store(true, Relaxed);
            let time = Instant::now();
            let result = profile::build(&data, RULES, options);
            let us = time.elapsed().as_micros();
            COUNT.store(false, Relaxed);
            let result = result.unwrap().unwrap();
            let largest = MAX.load(Relaxed);
            let calls = CALLS.load(Relaxed);
            assert_eq!(result.points.len(), 3);
            assert!((result.axis_length - 10.05).abs() < 1e-9);
            let pipe = result.points.iter().find(|p| !p.is_endpoint).unwrap();
            assert!((pipe.position.x - 5.).abs() < 1e-8);
            assert!((pipe.point_z - 8.).abs() < 1e-12);
            // A vector of vertex/segment copies would exceed this by orders of magnitude.
            assert!(
                largest < 8192,
                "unexpected vertex-sized temporary allocation: {largest}"
            );
            assert!(calls < 100, "unexpected per-vertex allocations: {calls}");
            println!(
                "{count},{round},{us},{calls},{largest},{}",
                result.points.len()
            );
        }
    }
}
