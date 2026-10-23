use serde::Deserialize;
use sleufbase_rust::{
    cross_section::{local_segment, markers, XY},
    virtual_trench::{TrenchPoint, VirtualTrench},
    Vertex,
};

#[derive(Deserialize)]
struct Case {
    polygon: Vec<XY>,
    origin: XY,
    normal: XY,
    preferred: f64,
    expected: Option<(XY, XY)>,
}
fn close(a: XY, b: XY) {
    assert!((a.0 - b.0).hypot(a.1 - b.1) < 1e-7, "{a:?} != {b:?}");
}

#[test]
fn local_spans_match_python_on_240_tapered_and_concave_cases() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/local_cross_sections.json")).unwrap();
    assert_eq!(cases.len(), 240);
    for c in cases {
        match (
            local_segment(&c.polygon, c.origin, c.normal, c.preferred),
            c.expected,
        ) {
            (Some((a, b)), Some((x, y))) => {
                close(a, x);
                close(b, y);
            }
            (None, None) => {}
            (got, wanted) => panic!("{got:?} != {wanted:?}"),
        }
    }
}

#[test]
fn markers_preserve_depth_and_adapt_to_measured_local_width() {
    let p = |x, y, z| Vertex { x, y, z: Some(z) };
    let mut t =
        VirtualTrench::new("PS1".into(), p(-2.0, 0.0, 10.0), p(2.0, 0.0, 10.0), 2.0).unwrap();
    t.boundary_3d = vec![
        p(-2.0, -1.0, 10.0),
        p(2.0, -0.3, 10.0),
        p(2.0, 0.3, 10.0),
        p(-2.0, 1.0, 10.0),
    ];
    t.objects = [-1.5, 1.5]
        .into_iter()
        .map(|x| TrenchPoint {metadata:Default::default(),
            role: "object".into(),
            object_name: "pipe".into(),
            source_name: "water".into(),
            position: p(x, 0.0, 3.12),
            attributes: Default::default(),
        })
        .collect();
    let m = markers(&t);
    assert_eq!(m.len(), 2);
    assert!(m[0].end.y - m[0].start.y > m[1].end.y - m[1].start.y + 0.8);
    assert_eq!(m[0].start.z, Some(3.12));
    assert_eq!(m[1].end.z, Some(3.12));
    assert_eq!(m[0].chainage, 0.5);
    assert_eq!(m[1].chainage, 3.5);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profile.dxf");
    sleufbase_rust::write_dxf_with_trenches(&path, &[], &[t], sleufbase_rust::RULES).unwrap();
    let overlay = sleufbase_rust::dxf_read::load_dxf(&path).unwrap();
    let lines: Vec<_> = overlay
        .features
        .iter()
        .filter(|f| f.layer == "VIRTUAL_TRENCH_MARKER")
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].color_aci, 5);
    close(
        (lines[0].points[0].x, lines[0].points[0].y),
        (m[0].start.x, m[0].start.y),
    );
    assert_eq!(lines[0].points[0].z, Some(3.12));
}

#[test]
fn invalid_or_non_intersecting_lines_never_create_global_width_markers() {
    assert!(local_segment(&[], (0.0, 0.0), (0.0, 1.0), 0.0).is_none());
    let polygon = vec![(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (0.0, 2.0)];
    assert!(local_segment(&polygon, (8.0, 0.0), (0.0, 1.0), 0.0).is_none());
    assert!(local_segment(&polygon, (2.0, 1.0), (f64::NAN, 1.0), 0.0).is_none());
}
