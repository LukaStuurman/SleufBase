//! Dense differential geometry, source persistence and bounded failure paths.
use serde::Deserialize;
use sleufbase_rust::{
    cad_geometry::{self as g, Affine, V3},
    dxf_read::{load_dxf, parse_dxf, DxfOverlay},
    project::{read_project, save_project, NativeProject},
    spline::Spline,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Deserialize)]
struct Expected {
    sources: BTreeMap<String, Spline>,
    draws: Vec<Draw>,
}
#[derive(Deserialize)]
struct Draw {
    id: String,
    path: String,
    paths: Vec<Vec<V3>>,
}
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/splines")
}
fn expected() -> Expected {
    serde_json::from_str(include_str!("fixtures/splines/expected.json")).unwrap()
}
fn distance(p: V3, a: V3, b: V3) -> f64 {
    let d = g::sub(b, a);
    let square = g::dot(d, d);
    let t = if square == 0. {
        0.
    } else {
        (g::dot(g::sub(p, a), d) / square).clamp(0., 1.)
    };
    g::length(g::sub(p, g::add(a, g::scale(d, t))))
}
fn close(a: V3, b: V3) {
    assert!(g::length(g::sub(a, b)) < 1e-7, "{a:?} != {b:?}");
}

fn verify(overlay: &DxfOverlay) {
    let expected = expected();
    let mut draws: BTreeMap<_, _> = expected
        .draws
        .iter()
        .map(|d| ((d.id.clone(), d.path.clone()), d))
        .collect();
    assert_eq!(draws.len(), 60);
    assert!(overlay.warnings.is_empty(), "{:?}", overlay.warnings);
    assert!(
        overlay.block_warnings.is_empty(),
        "{:?}",
        overlay.block_warnings
    );
    for index in 0..overlay.features.len() + overlay.expanded.features.len() {
        let actual = overlay.inspect_feature(index).unwrap();
        if actual.source.entity_type != "SPLINE" {
            continue;
        }
        let details = overlay.inspect_details(index).unwrap();
        let id = &actual.source.cad.as_ref().unwrap().xdata["SOURCE_ID"][0].value;
        assert_eq!(
            serde_json::to_value(actual.source.cad.as_ref().unwrap().spline.as_ref().unwrap())
                .unwrap(),
            serde_json::to_value(&expected.sources[id]).unwrap(),
            "{id}: original control/fit parameters changed"
        );
        let path = details.get("Blokpad").cloned().unwrap_or_default();
        let target = draws.remove(&(id.clone(), path)).unwrap();
        let paths: Vec<_> = actual.paths().collect();
        assert_eq!(paths.len(), target.paths.len(), "{id}");
        for (actual, reference) in paths.iter().zip(&target.paths) {
            close(g::xyz(actual[0]), reference[0]);
            close(g::xyz(*actual.last().unwrap()), *reference.last().unwrap());
            for &p in reference {
                let error = actual
                    .windows(2)
                    .map(|edge| distance(p, g::xyz(edge[0]), g::xyz(edge[1])))
                    .fold(f64::INFINITY, f64::min);
                assert!(
                    error <= 0.2500001,
                    "{id}: world curve error {error} exceeds 0.25"
                );
            }
        }
    }
    assert!(draws.is_empty());
}

#[test]
fn control_rational_periodic_fit_and_nested_world_splines_match_ezdxf_in_both_formats() {
    for file in ["splines-ascii.dxf", "splines-binary.dxf"] {
        verify(&load_dxf(&root().join(file)).unwrap());
    }
}

#[test]
fn native_project_retains_source_parameters_and_rebuilds_disconnected_curve_caches() {
    let overlay = load_dxf(&root().join("splines-binary.dxf")).unwrap();
    let before = serde_json::to_value(&overlay).unwrap();
    let snapshot = overlay.clone();
    assert!(Arc::ptr_eq(&overlay.expanded, &snapshot.expanded));
    assert!(Arc::ptr_eq(
        &overlay.blocks["curves"],
        &snapshot.blocks["curves"]
    ));
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("curves.json");
    save_project(
        &path,
        &NativeProject {
            overlays: vec![overlay],
            ..Default::default()
        },
    )
    .unwrap();
    let loaded = read_project(&path).unwrap();
    assert_eq!(serde_json::to_value(&loaded.overlays[0]).unwrap(), before);
    verify(&loaded.overlays[0]);
    assert_eq!(
        loaded.overlays[0]
            .rendered_features()
            .filter(|f| f.paths().count() == 2)
            .count(),
        3
    );
}

