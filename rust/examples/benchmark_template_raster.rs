//! Already-decoded template image preparation; no TIFF/PNG/DXF/network I/O.
use image::{Rgba, RgbaImage};
use sleufbase_rust::{
    geo::GeoTransform,
    template_raster::{self, Options},
};
use std::{sync::atomic::AtomicBool, time::Instant};
#[cfg(windows)]
fn peak_bytes() -> Option<usize> {
    use windows_sys::Win32::System::{
        ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetCurrentProcess,
    };
    // Process pseudo-handle and an initialized SDK counter structure.
    let mut c: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    (ok != 0).then_some(c.PeakWorkingSetSize)
}
#[cfg(not(windows))]
fn peak_bytes() -> Option<usize> {
    None
}
fn main() -> Result<(), String> {
    let mode = std::env::args().nth(1).ok_or("mode required")?;
    let (w, h) = (1400u32, 1000u32);
    let mut image = RgbaImage::new(w, h);
    let alpha = [0, 1, 15, 16, 127, 254, 255];
    for y in 0..h {
        for x in 0..w {
            let p = if x < w / 8 || x >= w * 7 / 8 || y < h / 8 || y >= h * 7 / 8 {
                [250, 250, 250, 255]
            } else {
                [
                    ((x * 71 + y * 13) % 256) as u8,
                    ((x * 37 + y * 61) % 256) as u8,
                    ((x * 19 + y * 43) % 256) as u8,
                    alpha[((x + y * 3) % 7) as usize],
                ]
            };
            image.put_pixel(x, y, Rgba(p));
        }
    }
    let options = Options {
        transform: GeoTransform {
            a: 1.,
            b: 0.,
            c: 0.,
            d: 0.,
            e: -1.,
            f: 0.,
        },
        roads: &[],
        terrain: &[],
        axis: if mode == "rotate" {
            Some([[0., 0.], [10., 3.]])
        } else {
            None
        },
        reverse: false,
    };
    let start = Instant::now();
    let result = template_raster::prepare(image, &options, &AtomicBool::new(false))?;
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.;
    let peak = peak_bytes();
    if let Some(path) = std::env::args().nth(2) {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        file.write_all(result.image.as_raw())
            .map_err(|e| e.to_string())?;
    }
    println!(
        "{}",
        serde_json::json!({"mode":mode,"build_mode":if cfg!(debug_assertions){"debug"}else{"release"},"elapsed_ms":elapsed_ms,"peak_working_set_bytes":peak,"width":result.image.width(),"height":result.image.height(),"crc32":crc32fast::hash(result.image.as_raw())})
    );
    Ok(())
}
