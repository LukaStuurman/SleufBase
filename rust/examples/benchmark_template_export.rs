//! Fresh-process prepared export, including PNG validation, DXF write and fsync.
use sleufbase_rust::{
    export_control::Control,
    template_export::{self, Request},
};
use std::{path::PathBuf, time::Instant};
#[derive(serde::Deserialize)]
struct Case {
    name: String,
    request: Request,
}
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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let cases: Vec<Case> = serde_json::from_str(include_str!(
        "../tests/fixtures/template-export/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    let mut request = cases
        .into_iter()
        .find(|c| c.name == "mixed")
        .ok_or("missing benchmark input")?
        .request;
    request.template = root.join(&request.template);
    for slot in request.slots.iter_mut().flatten() {
        slot.tiff_png = root.join(&slot.tiff_png);
        slot.map_png = root.join(&slot.map_png);
    }
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let output = temp.path().join("benchmark.dxf");
    let start = Instant::now();
    let result = template_export::export(&output, &request, &Control::default())?;
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.;
    let drawing_bytes = std::fs::metadata(&output).map_err(|e| e.to_string())?.len();
    let image_files = std::fs::read_dir(&result.published.assets)
        .map_err(|e| e.to_string())?
        .count();
    println!(
        "{}",
        serde_json::json!({"build_mode":if cfg!(debug_assertions){"debug"}else{"release"},"elapsed_ms":elapsed_ms,"peak_working_set_bytes":peak_bytes(),"filled_slots":result.filled_slots,"profiles":result.profiles,"image_files":image_files,"drawing_bytes":drawing_bytes})
    );
    Ok(())
}
