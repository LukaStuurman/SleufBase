use serde::Deserialize;
use sleufbase_rust::{
    cad_mesh::{Mesh, Source},
    dxf_read::{load_dxf, parse_dxf, DxfOverlay},
    project::{read_project, save_project, NativeProject},
    Vertex,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
type Edge = [[f64; 3]; 2];
#[derive(Deserialize)]
struct Case {
    id: String,
    nested: bool,
    edges: Vec<Edge>,
    #[serde(default)]
    first_face: Vec<[f64; 3]>,
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/meshes")
}
fn xyz(p: Vertex) -> [f64; 3] {
    [p.x, p.y, p.z.unwrap_or(0.)]
}
fn sort(edges: &mut [Edge]) {
    for edge in edges.iter_mut() {
        if edge[0].partial_cmp(&edge[1]).unwrap().is_gt() {
            edge.swap(0, 1);
        }
    }
    edges.sort_by(|a, b| a.partial_cmp(b).unwrap());
}
fn verify(overlay: &DxfOverlay) {
    let expected: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/meshes/expected.json")).unwrap();
    assert_eq!(expected.len(), 56);
    let mut cases: BTreeMap<_, _> = expected
        .into_iter()
        .map(|v| ((v.id.clone(), v.nested), v))
        .collect();
    for (index, draw) in overlay.rendered_features().enumerate() {
        let id = &draw.source.cad.as_ref().unwrap().xdata["SOURCE_ID"][0].value;
        let mut case = cases.remove(&(id.clone(), index >= 28)).unwrap();
        if !case.first_face.is_empty() {
            let mesh = draw.source.cad.as_ref().unwrap().mesh.as_ref().unwrap();
            let actual: Vec<_> = mesh.faces[0]
                .iter()
                .map(|i| xyz(mesh.vertices[i.unsigned_abs() as usize - 1]))
                .collect();
            assert_eq!(
                actual, case.first_face,
                "polygon-grid winding must match ezdxf"
            );
        }
        let mut actual = Vec::new();
        for path in draw.paths() {
            if path.is_empty() {
                continue;
            }
            assert_eq!(path.len(), 2, "each visible edge is independent: {id}");
            actual.push([xyz(path[0]), xyz(path[1])]);
        }
        sort(&mut actual);
        sort(&mut case.edges);
        assert_eq!(actual.len(), case.edges.len(), "{id}");
        for (a, b) in actual
            .iter()
            .flatten()
            .flatten()
            .zip(case.edges.iter().flatten().flatten())
        {
            assert!((a - b).abs() < 1e-7, "{id}: {a} != {b}");
        }
        assert!(draw.source.cad.as_ref().unwrap().mesh.is_some());
    }
    assert!(cases.is_empty());
}
#[test]
fn ascii_binary_surfaces_hidden_edges_ocs_polymesh_polyface_and_ngons_match_ezdxf() {
    for name in ["meshes-ascii.dxf", "meshes-binary.dxf"] {
        let overlay = load_dxf(&root().join(name)).unwrap();
        verify(&overlay);
        assert!(overlay.warnings.iter().any(|w| w.contains("subdivisie")));
        assert!(overlay
            .warnings
            .iter()
            .any(|w| w.contains("dikte-extrusie")));
        assert!(overlay
            .warnings
            .iter()
            .any(|w| w.contains("glad oppervlak")));
        assert!(overlay.block_warnings.is_empty());
        let mut base = load_dxf(
            &root()
                .parent()
                .unwrap()
                .join("marxact-cad/nested-leaders.dxf"),
        )
        .unwrap();
        let before = sleufbase_rust::marxact::parse_overlay(&base, &BTreeMap::new()).unwrap();
        base.features.extend(
            overlay
                .features
                .iter()
                .filter(|f| f.entity_type == "POLYLINE")
                .cloned(),
        );
        let after = sleufbase_rust::marxact::parse_overlay(&base, &BTreeMap::new()).unwrap();
        assert_eq!(
            serde_json::to_value(before.trenches).unwrap(),
            serde_json::to_value(after.trenches).unwrap()
        );
    }
}
#[test]
fn indexed_sources_and_separate_edges_survive_storage_and_snapshots_share_meshes() {
    let overlay = load_dxf(&root().join("meshes-binary.dxf")).unwrap();
    let snapshot = overlay.clone();
    assert!(Arc::ptr_eq(
        overlay.features[0]
            .cad
            .as_ref()
            .unwrap()
            .mesh
            .as_ref()
            .unwrap(),
        snapshot.features[0]
            .cad
            .as_ref()
            .unwrap()
            .mesh
            .as_ref()
            .unwrap()
    ));
    let expected = serde_json::to_value(&overlay).unwrap();
    let project = NativeProject {
        overlays: vec![overlay],
        ..Default::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("meshes.json");
    save_project(&path, &project).unwrap();
    let restored = read_project(&path).unwrap();
    verify(&restored.overlays[0]);
    assert_eq!(
        serde_json::to_value(&restored.overlays[0]).unwrap(),
        expected
    );
    let mesh = restored.overlays[0]
        .features
        .iter()
        .find(|f| f.entity_type == "MESH")
        .unwrap()
        .cad
        .as_ref()
        .unwrap()
        .mesh
        .as_ref()
        .unwrap();
    assert_eq!(mesh.creases, vec![0.5, -1.]);
    assert_eq!(mesh.edges, vec![[0, 5], [2, 3]]);
    let mesh = restored.overlays[0]
        .features
        .iter()
        .find(|f| {
            f.entity_type == "POLYLINE"
                && f.cad
                    .as_ref()
                    .unwrap()
                    .mesh
                    .as_ref()
                    .is_some_and(|m| matches!(m.source, Source::Polyface { .. }))
        })
        .unwrap()
        .cad
        .as_ref()
        .unwrap()
        .mesh
        .as_ref()
        .unwrap();
    assert!(mesh
        .vertex_metadata
        .values()
        .flatten()
        .any(|t| t.code == 420 && t.value == "0"));
    assert!(mesh
        .face_metadata
        .values()
        .flatten()
        .any(|t| t.code == 1000 && t.value == "face-metadata"));
}

#[test]
fn native_rebuilds_stale_mesh_edges_and_invalid_source_never_replaces_a_saved_project() {
    let overlay = load_dxf(&root().join("meshes-binary.dxf")).unwrap();
    let mut project = NativeProject {
        overlays: vec![overlay],
        ..Default::default()
    };
    let mut value = serde_json::to_value(&project).unwrap();
    value["overlays"][0]["features"][0]["points"] =
        serde_json::json!([{"x":999.0,"y":999.0,"z":0.0}]);
    value["overlays"][0]["features"][0]["cad"]["path_breaks"] = serde_json::json!([]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stale.json");
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    let source = std::fs::read(&path).unwrap();
    verify(&read_project(&path).unwrap().overlays[0]);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        source,
        "opening must not rewrite source"
    );
    let cad = project.overlays[0].features[0].cad.as_mut().unwrap();
    Arc::make_mut(cad.mesh.as_mut().unwrap()).faces[0][0] = i32::MIN;
    let mut mesh_value = serde_json::to_value(cad.mesh.as_ref().unwrap()).unwrap();
    assert!(save_project(&path, &project).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), source);
    mesh_value["future"] = serde_json::json!(true);
    assert!(serde_json::from_value::<Mesh>(mesh_value).is_err());
    assert!(serde_json::from_str::<Source>(
        r#"{"Solid":{"normal":[0,0,1],"thickness":1.5,"future":1}}"#
    )
    .is_err());
}

#[test]
fn shared_grid_edges_are_deduplicated_and_disconnected_without_per_frame_work() {
    let n = 100usize;
    let mut mesh = Mesh {
        source: Source::Polygon {
            flags: 16,
            size: [n as u32, n as u32],
            density: [0, 0],
            smooth_type: 0,
        },
        vertices: vec![],
        faces: vec![],
        edges: vec![],
        creases: vec![],
        subdivision: 0,
        vertex_metadata: Default::default(),
        face_metadata: Default::default(),
    };
    for a in 0..n {
        for b in 0..n {
            mesh.vertices.push(Vertex {
                x: a as f64,
                y: b as f64,
                z: Some((a + b) as f64),
            });
        }
    }
    for a in 0..n - 1 {
        for b in 0..n - 1 {
            mesh.faces.push(
                [
                    a * n + b,
                    (a + 1) * n + b,
                    (a + 1) * n + b + 1,
                    a * n + b + 1,
                ]
                .map(|i| (i + 1) as i32)
                .to_vec(),
            );
        }
    }
    let count = 2 * n * (n - 1);
    let (points, breaks) = mesh.wireframe(2 * count).unwrap();
    assert_eq!(points.len(), 2 * count);
    assert_eq!(breaks.len(), count - 1);
    assert!(breaks.iter().enumerate().all(|(i, b)| *b == 2 * (i + 1)));
    assert!(mesh.wireframe(2 * count - 1).is_err());
    // Shared storage keeps this regression small while exercising the complete
    // source count, including unused meshes with no INSERT instances.
    let mut feature = load_dxf(&root().join("meshes-binary.dxf"))
        .unwrap()
        .features
        .remove(0);
    feature.points.clear();
    let cad = feature.cad.as_mut().unwrap();
    cad.path_breaks.clear();
    cad.mesh = Some(Arc::new(mesh));
    let mut overlay = DxfOverlay {
        features: vec![feature; 1001],
        ..Default::default()
    };
    assert!(overlay
        .rebuild_block_cache()
        .unwrap_err()
        .contains("budget"));
}
#[test]
fn edge_budget_and_invalid_indices_fail_before_partial_geometry_is_returned() {
    let v = |x, y| Vertex { x, y, z: Some(0.) };
    let mut mesh = Mesh {
        source: Source::Polyface {
            flags: 64,
            declared: [0, 0],
        },
        vertices: vec![v(0., 0.), v(1., 0.), v(1., 1.)],
        faces: vec![vec![1, 2, 3]],
        edges: vec![],
        creases: vec![],
        subdivision: 0,
        vertex_metadata: Default::default(),
        face_metadata: Default::default(),
    };
    assert_eq!(mesh.wireframe(6).unwrap().0.len(), 6);
    assert!(mesh.wireframe(5).unwrap_err().contains("budget"));
    for index in [0, 4, i32::MIN] {
        mesh.faces[0][0] = index;
        assert!(mesh.validate().is_err());
    }
    mesh.faces = vec![vec![-1, -2, -3]];
    assert!(mesh.wireframe(0).unwrap().0.is_empty());
    mesh.faces = vec![vec![1, 2, 3], vec![3, 2, 1]];
    assert_eq!(mesh.wireframe(6).unwrap().0.len(), 6);
    mesh.edges = vec![[0, 8]];
    assert!(mesh.validate().is_err());
}
fn wrap(entity: &str) -> String {
    format!("0\nSECTION\n2\nENTITIES\n{entity}0\nENDSEC\n0\nEOF\n")
}
#[test]
fn mesh_counts_face_lengths_and_polygon_dimensions_are_strict_but_polyface_headers_are_advisory() {
    let valid="0\nPOLYLINE\n70\n64\n71\n999\n72\n999\n0\nVERTEX\n70\n128\n71\n1\n72\n2\n73\n3\n0\nVERTEX\n70\n192\n10\n0\n20\n0\n0\nVERTEX\n70\n192\n10\n1\n20\n0\n0\nVERTEX\n70\n192\n10\n1\n20\n1\n0\nSEQEND\n0\nLINE\n10\n1\n20\n2\n11\n3\n21\n4\n";
    let overlay = parse_dxf(&wrap(valid), Path::new("mixed.dxf")).unwrap();
    assert_eq!(overlay.features.len(), 2);
    assert_eq!(overlay.features[0].points.len(), 6);
    assert_eq!(overlay.features[1].entity_type, "LINE");
    assert!(parse_dxf(
        &wrap(&valid.replace("73\n3", "73\n4")),
        Path::new("invalid.dxf")
    )
    .is_err());
    assert!(parse_dxf(
        &wrap("0\nPOLYLINE\n70\n16\n71\n2\n72\n2\n0\nSEQEND\n"),
        Path::new("invalid.dxf")
    )
    .is_err());
    let text = std::fs::read_to_string(root().join("meshes-ascii.dxf"))
        .unwrap()
        .replace("\r\n", "\n");
    // Mutate list counts in a real serialized MESH rather than constructing a native-only shape.
    let start = text.find("\nMESH\n").unwrap();
    let end = start
        + text[start..]
            .find("\nENDSEC\n")
            .unwrap_or(text.len() - start);
    let part = &text[start..end];
    let needle = " 93\n10\n";
    assert!(part.contains(needle));
    assert!(parse_dxf(
        &text.replacen(needle, " 93\n1\n", 1),
        Path::new("invalid.dxf")
    )
    .is_err());
}
