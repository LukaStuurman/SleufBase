//! End-to-end native UI worker audit, independent of app settings/accounts.
use sleufbase_rust::{
    export_worker::{Exporter, Inputs, Kind, Request},
    geotiff::GeoTiffInfo,
    template_export,
    template_order::{Row, State},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
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
    std::fs::write(out.join("started.txt"), b"native template workflow")
        .map_err(|e| e.to_string())?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/template-export/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    let original: template_export::Request = serde_json::from_value(
        fixture
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "native-tiff")
            .unwrap()["request"]
            .clone(),
    )
    .map_err(|e| e.to_string())?;
    let source = original.slots[0]
        .as_ref()
        .unwrap()
        .tiff_source
        .as_ref()
        .unwrap();
    let path = root.join("..").join(&source.path);
    let raster = sleufbase_rust::raster::decode_full_with_cancel(
        &path,
        &std::sync::atomic::AtomicBool::new(false),
    )?;
    let mut inputs = Inputs {
        datasets: original.datasets,
        rasters: vec![GeoTiffInfo {
            path,
            width: raster.width,
            height: raster.height,
            transform: source.transform,
            bounds: source.transform.bounds(raster.width, raster.height)?,
            epsg: Some(28992),
            kickthemap_job_id: Some(1),
            kickthemap_job_title: "PS1".into(),
            kickthemap_job_sidecar: None,metadata:Default::default(),
        }],
        comments: vec![sleufbase_rust::project::Comment {
            x: 2.,
            y: 3.,
            text: "Grondwater\n grondwater\nStraße\nSTRASSE".into(),
        }],
        ..Default::default()
    };
    let rules = original.rules;
    inputs.overlays = vec![sleufbase_rust::dxf_read::DxfOverlay {
        visible: true,
        path: "source-cad.dxf".into(),
        features: vec![sleufbase_rust::dxf_read::DxfFeature {
            entity_type: "LINE".into(),
            handle: "AA".into(),
            layer: "CAD_voorbeeld".into(),
            color_aci: 1,
            points: vec![
                sleufbase_rust::Vertex {
                    x: 1.,
                    y: 1.,
                    z: None,
                },
                sleufbase_rust::Vertex {
                    x: 8.,
                    y: 5.,
                    z: None,
                },
            ],
            raw_text: String::new(),
            block_name: String::new(),
            closed: Some(false),
            cad: None,
        }],
        ..Default::default()
    }];
    let original_pointer = inputs.datasets[0].features.as_ptr();
    let original_raster = inputs.rasters.as_ptr();
    let mut worker = Exporter::new()?;
    let mut audit = Vec::new();
    for (name, reverse, draw, duplicates) in [
        ("normal", false, true, false),
        ("reverse", true, true, false),
        ("image-only", false, false, false),
        ("ordered-variants", false, true, true),
        ("missing-measurements", false, false, false),
        ("selected-points", false, true, false),
        ("tiff-comments", false, true, false),
    ] {
        inputs.rasters[0].metadata = if name == "selected-points" {
            serde_json::json!({"template_cross_section_start_point":{"x":10.,"y":0.},"template_reference_point":{"x":3.,"y":2.},"notes":"keep"})
        } else if name=="tiff-comments" {
            serde_json::json!({"tiff_comment_text":"TIFF Ø\n grondwater\nExtra\tdetail"})
        } else { serde_json::Value::Null };
        inputs.rasters[0].kickthemap_job_id = Some(if name == "missing-measurements" {
            999
        } else {
            1
        });
        let mut state = State {
            paired: false,
            project: "Native venster Ø 漢".into(),
            drawn_by: "Native test".into(),
            aerial: false,
            auto_bgt: false,
            reverse,
            profiles: draw,
            ..Default::default()
        };
        state.sync(&inputs.rasters)?;
        state.rows[0].number = "12".into();
        state.rows[0].address = Some("Adres:\\PTeststraat 12".into());
        if name == "selected-points" {
            state.rows[0].metadata=serde_json::json!({"template_cross_section_start_point":{"x":0.,"y":0.},"template_reference_point":{"x":99.,"y":99.}});
        }
        if name=="tiff-comments" {state.rows[0].comments="Extra slot\nTIFF Ø".into();}
        if duplicates {
            let mut second = state.rows[0].clone();
            second.variant = "AUTO".into();
            let mut excluded = second.clone();
            excluded.included = false;
            state.rows.extend([Row::default(), second, excluded]);
        }
        let output = out.join(format!("{name}.dxf"));
        worker
            .start(
                inputs,
                Request {
                    kind: Kind::Template,
                    path: output.clone(),
                    selection: sleufbase_rust::object_selection::Selection::all(0, 0),
                    rules: rules.clone(),
                    extra_layers: false,
                    options: Default::default(),
                    template: Some(Box::new(state.clone())),
                    discipline_excel: true,
                    maps: None,point_target:None,point_network:false,
                },
            )
            .map_err(|e| e.message.clone())?;
        let deadline = Instant::now() + Duration::from_secs(90);
        let ready = loop {
            if let Some(r) = worker.poll() {
                break r;
            }
            if Instant::now() > deadline {
                return Err("native worker timed out".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        };
        let result = ready.result?;
        if result.companion.as_deref() != Some(output.with_extension("xlsx").as_path()) {
            return Err(format!("Companion missing: {:?}", result.warnings));
        }
        inputs = ready.inputs;
        if inputs.datasets[0].features.as_ptr() != original_pointer
            || inputs.rasters.as_ptr() != original_raster
        {
            return Err("Source allocations changed".into());
        }
        audit.push(serde_json::json!({"name":name,"state":state,"draw":draw,"profiles":if draw{if duplicates{2}else{1}}else{0},"images":if duplicates{4}else{2},"path":output.file_name().unwrap().to_string_lossy()}));
    }
    std::fs::write(
        out.join("audit.json"),
        serde_json::to_vec_pretty(&audit).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