#[test]
fn loops_and_discontinuities_are_not_replaced_by_invented_chords() {
    let expected = expected();
    let looped = expected.sources["loop"]
        .flatten(Affine::IDENTITY, 0.25, 10000)
        .unwrap();
    assert!(looped.points.len() > 4);
    assert!(looped.points.iter().any(|p| p.y.abs() > 5.));
    let disconnected = expected.sources["disconnected"]
        .flatten(Affine::IDENTITY, 0.25, 10000)
        .unwrap();
    assert_eq!(disconnected.breaks.len(), 1);
    let split = disconnected.breaks[0];
    assert!(disconnected.points[split - 1].x < 3. && disconnected.points[split].x >= 20.);
    let mut invalid = NativeProject::default();
    let mut overlay = load_dxf(&root().join("splines-ascii.dxf")).unwrap();
    overlay.features[0].cad.as_mut().unwrap().path_breaks = vec![usize::MAX];
    invalid.overlays.push(overlay);
    assert!(invalid.validate().unwrap_err().contains("padonderbreking"));
}

#[test]
fn malformed_spline_counts_knots_weights_and_numeric_work_are_rejected() {
    let valid = expected().sources["cubic"].clone();
    let mut invalid = valid.clone();
    invalid.knots.pop();
    assert!(invalid.validate().is_err());
    let mut invalid = valid.clone();
    invalid.knots.swap(0, 5);
    assert!(invalid.validate().is_err());
    let mut invalid = valid.clone();
    invalid.weights = vec![-1.; invalid.controls.len()];
    assert!(invalid.validate().is_err());
    let mut invalid = valid.clone();
    invalid.weights = vec![1.];
    assert!(invalid.validate().is_err());
    let mut invalid = valid.clone();
    invalid.controls[0][2] = f64::NAN;
    assert!(invalid.validate().is_err());
    let mut invalid = valid.clone();
    invalid.degree = 32;
    invalid.knots.clear();
    invalid.controls = vec![[0.; 3]; 3000];
    assert!(invalid
        .validate()
        .unwrap_err()
        .contains("berekeningsbudget"));
    assert!(valid.flatten(Affine::IDENTITY, 0.25, 1).is_err());
    assert!(valid.flatten(Affine::IDENTITY, f64::NAN, 100).is_err());
    let mut fit = expected().sources["natural-fit"].clone();
    fit.fit_points[1] = fit.fit_points[0];
    assert!(fit.flatten(Affine::IDENTITY, 0.25, 100).is_err());
    let text = "0\nSECTION\n2\nENTITIES\n0\nSPLINE\n71\n1\n72\n0\n73\n3\n10\n0\n20\n0\n10\n1\n20\n1\n0\nENDSEC\n0\nEOF\n";
    assert!(parse_dxf(text, Path::new("count.dxf"))
        .unwrap_err()
        .contains("aantallen"));
}

#[test]
fn twenty_thousand_fit_points_use_the_bounded_solver_and_exact_output_budget() {
    let mut source = expected().sources["natural-fit"].clone();
    source.fit_points = (0..20_000)
        .map(|i| [168000. + i as f64, 401000., 3.])
        .collect();
    let output = source
        .flatten(Affine::IDENTITY, 0.25, source.fit_points.len())
        .unwrap();
    assert_eq!(output.points.len(), source.fit_points.len());
    assert!(output.breaks.is_empty());
    for (actual, expected) in output.points.iter().zip(&source.fit_points) {
        close(g::xyz(*actual), *expected);
    }
    assert!(source
        .flatten(Affine::IDENTITY, 0.25, source.fit_points.len() - 1)
        .is_err());
}
