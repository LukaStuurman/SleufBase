//! Real exporter: compare paired contents to independent forward/reverse calls.
use sleufbase_rust::{
    export_control::Control,
    template_export,
    virtual_trench::{TrenchPoint, VirtualTrench},
    Vertex,
};
use std::path::PathBuf;
fn main() -> Result<(), String> {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("fresh output directory required")?,
    );
    if out.exists() {
        return Err("Output already exists".into());
    }
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/template-export/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    let base = fixture
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "native-tiff")
        .unwrap()["request"]
        .clone();
    for name in [
        "profiles",
        "photos-no-draw",
        "virtual",
        "image-only",
        "extended",
        "stroke-font",
        "lff-font",
        "bigfont-fallback",
    ] {
        let dir = out.join(name);
        std::fs::create_dir(&dir).map_err(|e| e.to_string())?;
        let mut value = base.clone();
        value["template"] = serde_json::json!(root.join("assets/cadastral_template.dxf"));
        if name == "stroke-font" || name == "lff-font" || name == "bigfont-fallback" {
            let file = if name == "bigfont-fallback" {
                "big.shx"
            } else if name == "lff-font" {
                "fixture.lff"
            } else {
                "fixture.shx"
            };
            let data: &[u8] = if name == "bigfont-fallback" {
                b"AutoCAD-86 bigfont 1.0\r\n\x1aOriginal fixture"
            } else if name == "lff-font" {
                include_bytes!("../tests/fixtures/lff-font/polygons.lff")
            } else {
                include_bytes!("../tests/fixtures/stroke-font/fixture-1.0.shx")
            };
            let fonts = dir.join("cad-fonts");
            std::fs::create_dir(&fonts).map_err(|e| e.to_string())?;
            std::fs::write(fonts.join(file), data).map_err(|e| e.to_string())?;
            let source = std::fs::read_to_string(root.join("assets/cadastral_template.dxf"))
                .map_err(|e| e.to_string())?;
            if !source.contains("NLCS-ISO.ttf") {
                return Err("Missing profile style font".into());
            }
            let template = dir.join("stroke-template.dxf");
            // Change only this dev fixture's font references, retaining proxy/graph bytes.
            std::fs::write(&template, source.replace("NLCS-ISO.ttf", file))
                .map_err(|e| e.to_string())?;
            value["template"] = serde_json::json!(template);
            value["settings"]["font_directories"] = serde_json::json!([fonts]);
        }
        value["settings"]["project"] = serde_json::json!("Normaal / Reverse Ø 漢");
        value["settings"]["export_date"] = serde_json::json!([2026, 10, 20]);
        value["settings"]["discipline_excel"] = serde_json::json!(true);
        for slot in value["slots"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .filter(|s| !s.is_null())
        {
            slot["label"] = serde_json::json!("PS 2/A?");
            slot["tiff_source"]["path"] = serde_json::json!(
                root.join("rust/tests/fixtures/template-raster/orientation-1.tiff")
            );
            slot["tiff_png"] = serde_json::json!("");
            slot["map_png"] = serde_json::json!("");
            slot["map_source"] = serde_json::json!({"settings":{"dpi":40,"scale":225.},"background":{"kind":"solid","color":[245,245,245,255]}});
            if name == "photos-no-draw" {
                slot["profile"]["draw"] = serde_json::json!(false);
                slot["profile"]["show_reference"] = serde_json::json!(false);
                slot["photos"] = serde_json::json!([{"path":root.join("rust/tests/fixtures/template-export/tiff.png"),"turns":1,"area":"tiff"},{"path":root.join("rust/tests/fixtures/template-export/map.png"),"turns":3,"area":"map"}]);
            }
            if name == "image-only" {
                slot["profile"] = serde_json::Value::Null;
            }
        }
        if name == "virtual" {
            let mut trench = VirtualTrench::new(
                "Virtuele PS Ø".into(),
                Vertex {
                    x: 0.,
                    y: 0.,
                    z: Some(-0.2),
                },
                Vertex {
                    x: 10.,
                    y: 4.,
                    z: Some(-0.7),
                },
                0.6,
            )?;
            trench.objects.push(TrenchPoint {
                role: "water".into(),
                object_name: "Water".into(),
                source_name: "Water".into(),
                position: Vertex {
                    x: 5.,
                    y: 2.,
                    z: Some(-1.2),
                },
                attributes: Default::default(),
            });
            value["datasets"] = serde_json::json!([trench.to_dataset(trench.job_id, true)]);
            let bounds = sleufbase_rust::virtual_raster::layout(&trench, 1.)?.bounds;
            for slot in value["slots"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .filter(|s| !s.is_null())
            {
                slot["tiff_source"] = serde_json::Value::Null;
                slot["virtual_source"] =
                    serde_json::json!({"trench":trench,"quality":1.,"reverse":false});
                slot["bounds"] = serde_json::to_value(bounds).map_err(|e| e.to_string())?;
                slot["profile"]["dataset"] = serde_json::json!(0);
                slot["profile"]["forced_start"] = serde_json::json!([0., 0.]);
            }
        }
        if name == "extended" {
            let slots = value["slots"].as_array_mut().unwrap();
            let last = slots.iter().rfind(|s| !s.is_null()).unwrap().clone();
            for s in slots.iter_mut().skip(1) {
                *s = serde_json::Value::Null;
            }
            slots.resize(40, serde_json::Value::Null);
            slots[39] = last;
        }
        for mode in ["normal", "reverse", "paired"] {
            value["settings"]["paired"] = serde_json::json!(mode == "paired");
            if mode != "paired" {
                for slot in value["slots"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .filter(|s| !s.is_null())
                {
                    if slot["profile"].is_object() {
                        slot["profile"]["reverse"] = serde_json::json!(mode == "reverse");
                    }
                    if slot["tiff_source"].is_object() {
                        slot["tiff_source"]["reverse_orientation"] =
                            serde_json::json!(mode == "reverse");
                    }
                    if slot["virtual_source"].is_object() {
                        slot["virtual_source"]["reverse"] = serde_json::json!(mode == "reverse");
                    }
                }
            }
            let manifest = dir.join(format!("{mode}-input.json"));
            std::fs::write(
                &manifest,
                serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            let request = template_export::read_request(&manifest, &Control::default())?;
            let result = template_export::export(
                &dir.join(format!("{mode}.dxf")),
                &request,
                &Control::default(),
            )?;
            std::fs::write(dir.join(format!("{mode}-result.json")),serde_json::to_vec(&serde_json::json!({"filled":result.filled_slots,"profiles":result.profiles,"pages":result.added_pages,"assets":result.published.assets,"companion":result.companion,"warnings":result.warnings})).unwrap()).map_err(|e|e.to_string())?;
        }
    }
    Ok(())
}
