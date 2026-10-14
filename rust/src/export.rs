use crate::disciplines::{matching_rule, summarize, Rule};
use crate::model::{Dataset, Geometry, Vertex};
use rust_xlsxwriter::{Color, Format, Workbook};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

/// Do not silently overwrite a production drawing/report.
fn prepare_output(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err(format!("Bestand bestaat al: {}. Kies een nieuwe naam.", path.display()));
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("Map maken mislukt: {e}"))?;
        }
    }
    Ok(())
}

/// The first migration milestone: disciplinary summary of locally loaded
/// jobFeatures.json files. Uses the same labels and counts as the Python code.
pub fn write_xlsx(path: &Path, datasets: &[Dataset], rules: &[Rule]) -> Result<(), String> {
    if datasets.is_empty() { return Err("Geen ingeladen jobs".into()); }
    prepare_output(path)?;
    let header = Format::new()
        .set_bold().set_font_color(Color::White)
        .set_background_color(Color::RGB(0xC2410C));
    let body = Format::new().set_background_color(Color::RGB(0xFFF7ED));
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("Disciplines").map_err(|e| e.to_string())?;
    sheet.set_column_width(0, 28).map_err(|e| e.to_string())?;
    sheet.set_column_width(1, 24).map_err(|e| e.to_string())?;
    sheet.set_column_width(2, 28).map_err(|e| e.to_string())?;
    for (col, width) in (3..(3 + rules.len())).map(|col| (col, 22.0)) {
        sheet.set_column_width(col as u16, width).map_err(|e| e.to_string())?;
    }
    let headings: Vec<&str> = std::iter::once("Proefsleuf")
        .chain(["Aantal verschillende disciplines", "Totaal aantal kabels/leidingen"])
        .chain(rules.iter().map(|r| r.label)).collect();
    for (col, name) in headings.iter().enumerate() {
        sheet.write_string_with_format(0, col as u16, *name, &header)
            .map_err(|e| e.to_string())?;
    }
    for (index, dataset) in datasets.iter().enumerate() {
        let row = (index + 1) as u32;
        let summary = summarize(dataset, rules);
        sheet.write_string_with_format(row, 0, &summary.proefsleuf, &body)
            .map_err(|e| e.to_string())?;
        sheet.write_number_with_format(row, 1, summary.distinct_count() as f64, &body)
            .map_err(|e| e.to_string())?;
        sheet.write_number_with_format(row, 2, summary.total_count() as f64, &body)
            .map_err(|e| e.to_string())?;
        for (col, count) in summary.counts.iter().enumerate() {
            sheet.write_number_with_format(row, (col + 3) as u16, *count as f64, &body)
                .map_err(|e| e.to_string())?;
        }
    }
    let temporary = path.with_extension("xlsx.partial");
    if temporary.exists() { return Err("Tijdelijk exportbestand bestaat al".into()); }
    workbook.save(&temporary).map_err(|e| e.to_string())?;
    if let Err(err) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("Excel-export verplaatsen mislukt: {err}"));
    }
    Ok(())
}

fn write_line(out: &mut String, layer: &str, color: u16, start: &Vertex, end: &Vertex) {
    // ASCII DXF R12 LINE entity (basic CAD interoperability; no template).
    let _ = write!(out,
        "0\nLINE\n8\n{layer}\n62\n{color}\n10\n{:.8}\n20\n{:.8}\n30\n{:.8}\n11\n{:.8}\n21\n{:.8}\n31\n{:.8}\n",
        start.x, start.y, start.z.unwrap_or(0.0),
        end.x, end.y, end.z.unwrap_or(0.0));
}

pub fn write_dxf(path: &Path, datasets: &[Dataset], rules: &[Rule]) -> Result<(), String> {
    if datasets.is_empty() { return Err("Geen ingeladen jobs".into()); }
    prepare_output(path)?;
    let mut out = String::from(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n"
    );
    let mut segment_count = 0usize;
    for dataset in datasets {
        for feature in &dataset.features {
            let rule = matching_rule(&feature.source_name, rules);
            let (layer, color) = rule.map_or(("NIET_INGEDEELD", 7), |r| (r.target_layer, r.color_aci));
            match &feature.geometry {
                Geometry::Point(v) => {
                    let d = 0.5;
                    write_line(&mut out, layer, color,
                        &Vertex {x:v.x-d,y:v.y,z:v.z}, &Vertex {x:v.x+d,y:v.y,z:v.z});
                    write_line(&mut out, layer, color,
                        &Vertex {x:v.x,y:v.y-d,z:v.z}, &Vertex {x:v.x,y:v.y+d,z:v.z});
                    segment_count += 2;
                }
                Geometry::Polyline(vertices) => {
                    for pair in vertices.windows(2) {
                        write_line(&mut out, layer, color, &pair[0], &pair[1]);
                        segment_count += 1;
                    }
                }
            }
        }
    }
    if segment_count == 0 { return Err("Geen lijnen om te exporteren".into()); }
    out.push_str("0\nENDSEC\n0\nEOF\n");
    let temporary = path.with_extension("dxf.partial");
    if temporary.exists() { return Err("Tijdelijk exportbestand bestaat al".into()); }
    fs::write(&temporary, out).map_err(|e| format!("DXF-export schrijven mislukt: {e}"))?;
    if let Err(err) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("DXF-export verplaatsen mislukt: {err}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Dataset, Feature, Geometry, Vertex};
    use crate::disciplines::RULES;
    use std::path::PathBuf;

    #[test]
    fn exports_dxf_line_entities_and_rejects_overwrite() {
        let dir = std::env::temp_dir().join(format!("sleufbase-rust-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let output = dir.join("test_line.dxf");
        let _ = fs::remove_file(&output);
        let dataset = Dataset {
            job_id: 4, title: "PS4".into(), source_path: PathBuf::new(),
            features: vec![Feature {
                object_name: "Water".into(), source_name: "water".into(),
                geometry: Geometry::Polyline(vec![
                    Vertex {x:1.0,y:2.0,z:None}, Vertex {x:3.0,y:4.0,z:None}
                ]),
                attributes: Default::default(),
            }],
        };
        write_dxf(&output, &[dataset.clone()], RULES).unwrap();
        let contents = fs::read_to_string(&output).unwrap();
        assert!(contents.contains("N-OI-KL-WATER"));
        assert!(contents.contains("0\nLINE\n"));
        assert!(contents.ends_with("0\nEOF\n"));
        assert!(write_dxf(&output, &[dataset], RULES).is_err());
        fs::remove_file(output).unwrap();
    }
}