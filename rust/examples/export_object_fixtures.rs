//! Produce documents for an independent ezdxf audit; no Python at runtime.
use sleufbase_rust::{object_dxf, Dataset, RULES};
#[derive(serde::Deserialize)]
struct Case {
    name: String,
    datasets: Vec<Dataset>,
    options: object_dxf::Options,
}
fn main() -> Result<(), String> {
    let dir = std::env::args_os()
        .nth(1)
        .ok_or("Usage: export_object_fixtures output-directory")?;
    let dir = std::path::Path::new(&dir);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    sleufbase_rust::settings_compat::save_rust_settings(
        &dir.join("cli-settings.json"),
        &Default::default(),
    )?;
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("../tests/fixtures/object-export/cases.json"))
            .map_err(|e| e.to_string())?;
    for case in cases {
        object_dxf::write(
            &dir.join(format!("{}.dxf", case.name)),
            &case.datasets,
            RULES,
            case.options,
        )?;
    }
    use sleufbase_rust::{
        bgt::{BgtKind, BgtLayer},
        cadastral::{CadLabel, CadLinework, CadastralLayer},
        virtual_trench::{TrenchPoint, VirtualTrench},
        Bounds, Vertex,
    };
    let vertex = |x, y, z| Vertex { x, y, z: Some(z) };
    let mut trench =
        VirtualTrench::new("PS9".into(), vertex(0., 0.5, 10.), vertex(4., 0.5, 11.), 1.)?;
    trench.boundary_3d = vec![
        vertex(0., 0., 10.),
        vertex(4., 0., 11.),
        vertex(4., 1., 12.),
        vertex(0., 1., 13.),
    ];
    trench.add_object(TrenchPoint {
        role: "object".into(),
        object_name: "Water".into(),
        source_name: "water".into(),
        position: vertex(2., 0.5, 12.),
        attributes: Default::default(),
    })?;
    let bounds = Bounds {
        min_x: 168000.,
        min_y: 401000.,
        max_x: 168200.,
        max_y: 401200.,
    };
    let cad = CadastralLayer {
        visible: true,
        bounds,
        linework: vec![CadLinework {
            layer_name: "KADASTRALE_GRENZEN_MET_LANGE_NAAM_É".into(),
            paths: vec![vec![(168000., 401000.), (168001., 401000.)]],
        }],
        labels: vec![CadLabel {
            layer_name: "KAD_STRAATNAAM".into(),
            text: "Straatnaam É".into(),
            position: (168000.5, 401000.2),
            rotation: 30.,
        }],
    };
    let bgt = BgtLayer {
        kind: BgtKind::Wegdeel,
        visible: true,
        bounds,
        paths: vec![vec![(168000., 401000.), (168002., 401002.)]],
    };
    sleufbase_rust::write_dxf_with_layers(
        &dir.join("extras.dxf"),
        &[],
        &[trench],
        Some(&cad),
        &[bgt],
        RULES,
    )?;
    Ok(())
}
