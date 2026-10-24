use serde_json::{json, Value};
use sleufbase_rust::{
    export_control::Control, geotiff::GeoTiffInfo, map_comments as notes, project::Comment,
};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/map-actions/reference.json")).unwrap()
}
fn rasters(c: &Value) -> Vec<GeoTiffInfo> {
    c["rasters"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, r)| GeoTiffInfo {
            path: format!("Proefsleuf {i} Ø.tif").into(),
            width: r["width"].as_u64().unwrap() as u32,
            height: r["height"].as_u64().unwrap() as u32,
            transform: serde_json::from_value(r["transform"].clone()).unwrap(),
            bounds: serde_json::from_value(r["bounds"].clone()).unwrap(),
            epsg: Some(28992),
            kickthemap_job_id: None,
            kickthemap_job_title: String::new(),
            kickthemap_job_sidecar: None,
            metadata: r["metadata"].clone(),
        })
        .collect()
}
fn metadata(rasters: &[GeoTiffInfo]) -> Value {
    json!(rasters.iter().map(|r| &r.metadata).collect::<Vec<_>>())
}
fn close(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            assert!((a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 2e-8)
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                close(a, b)
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (k, v) in a {
                close(v, &b[k])
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
fn original_comment_normalization_affine_centres_formatting_edits_and_clear_batches_match() {
    for c in fixture()["cases"].as_array().unwrap() {
        let mut rasters = rasters(c);
        let mut comments: Vec<Comment> = serde_json::from_value(c["comments"].clone()).unwrap();
        assert_eq!(
            json!(rasters
                .iter()
                .map(|r| notes::text(&r.metadata).unwrap())
                .collect::<Vec<_>>()),
            c["texts"]
        );
        let combined = notes::combined(&comments, &rasters).unwrap();
        close(&json!(combined), &c["combined"]);
        assert_eq!(
            json!(rasters
                .iter()
                .map(|r| sleufbase_rust::template_workflow::comments(r.bounds, &combined, ""))
                .collect::<Vec<_>>()),
            c["formatted"]
        );
        let i = c["selected"].as_u64().unwrap() as usize;
        if let Some(text) = c["edit"].as_str() {
            notes::set(&mut rasters[i], text).unwrap();
        }
        assert_eq!(metadata(&rasters), c["after_edit"]);
        if !notes::text(&rasters[i].metadata).unwrap().is_empty() {
            notes::set(&mut rasters[i], "").unwrap();
        }
        assert_eq!(metadata(&rasters), c["after_selected_clear"]);
        let count = notes::clear(&mut comments, &mut rasters).unwrap();
        assert_eq!(metadata(&rasters), c["after_clear"]);
        assert!(comments.is_empty());
        assert_eq!(
            if count == 0 {
                "Geen opmerkingen om te wissen.".into()
            } else {
                format!("{count} opmerking(en) gewist.")
            },
            c["clear_status"]
        );
        assert_eq!(notes::clear(&mut comments, &mut rasters).unwrap(), 0);
    }
}
#[test]
fn note_validation_is_atomic_and_preserves_unknown_metadata_and_native_project_roundtrip() {
    let mut rasters = rasters(&fixture()["cases"][0]);
    notes::set(&mut rasters[0], "  Nieuwe opmerking Ø\nTweede  ").unwrap();
    let before = rasters[0].metadata.clone();
    assert!(!notes::set(&mut rasters[0], "Nieuwe opmerking Ø\nTweede").unwrap());
    assert!(notes::set(&mut rasters[0], &"x".repeat(65537)).is_err());
    assert_eq!(rasters[0].metadata, before);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.json");
    let project = sleufbase_rust::project::NativeProject {
        rasters: rasters.clone(),
        comments: vec![Comment {
            x: 1.,
            y: 2.,
            text: "free note".into(),
        }],
        ..Default::default()
    };
    sleufbase_rust::project::save_project_new(&path, &project).unwrap();
    let loaded = sleufbase_rust::project::read_project(&path).unwrap();
    assert_eq!(metadata(&loaded.rasters), metadata(&rasters));
    let mut foreign = rasters[0].clone();
    foreign.epsg = Some(4326);
    assert!(notes::combined(&[], &[foreign]).is_err());
    rasters[1].metadata[notes::KEY] = Value::String("x".repeat(2 * 1024 * 1024));
    let original = metadata(&rasters);
    let mut comments = loaded.comments;
    assert!(notes::clear(&mut comments, &mut rasters).is_err());
    assert_eq!(metadata(&rasters), original);
    assert_eq!(comments[0].text, "free note");
    assert!(notes::combined(
        &[Comment {
            x: 0.,
            y: 0.,
            text: "x".repeat(16 * 1024 * 1024 + 1)
        }],
        &[]
    )
    .is_err());
}
#[test]
fn both_native_template_planners_use_free_and_tiff_notes_with_original_deduplication() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/template-export/reference.json")).unwrap();
    let original: sleufbase_rust::template_export::Request = serde_json::from_value(
        fixture
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "native-tiff")
            .unwrap()["request"]
            .clone(),
    )
    .unwrap();
    let src = original.slots[0]
        .as_ref()
        .unwrap()
        .tiff_source
        .as_ref()
        .unwrap();
    let path = root.join("..").join(&src.path);
    let raster = sleufbase_rust::raster::decode_full_with_cancel(
        &path,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    let mut input = sleufbase_rust::export_worker::Inputs {
        datasets: original.datasets.into_iter().take(1).collect(),
        rasters: vec![GeoTiffInfo {
            path,
            width: raster.width,
            height: raster.height,
            transform: src.transform,
            bounds: src.transform.bounds(raster.width, raster.height).unwrap(),
            epsg: Some(28992),
            kickthemap_job_id: Some(1),
            kickthemap_job_title: "PS1".into(),
            kickthemap_job_sidecar: None,
            metadata: json!({notes::KEY:"TIFF Ø\n water\nStraße\nTab\tkolom"}),
        }],
        comments: vec![
            Comment {
                x: 2.,
                y: 3.,
                text: "Water\nSTRASSE".into(),
            },
            Comment {
                x: 1e6,
                y: 1e6,
                text: "outside".into(),
            },
        ],
        ..Default::default()
    };
    let mut state = sleufbase_rust::template_order::State {
        profiles: false,
        paired: false,
        auto_bgt: false,
        ..Default::default()
    };
    state.sync(&input.rasters).unwrap();
    state.rows[0].comments = "Extra\nTIFF Ø".into();
    let request = sleufbase_rust::template_workflow::request(
        &state,
        &input,
        sleufbase_rust::RULES,
        &Control::default(),
    )
    .unwrap();
    let expected = "Opmerkingen:\\PWater\\PSTRASSE\\PTIFF Ø\\PTab\tkolom\\PExtra";
    assert_eq!(request.slots[0].as_ref().unwrap().comments, expected);
    let options = sleufbase_rust::template_project::Options {
        template: root.join("../assets/cadastral_template.dxf"),
        choices: {
            let mut c =
                sleufbase_rust::template_project::suggest(&input.datasets, &input.rasters, &[]);
            c[0].comments = "Extra\nTIFF Ø".into();
            c
        },
        ..Default::default()
    };
    let request = sleufbase_rust::template_project::build_from_inputs(
        &options,
        &input,
        sleufbase_rust::RULES,
        &Control::default(),
    )
    .unwrap();
    assert_eq!(request.slots[0].as_ref().unwrap().comments, expected);
    notes::clear(&mut input.comments, &mut input.rasters).unwrap();
    state.rows[0].comments.clear();
    let request = sleufbase_rust::template_workflow::request(
        &state,
        &input,
        sleufbase_rust::RULES,
        &Control::default(),
    )
    .unwrap();
    assert_eq!(request.slots[0].as_ref().unwrap().comments, "Opmerkingen:");
}
