//! Actual project planner/export path after whole-layer move and reference click.
use serde_json::json;
use sleufbase_rust::{
    export_control::Control,
    export_worker::Inputs,
    template_export,
    template_project::{self, Choice, Options, Source},
    virtual_map as map,
    virtual_trench::VirtualTrench,
};
use std::path::PathBuf;
fn main() -> Result<(), String> {
    let out = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("Fresh output directory required")?,
    );
    if out.exists() {
        return Err("Output already exists".into());
    }
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    std::fs::write(out.join("started.txt"), b"native virtual map").map_err(|e| e.to_string())?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/virtual-map/reference.json"))
            .map_err(|e| e.to_string())?;
    let rules = sleufbase_rust::settings_compat::MigratedSettings::default().object_rules;
    let mut audit = Vec::new();
    for (name, index, draw) in [
        ("virtual-reference", 1, true),
        ("measured-moved", 0, true),
        ("virtual-image-only", 1, false),
    ] {
        let mut t: VirtualTrench = serde_json::from_value(data["cases"][index]["source"].clone())
            .map_err(|e| e.to_string())?;
        map::translate(&mut t, 1.25, -2.5)?;
        let center = map::raster_center(&t);
        let world = [center[0] + 1.1, center[1] + 0.4];
        map::set_reference(&mut t, world)?;
        let inputs = Inputs {
            trenches: vec![t],
            ..Default::default()
        };
        let opts = Options {
            template: root.join("../assets/cadastral_template.dxf"),
            paired: false,
            project: "Virtuele kaart Ø".into(),
            drawn_by: "Technicus".into(),
            dpi: 96,
            include_bgt: false,
            include_cadastral: false,
            include_cad: false,
            choices: vec![Choice {
                source: Source::Virtual(0),
                raster: None,
                include: true,
                title: "PS12".into(),
                profile: draw,
                reverse: false,
                comments: String::new(),
                photos: Vec::new(),
            }],
            ..Default::default()
        };
        let mut request =
            template_project::build_from_inputs(&opts, &inputs, &rules, &Control::default())?;
        request.settings.discipline_excel = true;
        let path = out.join(format!("{name}.dxf"));
        let result = template_export::export(&path, &request, &Control::default())?;
        audit.push(json!({"name":name,"path":path.file_name().unwrap().to_string_lossy(),"label":request.slots[0].as_ref().unwrap().label,"draw":draw,"trench":inputs.trenches[0],"reference_world":world,"rules":rules,"dataset":request.datasets[0],"filled_slots":result.filled_slots}));
    }
    std::fs::write(
        out.join("audit.json"),
        serde_json::to_vec(&audit).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
