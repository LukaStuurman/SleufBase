//! Run the actual CLI with independently generated rules and job sidecars.
use serde::Deserialize;
use sleufbase_rust::{
    settings_compat::{self, MigratedSettings},
    Dataset, Geometry, Rule,
};
use std::{path::PathBuf, process::Command};
#[path = "support/worker.rs"]
mod worker;
use sleufbase_rust::{
    export_worker::{Exporter, Inputs, Kind, Request},
    object_selection::Selection,
};
#[derive(Deserialize)]
struct Case {
    name: String,
    rules: Vec<Rule>,
    label_gap: f64,
    datasets: Vec<Dataset>,
}
fn main() {
    let mut background = if std::env::args().any(|a| a == "--worker") {
        Some(Exporter::new().unwrap())
    } else {
        None
    };
    let mut args = std::env::args_os().skip(1);
    let output = PathBuf::from(args.next().expect("fresh output directory"));
    let exe = PathBuf::from(args.next().expect("native CLI executable"));
    std::fs::create_dir_all(&output).unwrap();
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("../tests/fixtures/disciplines/workbooks.json")).unwrap();
    for case in cases {
        let folder = output.join(&case.name);
        std::fs::create_dir_all(&folder).unwrap();
        let settings = folder.join("settings.json");
        settings_compat::save_rust_settings(
            &settings,
            &MigratedSettings {
                object_rules: case.rules.clone(),
                dxf_label_gap: case.label_gap,
                ..Default::default()
            },
        )
        .unwrap();
        if let Some(exporter) = background.as_mut() {
            let mut inputs = Inputs {
                datasets: case.datasets,
                ..Default::default()
            };
            for (kind, extension) in [(Kind::Dxf, "dxf"), (Kind::Xlsx, "xlsx")] {
                let request = Request {
                    kind,
                    path: output.join(format!("{}.{extension}", case.name)),
                    selection: Selection::all(inputs.datasets.len(), 0),
                    rules: case.rules.clone(),
                    extra_layers: false,
                    options: sleufbase_rust::object_dxf::Options {
                        label_gap: case.label_gap,
                        ..Default::default()
                    },
                };
                inputs = worker::run(exporter, inputs, request).unwrap();
            }
            continue;
        }
        let mut inputs = Vec::new();
        for dataset in case.datasets {
            let file = folder.join(format!("{}_jobFeatures.json", dataset.job_id));
            let features:Vec<_>=dataset.features.iter().map(|f| {
                let Geometry::Point(v)=f.geometry else {panic!("point fixture expected")};
                serde_json::json!({"geometry":{"type":"Point","coordinates":[v.x,v.y,v.z]},"properties":{"idName":f.source_name}})
            }).collect();
            std::fs::write(
                &file,
                serde_json::to_vec(&serde_json::json!({"features":features})).unwrap(),
            )
            .unwrap();
            std::fs::write(
                file.with_extension("json.job.json"),
                serde_json::to_vec(
                    &serde_json::json!({"job_id":dataset.job_id,"title":dataset.title}),
                )
                .unwrap(),
            )
            .unwrap();
            inputs.push(file);
        }
        for extension in ["xlsx", "dxf"] {
            let result = Command::new(&exe)
                .arg("--settings")
                .arg(&settings)
                .arg(format!("--{extension}"))
                .arg(output.join(format!("{}.{extension}", case.name)))
                .args(&inputs)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{} {extension}: {}",
                case.name,
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}
