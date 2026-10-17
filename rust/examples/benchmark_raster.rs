//! Standalone decoder/preview measurement; excludes GUI/GPU memory and startup time.
use sleufbase_rust::raster::{decode_preview, PreviewOptions};
use std::{path::PathBuf, time::Instant};

#[cfg(windows)]
fn peak_bytes() -> Option<usize> {
    use windows_sys::Win32::System::{
        ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetCurrentProcess,
    };
    // The SDK structure contains only integer counters; all-zero is valid before
    // setting its required size. The pseudo-handle belongs to this process.
    let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    (ok != 0).then_some(counters.PeakWorkingSetSize)
}
#[cfg(not(windows))]
fn peak_bytes() -> Option<usize> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmHWM:")?
                .split_whitespace()
                .next()?
                .parse::<usize>()
                .ok()
        })
        .map(|kib| kib * 1024)
}

fn main() {
    let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("Gebruik: benchmark_raster bestand.tiff");
        std::process::exit(2);
    };
    let start = Instant::now();
    let image = decode_preview(&path, PreviewOptions::default()).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    let crc32 = crc32fast::hash(&image.rgba);
    println!(
        "{}",
        serde_json::json!({
            "mode":"native-strip-preview","build_mode":if cfg!(debug_assertions){"debug"}else{"release"},
            "file":path.file_name().unwrap_or_default().to_string_lossy(),
            "width":image.width,"height":image.height,"crc32":crc32,"elapsed_ms":elapsed,
            "peak_working_set_bytes":peak_bytes(),"peak_chunk_bytes":image.stats.peak_chunk_bytes,
            "source_pixels":image.stats.source_pixels,"decoded_chunks":image.stats.decoded_chunks,"skipped_chunks":image.stats.skipped_chunks,
        })
    );
}
