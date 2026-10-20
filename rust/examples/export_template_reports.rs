//! Write original template-order discipline cases through the production writer.
use sleufbase_rust::{
    discipline_xlsx::{self, ReportRow},
    Dataset, Rule,
};
use std::path::PathBuf;
#[derive(serde::Deserialize)]
struct Row {
    label: String,
    dataset: Option<usize>,
}
#[derive(serde::Deserialize)]
struct Case {
    name: String,
    rules: Vec<Rule>,
    datasets: Vec<Dataset>,
    rows: Vec<Row>,
}
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
    let cases: Vec<Case> = serde_json::from_str(include_str!(
        "../tests/fixtures/template-reports/reference.json"
    ))
    .map_err(|e| e.to_string())?;
    for c in cases {
        discipline_xlsx::write_rows(
            &out.join(format!("{}.xlsx", c.name)),
            c.rows.iter().map(|r| ReportRow {
                label: Some(&r.label),
                dataset: r.dataset.map(|i| &c.datasets[i]),
            }),
            &c.rules,
        )?;
    }
    Ok(())
}
