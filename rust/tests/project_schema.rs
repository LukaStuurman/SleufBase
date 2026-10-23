//! Persisted object contracts must fail closed on unknown fields, preserving
//! unsupported input rather than loading/saving it with silent field loss.
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use sleufbase_rust::{
    bgt::{BgtKind, BgtLayer},
    cadastral::{CadLabel, CadLinework, CadastralLayer},
    geo::GeoTransform,
    geotiff::read_geotiff_metadata,
    legacy_project::LegacyArchive,
    marxact_alignment::MarXactAlignment,
    model::*,
    project::*,
    virtual_trench::*,
};
use std::{path::Path, sync::Arc};

fn unknown<T: Serialize + DeserializeOwned>(source: T) {
    let mut value = serde_json::to_value(source).unwrap();
    assert!(serde_json::from_value::<T>(value.clone()).is_ok());
    value["future_field"] = json!({"must_be_preserved":true});
    assert!(
        serde_json::from_value::<T>(value).is_err(),
        "{} silently dropped future fields",
        std::any::type_name::<T>()
    );
}

#[test]
fn every_native_project_object_rejects_unknown_fields() {
    let p = Vertex {
        x: 1.,
        y: 2.,
        z: Some(3.),
    };
    let q = Vertex {
        x: 4.,
        y: 5.,
        z: Some(6.),
    };
    let bounds = Bounds {
        min_x: 0.,
        min_y: 0.,
        max_x: 10.,
        max_y: 10.,
    };
    unknown(p);
    unknown(bounds);
    let feature = Feature {
        object_name: "water".into(),
        source_name: "Water".into(),
        geometry: Geometry::Point(p),
        attributes: Default::default(),
    };
    unknown(feature.clone());
    unknown(Dataset {
        job_id: 1,
        title: "PS1".into(),
        source_path: "job.json".into(),
        features: vec![feature],
    });
    unknown(GeoTransform {
        a: 1.,
        b: 0.,
        c: 0.,
        d: 0.,
        e: -1.,
        f: 0.,
    });
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/gray16.tiff");
    unknown(read_geotiff_metadata(&path).unwrap());
    unknown(VirtualTrench::new("PS1".into(), p, q, 1.).unwrap());
    unknown(TrenchPoint {metadata:Default::default(),
        role: "object".into(),
        object_name: "water".into(),
        source_name: "water".into(),
        position: p,
        attributes: Default::default(),
    });
    unknown(MarXactAlignment::new(p, q, 1.));
    unknown(BgtLayer {
        kind: BgtKind::Wegdeel,
        visible: true,
        bounds,
        paths: vec![],
    });
    unknown(CadLinework {
        layer_name: "line".into(),
        paths: vec![],
    });
    unknown(CadLabel {
        layer_name: "label".into(),
        text: "Road".into(),
        position: (1., 2.),
        rotation: 0.,
    });
    unknown(CadastralLayer {
        visible: true,
        bounds,
        linework: vec![],
        labels: vec![],
    });
    unknown(Comment {
        x: 1.,
        y: 2.,
        text: "note".into(),
    });
    unknown(LegacyArchive {
        source_path: "old.klicproject".into(),
        original: Arc::new(json!({"version":1})),
        notices: vec![],
    });
    unknown(NativeProject::default());
    use sleufbase_rust::cad_geometry::{ArcCurve, CurvePath, Placement};
    unknown(Placement {
        insert: [0.; 3],
        scale: [1.; 3],
        rotation: 0.,
        normal: [0., 0., 1.],
        rows: 1,
        columns: 1,
        row_spacing: 0.,
        column_spacing: 0.,
    });
    unknown(CurvePath { segments: vec![] });
    unknown(ArcCurve {
        origin: [0.; 3],
        u: [1., 0., 0.],
        v: [0., 1., 0.],
        sweep: 1.,
    });
    unknown(sleufbase_rust::cad_scene::Block {
        name: "B".into(),
        base: [0.; 3],
        features: vec![],
    });
    unknown(sleufbase_rust::spline::Spline {
        degree: 1,
        flags: 0,
        knots: vec![],
        controls: vec![[0.; 3], [1.; 3]],
        weights: vec![],
        fit_points: vec![],
        start_tangent: None,
        end_tangent: None,
        normal: None,
        knot_tolerance: 1e-7,
        control_tolerance: 1e-7,
        fit_tolerance: 1e-10,
    });
}

#[test]
fn original_python_json_remains_an_open_lossless_archive_inside_strict_native_schema() {
    let original =
        json!({"version":1,"future_python_data":{"anything":[1,2,3],"metadata":"preserve"}});
    let project = NativeProject {
        legacy_archive: Some(LegacyArchive {
            source_path: "old.klicproject".into(),
            original: Arc::new(original.clone()),
            notices: vec![],
        }),
        ..Default::default()
    };
    let value: Value = serde_json::to_value(&project).unwrap();
    let decoded: NativeProject = serde_json::from_value(value).unwrap();
    decoded.validate().unwrap();
    assert_eq!(decoded.legacy_archive.unwrap().original.as_ref(), &original);
}
