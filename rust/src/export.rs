use crate::disciplines::Rule;
use crate::model::Dataset;
use crate::virtual_trench::VirtualTrench;
use crate::cadastral::CadastralLayer;
use crate::bgt::BgtLayer;
#[cfg(test)]
use std::fs;
use std::path::Path;

/// Discipline report with original ordering, totals, layout and styling.
pub fn write_xlsx(path: &Path, datasets: &[Dataset], rules: &[Rule]) -> Result<(), String> {
    crate::discipline_xlsx::write(path,datasets.iter(),rules)
}

pub fn write_dxf(path: &Path, datasets: &[Dataset], rules: &[Rule]) -> Result<(), String> {
    write_dxf_with_trenches(path,datasets,&[],rules)
}

/// Exports a standards-compatible R2018 3D POLYLINE for each measured MarXact
/// boundary or manually created virtual trench, preserving measured elevations.
/// The DXF template/AutoCAD dynamic blocks are still separate future parity work.
pub fn write_dxf_with_trenches(path: &Path, datasets: &[Dataset],
    trenches: &[VirtualTrench],rules: &[Rule]) -> Result<(), String> {
    write_dxf_with_layers(path,datasets,trenches,None,&[],rules)
}
/// Native object export (not a template): object lines, 3D measured trench
/// boundaries, PDOK cadastral boundaries and text with measured rotation,
/// and selected BGT linework, each on separate named AutoCAD layers.
pub fn write_dxf_with_layers(path:&Path,datasets:&[Dataset],
    trenches:&[VirtualTrench],cadastral:Option<&CadastralLayer>,
    bgt:&[BgtLayer],rules:&[Rule]) -> Result<(),String> {
    crate::object_dxf::write_with_layers(path,datasets,trenches,cadastral,bgt,rules,Default::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bgt::BgtKind;
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
        assert!(text.contains("50\n30\n"));
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
    fn exports_object_polylines_and_rejects_overwrite() {
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
        assert!(contents.contains("0\nPOLYLINE\n"));
        assert!(contents.ends_with("0\nEOF\n"));
        assert!(write_dxf(&output, &[dataset], RULES).is_err());
        fs::remove_file(output).unwrap();
    }
}
