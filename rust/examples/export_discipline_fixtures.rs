//! Run the actual CLI with independently generated rules and job sidecars.
use serde::Deserialize;
use sleufbase_rust::{
    settings_compat::{self, MigratedSettings},
    Dataset, Geometry, Rule,
};
use std::{path::PathBuf, process::Command};
#[derive(Deserialize)]
struct Case {
    name: String,
    rules: Vec<Rule>,
    datasets: Vec<Dataset>,
}
fn main() {
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
                object_rules: case.rules,
                ..Default::default()
            },
        )
        .unwrap();
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
