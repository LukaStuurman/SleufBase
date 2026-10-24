//! Exercise the actual catalog on private temporary copies of original fixtures.
#[path = "../tests/common/font_resolution.rs"]
mod font_resolution;
fn main() -> Result<(), String> {
    let output = std::env::args()
        .nth(1)
        .ok_or("fresh output JSON required")?;
    let result = font_resolution::run()?;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| e.to_string())?;
    serde_json::to_writer(file, &result).map_err(|e| e.to_string())
}
