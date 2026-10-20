use sleufbase_rust::dxf_fonts::Catalog;
use std::sync::atomic::AtomicBool;
fn main() -> Result<(), String> {
    let cancel = AtomicBool::new(false);
    let mut catalog = Catalog::system(&cancel)?;
    let mut result = std::collections::BTreeMap::new();
    for request in std::env::args().skip(1) {
        catalog.resolve(&request, &cancel)?;
        result.insert(
            request.clone(),
            catalog.selected_path(&request).map(|p| p.to_owned()),
        );
    }
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|e| e.to_string())?
    );
    Ok(())
}
