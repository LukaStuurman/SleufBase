use crate::disciplines::{matching_rule, summarize, Rule};
use crate::model::{Dataset, Geometry, Vertex};
use crate::virtual_trench::VirtualTrench;
use crate::cadastral::CadastralLayer;
use crate::bgt::{BgtKind,BgtLayer};
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
    write_dxf_with_trenches(path,datasets,&[],rules)
}

/// Exports a standards-compatible R12 3D POLYLINE for each measured MarXact
/// boundary or manually created virtual trench, preserving measured elevations.
/// The DXF template/AutoCAD dynamic blocks are still separate future parity work.
pub fn write_dxf_with_trenches(path: &Path, datasets: &[Dataset],
    trenches: &[VirtualTrench],rules: &[Rule]) -> Result<(), String> {
    write_dxf_with_layers(path,datasets,trenches,None,&[],rules)
}
/// Base CAD export (not a template): object lines, 3D measured trench
/// boundaries, PDOK cadastral boundaries and text with measured rotation,
/// and selected BGT linework, each on separate named AutoCAD layers.
pub fn write_dxf_with_layers(path:&Path,datasets:&[Dataset],
    trenches:&[VirtualTrench],cadastral:Option<&CadastralLayer>,
    bgt:&[BgtLayer],rules:&[Rule]) -> Result<(),String> {
    if datasets.is_empty() && trenches.is_empty() &&
        cadastral.is_none_or(|c|!c.visible) &&
        bgt.iter().all(|l|!l.visible)
    {
        return Err("Geen objecten, proefsleuven of kaartlagen om te exporteren".into());
    }
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
    for trench in trenches {
        let boundary=trench.measured_boundary();
        if boundary.len()<4 { continue; }
        let _=write!(out,"0\nPOLYLINE\n8\nVIRTUAL_TRENCH_3D\n62\n7\n66\n1\n70\n9\n");
        for (index,(x,y)) in boundary.iter().enumerate().take(boundary.len()-1) {
            let z=trench.boundary_3d.get(index).and_then(|v|v.z).unwrap_or(0.0);
            let _=write!(out,
                "0\nVERTEX\n8\nVIRTUAL_TRENCH_3D\n10\n{:.8}\n20\n{:.8}\n30\n{:.8}\n70\n32\n",
                x,y,z);
        }
        out.push_str("0\nSEQEND\n8\nVIRTUAL_TRENCH_3D\n");
        segment_count += boundary.len();
        write_line(&mut out,"VIRTUAL_TRENCH_AXIS",30,&trench.start,&trench.end);
        segment_count += 1;
        let label=trench.label.chars().filter(|c|
            c.is_alphanumeric()||" _.-".contains(*c)).take(100).collect::<String>();
        if !label.is_empty(){
            let _=write!(out,
                "0\nTEXT\n8\nVIRTUAL_TRENCH_LABEL\n10\n{:.8}\n20\n{:.8}\n30\n0.0\n40\n0.3\n1\n{}\n",
                trench.start.x,trench.start.y,label);
        }
    }
    if let Some(cad)=cadastral.filter(|c|c.visible) {
        for collection in &cad.linework {
            let layer=collection.layer_name.chars()
                .filter(|c|c.is_ascii_alphanumeric()||*c=='_')
                .take(30).collect::<String>();
            if layer.is_empty(){continue;}
            for line in &collection.paths {
                for pair in line.windows(2) {
                    let (x0,y0)=pair[0];
                    let (x1,y1)=pair[1];
                    if [x0,y0,x1,y1].iter().any(|v|!v.is_finite()){continue;}
                    write_line(&mut out,&layer,7,
                        &Vertex{x:x0,y:y0,z:None},
                        &Vertex{x:x1,y:y1,z:None});
                    segment_count+=1;
                }
            }
        }
        for label in &cad.labels {
            let layer=label.layer_name.chars()
                .filter(|c|c.is_ascii_alphanumeric()||*c=='_')
                .take(30).collect::<String>();
            let value=label.text.chars()
                .filter(|c|!c.is_control() && *c!='\\')
                .take(200).collect::<String>();
            if layer.is_empty() || value.is_empty() ||
                !label.position.0.is_finite() || !label.position.1.is_finite() ||
                !label.rotation.is_finite(){continue;}
            let _=write!(out,"0\nTEXT\n8\n{}\n10\n{:.8}\n20\n{:.8}\n30\n0\n40\n0.6\n50\n{:.6}\n1\n{}\n",
                layer,label.position.0,label.position.1,label.rotation,value);
            segment_count+=1;
        }
    }
    for collection in bgt.iter().filter(|c|c.visible) {
        let (layer,color)=match collection.kind {
            BgtKind::Wegdeel=>("BGT_WEGDEEL",8u16),
            BgtKind::OnbegroeidTerreindeel=>("BGT_TERREIN",3u16)
        };
        for path in &collection.paths{
            for pair in path.windows(2){
                let (x0,y0)=pair[0];let(x1,y1)=pair[1];
                if [x0,y0,x1,y1].iter().any(|v|!v.is_finite()){continue;}
                write_line(&mut out,layer,color,
                    &Vertex{x:x0,y:y0,z:None},
                    &Vertex{x:x1,y:y1,z:None});
                segment_count+=1;
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
    fn exports_pdok_cadastral_text_and_bgt_layers() {
        let dir=tempfile::tempdir().unwrap();
        let path=dir.path().join("cadastral.dxf");
        let bounds=crate::model::Bounds{
            min_x:168000.,min_y:401000.,max_x:168200.,max_y:401200.
        };
        let cadastral=CadastralLayer{
            visible:true,bounds,
            linework:vec![crate::cadastral::CadLinework{
                layer_name:"KAD_PERCEEL".into(),
                paths:vec![vec![(168000.,401000.),(168001.,401000.)]]
            }],
            labels:vec![crate::cadastral::CadLabel{
                layer_name:"KAD_STRAATNAAM".into(),
                text:"Nieuwe straat".into(),
                position:(168000.5,401000.2),rotation:30.0
            }]
        };
        let bgt=BgtLayer{
            kind:BgtKind::Wegdeel,visible:true,bounds,
            paths:vec![vec![(168000.,401000.),(168002.,401002.)]]
        };
        write_dxf_with_layers(&path,&[],&[],Some(&cadastral),&[bgt],RULES).unwrap();
        let text=fs::read_to_string(path).unwrap();
        assert!(text.contains("8\nKAD_PERCEEL\n"));
        assert!(text.contains("8\nBGT_WEGDEEL\n"));
        assert!(text.contains("1\nNieuwe straat\n"));
        assert!(text.contains("50\n30.000000\n"));
    }
    #[test]
    fn exports_measured_marxact_3d_polygon() {
        let dir=tempfile::tempdir().unwrap();
        let path=dir.path().join("boundary.dxf");
        let mut trench=VirtualTrench::new(
            "PS_123".to_owned(),
            Vertex{x:0.,y:0.5,z:Some(10.)},
            Vertex{x:4.,y:0.5,z:Some(11.)},
            1.0
        ).unwrap();
        trench.boundary_3d=vec![
            Vertex{x:0.,y:0.,z:Some(10.)},
            Vertex{x:4.,y:0.,z:Some(11.)},
            Vertex{x:4.,y:1.,z:Some(12.)},
            Vertex{x:0.,y:1.,z:Some(13.)}
        ];
        write_dxf_with_trenches(&path,&[],&[trench],RULES).unwrap();
        let text=fs::read_to_string(&path).unwrap();
        assert!(text.contains("VIRTUAL_TRENCH_3D"));
        assert!(text.contains("70\n9\n"));
        let parsed=crate::dxf_read::parse_dxf(&text,&path).unwrap();
        let boundary=parsed.features.iter().find(|f|f.entity_type=="POLYLINE").unwrap();
        assert_eq!(boundary.points.len(),5);
        assert_eq!(boundary.points[3].z,Some(13.0));
        assert_eq!(boundary.points[0].x,boundary.points[4].x);
        assert!(write_dxf_with_trenches(&path,&[],&[],RULES).is_err());
    }
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
