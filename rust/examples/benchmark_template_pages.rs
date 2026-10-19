//! Fresh-process load + real template extension. No DXF output, GUI or network.
use sleufbase_rust::{dxf_document::Document, template_pages};
use std::{path::PathBuf, sync::atomic::AtomicBool, time::Instant};
#[cfg(windows)]
fn peak_bytes() -> Option<usize> {
    use windows_sys::Win32::System::{
        ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetCurrentProcess,
    };
    // Valid process pseudo-handle and initialized integer SDK structure.
    let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    (ok != 0).then_some(counters.PeakWorkingSetSize)
}
#[cfg(not(windows))]
fn peak_bytes() -> Option<usize> {
    None
}
fn main() -> Result<(), String> {
    let path = PathBuf::from(std::env::args().nth(1).ok_or("template path required")?);
    let cancel = AtomicBool::new(false);
    let start = Instant::now();
    let mut doc = Document::load(&path, &cancel)?;
    let result = template_pages::extend(&mut doc, 64, &cancel)?;
    let model = doc
        .records()
        .find(|r| r.kind() == "BLOCK_RECORD" && r.first(2) == Some("*Model_Space"))
        .and_then(|r| r.handle())
        .ok_or("model owner missing")?;
    let entities = doc
        .records()
        .filter(|r| {
            r.section() == Some("ENTITIES")
                && r.first(330)
                    .and_then(|v| u64::from_str_radix(v.trim(), 16).ok())
                    == Some(model)
        })
        .count();
    let layouts = doc.records().filter(|r| r.kind() == "LAYOUT").count();
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.;
    let peak = peak_bytes();
    println!(
        "{}",
        serde_json::json!({"build_mode":if cfg!(debug_assertions){"debug"}else{"release"},"elapsed_ms":elapsed_ms,"peak_working_set_bytes":peak,"slots":result.slots.len(),"added_pages":result.added_pages,"layouts":layouts,"model_entities":entities})
    );
    std::hint::black_box(doc);
    Ok(())
}
