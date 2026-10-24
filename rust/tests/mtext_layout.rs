use serde_json::Value;
use sleufbase_rust::mtext_layout::{
    self as ml, CellSpec, ColumnSpec, Error, Event, ParagraphSpec, Primitive, Request, TabStop,
};
use std::sync::{atomic::AtomicBool, Arc};

fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert!(
                (a - b).abs() <= 1e-9 + 64. * f64::EPSILON * a.abs().max(b.abs()),
                "{path}: {a} != {b}"
            );
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, b) in b {
                compare(
                    a.get(k).unwrap_or_else(|| panic!("{path}: missing {k}")),
                    b,
                    &format!("{path}.{k}"),
                );
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
fn request(events: Vec<Event>) -> Request {
    Request {
        width: 10.,
        height: None,
        margins: Vec::new(),
        columns: vec![ColumnSpec::default()],
        events,
        at: [0.; 2],
        align: 1,
        frame: None,
    }
}
fn text(id: u32, width: f64, height: f64) -> CellSpec {
    CellSpec::Text {
        id,
        width,
        height,
        valign: 0,
        stroke: 0,
    }
}
fn paragraph(cells: Vec<CellSpec>) -> ParagraphSpec {
    ParagraphSpec {
        cells,
        align: 1,
        indent: [0.; 3],
        spacing: 1.,
        margins: Vec::new(),
        tabs: Arc::default(),
        frame: None,
    }
}

#[test]
fn full_original_measured_cell_geometry_tabs_columns_and_render_order_match() {
    let data: Value =
        serde_json::from_str(include_str!("fixtures/mtext-layout/layout.json")).unwrap();
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 369);
    let mut primitives = 0;
    let mut errors = 0;
    for (i, c) in cases.iter().enumerate() {
        let r: Request = serde_json::from_value(c["input"].clone()).unwrap();
        let actual = ml::layout(r, &AtomicBool::new(false));
        if let Some(expected) = c.get("output") {
            let actual = actual.unwrap_or_else(|e| panic!("case {i}: {e}"));
            primitives += actual.primitives.len();
            compare(
                &serde_json::to_value(actual).unwrap(),
                expected,
                &format!("case {i}"),
            );
        } else {
            assert!(actual.is_err(), "case {i}: expected original error/guard");
            errors += 1;
        }
    }
    assert_eq!(primitives, 8747);
    assert_eq!(errors, 66);
}
#[test]
fn shared_unsorted_tab_arrays_are_indexed_once_for_many_paragraphs() {
    let tabs: Arc<[TabStop]> = Arc::from(
        (1..=20_000)
            .rev()
            .map(|n| TabStop {
                pos: f64::from(n),
                kind: 0,
            })
            .collect::<Vec<_>>(),
    );
    let events = (0..3000)
        .map(|n| {
            let mut p = paragraph(vec![
                text(n * 2 + 1, 1., 1.),
                CellSpec::Tab {
                    width: 1.,
                    min: None,
                    max: None,
                },
                text(n * 2 + 2, 1., 1.),
            ]);
            p.tabs = tabs.clone();
            Event::Paragraph(p)
        })
        .collect();
    let mut r = request(events);
    r.width = 20_001.;
    let out = ml::layout(r, &AtomicBool::new(false)).unwrap();
    assert_eq!(out.primitives.len(), 6000);
    for pair in out.primitives.as_chunks::<2>().0 {
        let Primitive::Text { rect, .. } = pair[1] else {
            panic!("text required")
        };
        assert_eq!(rect.left, 20_000.);
        assert_eq!(rect.right, 20_001.);
    }
}
#[test]
fn column_height_accumulation_stays_linear_for_many_empty_paragraphs() {
    let events = (0..50_000)
        .map(|_| Event::Empty {
            height: 1.,
            spacing: 1.,
        })
        .collect();
    let out = ml::layout(request(events), &AtomicBool::new(false)).unwrap();
    assert!(out.primitives.is_empty());
    assert_eq!(out.columns[0].paragraphs.len(), 50_000);
    let expected = (0..49_999).fold(0., |h, _| h + 1.667) + 1.;
    assert!((out.columns[0].used_height - expected).abs() < 1e-8);
}
#[test]
fn cancel_invalid_geometry_and_resource_limits_return_no_partial_plan() {
    assert!(matches!(
        ml::layout(request(Vec::new()), &AtomicBool::new(true)),
        Err(Error::Cancelled)
    ));
    let mut bad = request(vec![Event::Paragraph(paragraph(vec![text(1, 1., 1.)]))]);
    bad.width = 0.;
    assert!(matches!(
        ml::layout(bad, &AtomicBool::new(false)),
        Err(Error::Invalid(_))
    ));
    let mut bad = request(Vec::new());
    bad.at[0] = f64::NAN;
    assert!(matches!(
        ml::layout(bad, &AtomicBool::new(false)),
        Err(Error::Invalid(_))
    ));
    let bad = request(
        (0..200_001)
            .map(|_| Event::Empty {
                height: 1.,
                spacing: 1.,
            })
            .collect(),
    );
    assert!(matches!(
        ml::layout(bad, &AtomicBool::new(false)),
        Err(Error::Limit(_))
    ));
    let mut bad = request(vec![Event::Paragraph(paragraph(vec![text(1, 1., 10.)]))]);
    bad.columns[0].height = Some(1.);
    assert!(matches!(
        ml::layout(bad, &AtomicBool::new(false)),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn canonical_value_decode_handles_tagged_decimal_metrics_without_losing_precision() {
    let mut p = paragraph(vec![text(1, 1.3910000000000002, 1.3235000000000001)]);
    p.spacing = 1.3910000000000002;
    let request = request(vec![Event::Paragraph(p)]);
    let raw = serde_json::to_vec(&request).unwrap();
    let value: Value = serde_json::from_slice(&raw).unwrap();
    let decoded: Request = serde_json::from_value(value).unwrap();
    let out = ml::layout(decoded, &AtomicBool::new(false)).unwrap();
    let Primitive::Text { rect, .. } = out.primitives[0] else {
        panic!("text required")
    };
    assert_eq!(rect.right, 1.3910000000000002);
    assert_eq!(rect.bottom, -1.3235000000000001);
}
