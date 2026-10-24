//! Fresh-process real-template load + composed preparation (no final DXF I/O).
use sleufbase_rust::{
    dxf_document::Document,
    template_metadata::Date,
    template_setup::{self, Options},
    Bounds,
};
use std::{path::PathBuf, sync::atomic::AtomicBool, time::Instant};
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
    let path = PathBuf::from(std::env::args().nth(1).ok_or("template path required")?);
    let cancel = AtomicBool::new(false);
    let slots = vec![
        Some(Bounds {
            min_x: 120000.,
            min_y: 450000.,
            max_x: 120030.,
            max_y: 450040.
        });
        64
    ];
    let options = Options {
        project: Some("Benchmark project".into()),
        drawn_by: "User".into(),
        export_date: Some(Date::new(2026, 10, 19)?),
        label_gap: 18.,
        profile_layers: None,
    };
    let start = Instant::now();
    let mut doc = Document::load(&path, &cancel)?;
    let result = template_setup::prepare(&mut doc, &slots, &options, &cancel)?;
    let layouts = doc.records().filter(|r| r.kind() == "LAYOUT").count();
    let attributes = doc.records().filter(|r| r.kind() == "ATTRIB").count();
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.;
    println!(
        "{}",
        serde_json::json!({"build_mode":if cfg!(debug_assertions){"debug"}else{"release"},"elapsed_ms":elapsed_ms,"peak_working_set_bytes":peak_bytes(),"slots":result.slots.len(),"added_pages":result.added_pages,"restored_legends":result.restored_legends,"layouts":layouts,"attributes":attributes})
    );
    std::hint::black_box(doc);
    Ok(())
}
