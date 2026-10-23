//! Original Python entity/label oracles plus native publication invariants.
use serde::Deserialize;
use serde_json::{json, Value};
use sleufbase_rust::{
    dxf_read,
    object_dxf::{self, Options},
    Dataset, Geometry, RULES,
};
use std::path::Path;
#[derive(Deserialize)]
struct Case {
    name: String,
    datasets: Vec<Dataset>,
    options: Options,
    expected: Value,
    native_layers: Value,
}
fn cases() -> Vec<Case> {
    serde_json::from_str(include_str!("fixtures/object-export/cases.json")).unwrap()
}
#[test]
fn virtual_job_exports_stably_order_objects_between_endpoints_and_use_the_trench_direction() {
    use sleufbase_rust::virtual_trench::{TrenchPoint, VirtualTrench};
    for case in cases()
        .into_iter()
        .filter(|c| c.name.starts_with("virtual-"))
    {
        let source = &case.datasets[0];
        let point = |f: &sleufbase_rust::Feature| match f.geometry {
            Geometry::Point(p) => p,
            _ => panic!(),
        };
        let mut trench = VirtualTrench::new(
            source.title.clone(),
            point(&source.features[0]),
            point(source.features.last().unwrap()),
            1.,
        )
        .unwrap();
        trench.job_id = source.job_id;
        trench.source_path = Some(source.source_path.clone());
        let mut items: Vec<_> = source.features[1..3].iter().collect();
        if case.name == "virtual-sequence" {
            items.reverse();
        }
        for f in items {
            let object = TrenchPoint {metadata:Default::default(),
                role: "object".into(),
                object_name: f.object_name.clone(),
                source_name: f.source_name.clone(),
                position: point(f),
                attributes: f.attributes.clone(),
            };
            if case.name == "virtual-zero-tie" {
                trench.add_object(object).unwrap();
            } else {
                trench.objects.push(object);
            }
        }
        let prepared = trench.to_dataset(0, true);
        if case.name == "virtual-sequence" {
            assert_eq!(
                trench.objects[0].position.x, 8.,
                "source order remains intact"
            );
        }
        let mut bytes = Vec::new();
        object_dxf::encode_iter(
            &mut bytes,
            std::iter::once(&prepared),
            &[],
            None,
            &[],
            RULES,
            case.options,
        )
        .unwrap();
        let mut expected = case.expected;
        expected["layers"] = case.native_layers;
        close(&signature(&bytes), &expected);
    }
}
fn close(actual: &Value, expected: &Value) {
    match expected {
        Value::Array(items) => {
            let actual = actual.as_array().unwrap();
            assert_eq!(actual.len(), items.len());
            for (a, b) in actual.iter().zip(items) {
                close(a, b);
            }
        }
        Value::Object(items) => {
            let actual = actual.as_object().unwrap();
            assert_eq!(actual.len(), items.len());
            for (k, b) in items {
                close(&actual[k], b);
            }
        }
        Value::Number(_) => {
            let a = actual.as_f64().unwrap();
            let b = expected.as_f64().unwrap();
            assert!((a - b).abs() < 1e-9, "{a} != {b}");
        }
        _ => assert_eq!(actual, expected),
    }
}
fn signature(bytes: &[u8]) -> Value {
    let text = std::str::from_utf8(bytes).unwrap();
    let doc = dxf_read::parse_dxf(text, Path::new("native.dxf")).unwrap();
    let mut features = doc.features.iter();
    let lines: Vec<_> = text.lines().collect();
    let tags: Vec<_> = lines
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| (p[0].trim().parse::<i32>().unwrap(), p[1]))
        .collect();
    let mut inside = false;
    let mut records = Vec::new();
    let mut font = None;
    let mut units = None;
    for (at, &(code, value)) in tags.iter().enumerate() {
        if code == 9 && value == "$INSUNITS" {
            units = Some(tags[at + 1].1.parse::<u32>().unwrap());
        }
        if code == 0 && value == "STYLE" {
            let end = tags[at + 1..]
                .iter()
                .position(|(c, _)| *c == 0)
                .map_or(tags.len(), |n| at + 1 + n);
            let part = &tags[at..end];
            if part.contains(&(2, "PROEFSLEUVEN_LIBMONO")) {
                font = Some(part.iter().find(|(c, _)| *c == 3).unwrap().1);
            }
        }
        if code == 2 && value == "ENTITIES" {
            inside = true;
            continue;
        }
        if inside && code == 0 && value == "ENDSEC" {
            inside = false;
        }
        if !inside || code != 0 || matches!(value, "VERTEX" | "SEQEND") {
            continue;
        }
        let end = tags[at + 1..]
            .iter()
            .position(|(c, _)| *c == 0)
            .map_or(tags.len(), |n| at + 1 + n);
        let part = &tags[at..end];
        let field = |c| {
            part.iter()
                .find(|(code, _)| *code == c)
                .map(|(_, v)| *v)
                .unwrap()
        };
        let number = |c| field(c).parse::<f64>().unwrap();
        let mut record =
            json!({"kind":value,"layer":field(8),"color":field(62).parse::<i32>().unwrap()});
        if value == "TEXT" {
            record["text"] = json!(field(1));
            record["height"] = json!(number(40));
            record["style"] = json!(field(7));
            record["align"] = json!([number(72), number(73)]);
            record["true_color"] = json!(number(420));
            record["points"] = json!([
                [number(10), number(20), number(30)],
                [number(11), number(21), number(31)]
            ]);
        } else {
            let feature = features.next().unwrap();
            assert_eq!(feature.entity_type, value);
            record["points"] = json!(feature
                .points
                .iter()
                .map(|v| [v.x, v.y, v.z.unwrap_or(0.)])
                .collect::<Vec<_>>());
            if value == "POLYLINE" {
                record["closed"] = json!(feature.closed.unwrap());
            }
            if let Some(data) = feature
                .cad
                .as_ref()
                .and_then(|p| p.xdata.get("KTM_DXF_EXPORT"))
            {
                record["xdata"] = json!(data.iter().map(|t| &t.value).collect::<Vec<_>>());
            }
        }
        records.push(record);
    }
    let layers: serde_json::Map<_, _> = doc
        .layers
        .iter()
        .map(|(name, l)| {
            (
                name.clone(),
                json!({"color":l.color_aci,"true_color":l.true_color}),
            )
        })
        .collect();
    json!({"entities":records,"layers":layers,"font":font.unwrap(),"units":units.unwrap()})
}
#[test]
fn complete_object_exports_match_python_entities_names_direction_styles_xdata_and_labels() {
    for case in cases() {
        let mut out = Vec::new();
        object_dxf::encode(
            &mut out,
            &case.datasets,
            &[],
            None,
            &[],
            RULES,
            case.options,
        )
        .unwrap();
        let mut expected = case.expected;
        expected["layers"] = case.native_layers;
        close(&signature(&out), &expected);
        assert!(
            std::str::from_utf8(&out).unwrap().ends_with("0\nEOF\n"),
            "{}",
            case.name
        );
    }
}
#[test]
fn job_label_word_boundaries_unicode_digits_zeroes_and_fallbacks_match_python() {
    #[derive(Deserialize)]
    struct Label {
        title: String,
        expected: String,
    }
    let labels: Vec<Label> =
        serde_json::from_str(include_str!("fixtures/object-export/labels.json")).unwrap();
    for label in labels {
        let dataset = Dataset {
            title: label.title.clone(),
            source_path: "fallback-42.json".into(),
            features: vec![],
            job_id: 0,
        };
        assert_eq!(
            object_dxf::job_label(&dataset, 3),
            label.expected,
            "{:?}",
            label.title
        );
    }
}
#[test]
fn file_and_buffer_encodings_are_identical_and_precise_polylines_roundtrip_f64_bits() {
    let case = cases().remove(1);
    let mut data = case.datasets;
    let Geometry::Polyline(points) = &mut data[0].features[0].geometry else {
        panic!()
    };
    points[0].x = -0.42838541666666685;
    points[0].y = 1e-10;
    points[0].z = Some(f64::from_bits(1));
    let original = points[0];
    let mut buffer = Vec::new();
    object_dxf::encode(
        &mut buffer,
        &data,
        &[],
        None,
        &[],
        RULES,
        Default::default(),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("precise.dxf");
    sleufbase_rust::write_dxf(&path, &data, RULES).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), buffer);
    let parsed = dxf_read::load_dxf(&path).unwrap();
    let p = parsed.features[0].points[0];
    assert_eq!(p.x.to_bits(), original.x.to_bits());
    assert_eq!(p.y.to_bits(), original.y.to_bits());
    assert_eq!(p.z.unwrap().to_bits(), original.z.unwrap().to_bits());
}
#[test]
fn limits_nonfinite_values_invalid_symbols_and_injected_tags_never_publish_partial_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("objects.dxf");
    let mut case = cases().remove(0);
    let options = Options {
        max_bytes: 100,
        ..Default::default()
    };
    assert!(object_dxf::write(&path, &case.datasets, RULES, options)
        .unwrap_err()
        .contains("uitvoerlimiet"));
    assert!(!path.exists());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    for bad in ["bad\n0\nEOF", "nul\0value", "é".repeat(128).as_str()] {
        case.datasets[0].features[0].object_name = bad.into();
        assert!(sleufbase_rust::write_dxf(&path, &case.datasets, RULES).is_err());
        assert!(!path.exists());
    }
    case.datasets[0].features[0].object_name = "safe".into();
    if let Geometry::Point(v) = &mut case.datasets[0].features[0].geometry {
        v.x = f64::NAN;
    }
    assert!(sleufbase_rust::write_dxf(&path, &case.datasets, RULES).is_err());
    assert!(!path.exists());
    let case = cases().remove(0);
    static INVALID: &[sleufbase_rust::Rule] = &[sleufbase_rust::Rule {
        keywords: std::borrow::Cow::Borrowed(&[std::borrow::Cow::Borrowed("water")]),
        target_layer: std::borrow::Cow::Borrowed("BAD\nLAYER"),
        color_aci: 5,
        label: std::borrow::Cow::Borrowed("Water"),
    }];
    assert!(object_dxf::write(&path, &case.datasets, INVALID, Default::default()).is_err());
    assert!(!path.exists());
    object_dxf::write(&path, &case.datasets, RULES, Default::default()).unwrap();
    let previous = std::fs::read(&path).unwrap();
    assert!(object_dxf::write(&path, &[], INVALID, Default::default())
        .unwrap_err()
        .contains("bestaat al"));
    assert_eq!(std::fs::read(&path).unwrap(), previous);
    assert!(object_dxf::encode(
        Vec::new(),
        &case.datasets,
        &[],
        None,
        &[],
        RULES,
        Options {
            point_half_length: f64::INFINITY,
            ..Default::default()
        }
    )
    .is_err());
}
