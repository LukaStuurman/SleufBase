//! Actual native overview outputs for independent original/ezdxf auditing.
use sleufbase_rust::{cadastral_overview, export_control::Control, template_export};
use std::path::PathBuf;
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 2 {
        let request =
            cadastral_overview::read_request(&PathBuf::from(&args[0]), &Control::default())?;
        cadastral_overview::export(&PathBuf::from(&args[1]), &request, &Control::default())?;
        return Ok(());
    }
    let output = PathBuf::from(args.first().ok_or("fresh output directory required")?);
    if output.exists() {
        return Err("Output already exists".into());
    }
    std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cadastral-overview");
    let cases: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("reference.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/template-export/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    let base = fixture
        .as_array()
        .ok_or("invalid template cases")?
        .iter()
        .find(|c| c["name"] == "native-tiff")
        .ok_or("native TIFF case missing")?["request"]
        .clone();
    for case in cases.as_array().ok_or("invalid cases")? {
        let mut request: cadastral_overview::Request =
            serde_json::from_value(case["request"].clone()).map_err(|e| e.to_string())?;
        for layer in &mut request.layers {
            layer.path = root.join(layer.path.file_name().ok_or("source filename missing")?);
        }
        let name = case["name"].as_str().ok_or("case name missing")?;
        let result = cadastral_overview::export(
            &output.join(format!("{name}.dxf")),
            &request,
            &Control::default(),
        )?;
        println!(
            "{name}: {} rasters, {} layers",
            result.images, result.layers
        );
        let mut value = base.clone();
        let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        value["template"] = serde_json::json!(repository.join("assets/cadastral_template.dxf"));
        value["overview"] = serde_json::to_value(&request).map_err(|e| e.to_string())?;
        value["settings"]["paired"] = serde_json::json!(true);
        for slot in value["slots"]
            .as_array_mut()
            .ok_or("template slots missing")?
            .iter_mut()
            .filter(|s| !s.is_null())
        {
            slot["tiff_source"]["path"] = serde_json::json!(
                repository.join("rust/tests/fixtures/template-raster/orientation-1.tiff")
            );
            slot["map_png"] = serde_json::json!("");
            slot["profile"] = serde_json::Value::Null;
            slot["map_source"] = serde_json::json!({"settings":{"dpi":40,"scale":225.},"background":{"kind":"solid","color":[245,245,245,255]}});
        }
        let request: template_export::Request =
            serde_json::from_value(value).map_err(|e| e.to_string())?;
        let result = template_export::export(
            &output.join(format!("template-{name}.dxf")),
            &request,
            &Control::default(),
        )?;
        println!(
            "template-{name}: {} paired slots, same atomic overview bundle",
            result.filled_slots
        );
    }
    Ok(())
}
