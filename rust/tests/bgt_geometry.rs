use sleufbase_rust::{
    bgt_features, bgt_geometry,
    bgt_surface::{Feature, Geometry, Index},
    Bounds,
};
use std::sync::atomic::AtomicBool;

#[derive(serde::Deserialize)]
struct Probe {
    start: [f64; 2],
    end: [f64; 2],
    expected: Option<String>,
}
#[derive(serde::Deserialize)]
struct Case {
    name: String,
    parts: Vec<Geometry>,
    bounds: Bounds,
    expected: Option<Geometry>,
    area: f64,
    length: f64,
    probes: Vec<Probe>,
}
fn metrics(g: &Geometry) -> (f64, f64) {
    fn length(p: &[[f64; 2]]) -> f64 {
        p.windows(2)
            .map(|s| (s[1][0] - s[0][0]).hypot(s[1][1] - s[0][1]))
            .sum()
    }
    match g {
        Geometry::Point(_) | Geometry::MultiPoint(_) => (0., 0.),
        Geometry::LineString(p) => (0., length(p)),
        Geometry::MultiLineString(p) => (0., p.iter().map(|p| length(p)).sum()),
        Geometry::Polygon(rings) => {
            let mut area = 0.;
            let mut perimeter = 0.;
            for (i, p) in rings.iter().enumerate() {
                let Some(o) = p.first() else {
                    continue;
                };
                let a = p
                    .windows(2)
                    .map(|s| {
                        (s[0][0] - o[0]) * (s[1][1] - o[1]) - (s[1][0] - o[0]) * (s[0][1] - o[1])
                    })
                    .sum::<f64>()
                    .abs()
                    * 0.5;
                area += if i == 0 { a } else { -a };
                perimeter += length(p);
            }
            (area, perimeter)
        }
        Geometry::MultiPolygon(p) => p
            .iter()
            .map(|p| metrics(&Geometry::Polygon(p.clone())))
            .fold((0., 0.), |(a, l), (aa, ll)| (a + aa, l + ll)),
        Geometry::Collection(p) => p
            .iter()
            .map(metrics)
            .fold((0., 0.), |(a, l), (aa, ll)| (a + aa, l + ll)),
    }
}
#[test]
fn geos_repair_union_clipping_metrics_and_boundary_votes() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/bgt-geometry/reference.json")).unwrap();
    let cancel = AtomicBool::new(false);
    let mut failures = Vec::new();
    for c in cases {
        let refs = c.parts.iter().collect::<Vec<_>>();
        let result = bgt_geometry::normalize(&refs, c.bounds, &cancel)
            .unwrap_or_else(|e| panic!("{}: {e}", c.name));
        assert_eq!(result.is_none(), c.expected.is_none(), "{}", c.name);
        let (a, l) = result.as_ref().map(metrics).unwrap_or((0., 0.));
        let tolerance = 2e-8 * c.length.max(1.);
        assert!(
            (a - c.area).abs() <= tolerance,
            "{}: area {a}/{}",
            c.name,
            c.area
        );
        assert!(
            (l - c.length).abs() <= tolerance,
            "{}: length {l}/{}",
            c.name,
            c.length
        );
        let features = result
            .map(|geometry| Feature {
                physical_appearance: "Asfalt".into(),
                geometry,
            })
            .into_iter()
            .collect::<Vec<_>>();
        let index = Index::from_normalized_features(features.iter(), &cancel).unwrap();
        for p in c.probes {
            let actual = index.dominant(p.start, p.end, &cancel).unwrap();
            if actual != p.expected.as_deref() {
                failures.push(format!(
                    "{}: {:?}/{:?}: {:?}/{:?}",
                    c.name, p.start, p.end, actual, p.expected
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatches: {:?}",
        failures.len(),
        failures
    );
}
#[test]
fn original_decimal_rounding_and_reversed_path_deduplication() {
    #[derive(serde::Deserialize)]
    struct Input {
        paths: Vec<Vec<[f64; 2]>>,
        expected: Vec<Vec<[f64; 2]>>,
    }
    let c: Input = serde_json::from_str(include_str!("fixtures/bgt-geometry/dedupe.json")).unwrap();
    assert_eq!(
        bgt_features::dedupe(c.paths, &AtomicBool::new(false)).unwrap(),
        c.expected
    );
}
#[test]
fn cancellation_invalid_bounds_coordinates_and_recursive_limits() {
    let bounds = Bounds {
        min_x: 0.,
        min_y: 0.,
        max_x: 10.,
        max_y: 10.,
    };
    let cancel = AtomicBool::new(false);
    assert!(bgt_geometry::normalize(&[], bounds, &AtomicBool::new(true)).is_err());
    assert!(bgt_geometry::normalize(
        &[],
        Bounds {
            min_x: 11.,
            ..bounds
        },
        &cancel
    )
    .is_err());
    assert!(bgt_geometry::normalize(&[&Geometry::Point([f64::NAN, 0.])], bounds, &cancel).is_err());
    let mut g = Geometry::Point([1., 1.]);
    for _ in 0..18 {
        g = Geometry::Collection(vec![g]);
    }
    assert!(bgt_geometry::normalize(&[&g], bounds, &cancel).is_err());
}
#[test]
fn constructed_boundary_roundoff_is_local_and_strict_inputs_remain_exact() {
    let cancel = AtomicBool::new(false);
    let features = [Feature {
        physical_appearance: "Asfalt".into(),
        geometry: Geometry::LineString(vec![[120000., 450000.], [120010., 450000.]]),
    }];
    let exact = Index::new(&features, &cancel).unwrap();
    let constructed = Index::from_normalized_features(features.iter(), &cancel).unwrap();
    let y = f64::from_bits(450000f64.to_bits() + 1);
    assert_eq!(
        exact.dominant([120001., y], [120009., y], &cancel).unwrap(),
        None
    );
    assert_eq!(
        constructed
            .dominant([120001., y], [120009., y], &cancel)
            .unwrap(),
        Some("Asfalt")
    );
    assert_eq!(
        constructed
            .dominant([120001., 450000.000001], [120009., 450000.000001], &cancel)
            .unwrap(),
        None
    );
}
#[test]
fn complete_client_normalizes_cached_real_tile_before_surface_index() {
    use sleufbase_rust::{
        bgt_tile_client::{Client, Options, Transport},
        bgt_tiles::{TileId, ORIGIN_X, ORIGIN_Y, SPAN},
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    struct Recorded(Arc<AtomicUsize>);
    impl Transport for Recorded {
        fn load(&self, id: TileId, _: &AtomicBool) -> Result<Vec<u8>, String> {
            assert_eq!(
                id,
                TileId {
                    row: 2420,
                    col: 2075
                }
            );
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(include_bytes!("fixtures/bgt-tiles/pdok-12-2420-2075.pbf").to_vec())
        }
    }
    let cancel = AtomicBool::new(false);
    let calls = Arc::new(AtomicUsize::new(0));
    let client = Client::with_transport(Recorded(calls.clone()), Options::default()).unwrap();
    let x = ORIGIN_X + 2075. * SPAN;
    let y = ORIGIN_Y - 2420. * SPAN;
    let bounds = Bounds {
        min_x: x + 20.,
        min_y: y - SPAN + 20.,
        max_x: x + SPAN - 20.,
        max_y: y - 20.,
    };
    let surfaces = client.fetch_surface_features(bounds, &cancel).unwrap();
    let paths = client.fetch_paths(bounds, &cancel).unwrap();
    assert!(!surfaces.is_empty());
    assert!(!paths.is_empty());
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    for p in paths.iter().flatten() {
        assert!(
            p[0] >= bounds.min_x - 1e-8
                && p[0] <= bounds.max_x + 1e-8
                && p[1] >= bounds.min_y - 1e-8
                && p[1] <= bounds.max_y + 1e-8
        );
    }
    let index = bgt_features::surface_index(&surfaces, &cancel).unwrap();
    let _ = index
        .dominant(
            [bounds.min_x, bounds.min_y],
            [bounds.max_x, bounds.max_y],
            &cancel,
        )
        .unwrap();
    assert!(client
        .fetch_surface_features(bounds, &AtomicBool::new(true))
        .is_err());
}
