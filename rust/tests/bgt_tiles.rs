use sleufbase_rust::{
    bgt_tiles::{self, TileId},
    Bounds,
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn compare(actual: &serde_json::Value, expected: &serde_json::Value, path: &str) {
    use serde_json::Value;
    match (actual, expected) {
        (Value::Number(a), Value::Number(e)) => assert!(
            (a.as_f64().unwrap() - e.as_f64().unwrap()).abs() < 1e-9,
            "{path}: {a} != {e}"
        ),
        (Value::Array(a), Value::Array(e)) => {
            assert_eq!(a.len(), e.len(), "{path}");
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                compare(a, e, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(e)) => {
            assert_eq!(a.len(), e.len(), "{path}");
            for (k, e) in e {
                compare(&a[k], e, &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
#[test]
fn original_tile_decoding_and_surface_parts_preserve_rd_geometry_ids_and_order() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/bgt-tiles/reference.json")).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let cancel = AtomicBool::new(false);
    for case in fixture["cases"].as_array().unwrap() {
        let id: TileId = serde_json::from_value(case["id"].clone()).unwrap();
        let data = std::fs::read(root.join(case["path"].as_str().unwrap())).unwrap();
        let tile = bgt_tiles::decode(&data, id, &cancel).unwrap();
        let mut features = Vec::new();
        let mut surfaces = Vec::new();
        for f in &tile.features {
            let v = serde_json::to_value(f).unwrap();
            let mut basic = v.clone();
            basic.as_object_mut().unwrap().remove("physical_appearance");
            features.push(basic);
            if !f.physical_appearance.is_empty()
                && matches!(
                    f.geometry,
                    sleufbase_rust::bgt_surface::Geometry::Polygon(_)
                        | sleufbase_rust::bgt_surface::Geometry::MultiPolygon(_)
                )
            {
                surfaces.push(v);
            }
        }
        compare(
            &serde_json::json!(features),
            &case["features"],
            case["name"].as_str().unwrap(),
        );
        compare(
            &serde_json::json!(surfaces),
            &case["surfaces"],
            case["name"].as_str().unwrap(),
        );
        assert!(tile.memory_bytes <= bgt_tiles::MAX_DECODED_BYTES);
    }
}
#[test]
fn original_matrix_boundaries_keep_epsilon_and_enumeration_order() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/bgt-tiles/reference.json")).unwrap();
    for case in fixture["grids"].as_array().unwrap() {
        let b: Bounds = serde_json::from_value(case["bounds"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(bgt_tiles::tiles_for_bounds(b).unwrap()).unwrap(),
            case["expected"]
        );
    }
    for b in [
        Bounds {
            min_x: 0.,
            min_y: 0.,
            max_x: f64::NAN,
            max_y: 1.,
        },
        Bounds {
            min_x: 5.,
            min_y: 0.,
            max_x: 1.,
            max_y: 1.,
        },
        Bounds {
            min_x: -1e8,
            min_y: -1e8,
            max_x: 1e8,
            max_y: 1e8,
        },
    ] {
        assert!(bgt_tiles::tiles_for_bounds(b).is_err());
    }
    assert!(TileId { row: 4096, col: 0 }.url().is_err());
}
#[test]
fn malformed_protobuf_and_commands_cancellation_and_limits_return_errors() {
    let cancel = AtomicBool::new(false);
    let id = TileId { row: 0, col: 0 };
    for bytes in [
        vec![0],
        vec![0xff; 11],
        vec![0x1a, 10, 1],
        vec![0x0e],
        vec![0x1a, 2, 0x08, 1],
    ] {
        assert!(bgt_tiles::decode(&bytes, id, &cancel).is_err(), "{bytes:?}");
    }
    let valid = include_bytes!("fixtures/bgt-tiles/value-types.pbf");
    assert!(bgt_tiles::decode(valid, id, &AtomicBool::new(true)).is_err());
    assert!(bgt_tiles::decode(&vec![0; bgt_tiles::MAX_TILE_BYTES + 1], id, &cancel).is_err());
    let mut bad = valid.to_vec();
    bad.pop();
    assert!(bgt_tiles::decode(&bad, id, &cancel).is_err());
}
