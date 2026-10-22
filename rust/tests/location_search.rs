use serde_json::Value;
use sleufbase_rust::location_search::*;
fn reference() -> Value {
    serde_json::from_str(include_str!("fixtures/location/reference.json")).unwrap()
}
#[test]
fn original_rd_gps_prefixes_order_unicode_and_pyproj_coordinates_match() {
    let fixture = reference();
    let mut gps = 0;
    for case in fixture["coordinates"].as_array().unwrap() {
        let query = case["query"].as_str().unwrap();
        assert_eq!(
            serde_json::to_value(parse_rd(query)).unwrap(),
            case["rd"],
            "RD {query}"
        );
        let actual = parse_coordinate(query);
        if case["coordinate"].is_null() {
            assert!(actual.is_none(), "{query}");
            continue;
        }
        let expected: Location = serde_json::from_value(case["coordinate"].clone()).unwrap();
        let actual = actual.unwrap_or_else(|| panic!("missing {query}"));
        assert_eq!(actual.label, expected.label, "{query}");
        assert_eq!(actual.location_type, expected.location_type);
        let error = (actual.x - expected.x).hypot(actual.y - expected.y);
        assert!(
            error < 0.002,
            "{query}: {error} metres; {actual:?} != {expected:?}"
        );
        gps += usize::from(actual.location_type == "wgs84");
    }
    assert!(gps > 190);
}
fn compare_request(url: reqwest::Url, expected: &Value) {
    let query = url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let params = expected["params"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                v.as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.to_string()),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        url.as_str().split('?').next().unwrap(),
        expected["url"].as_str().unwrap()
    );
    assert_eq!(query, params);
}
#[test]
fn actual_python_search_reverse_urls_fields_defaults_and_zoom_scales_match() {
    let fixture = reference();
    for case in fixture["searches"].as_array().unwrap() {
        let query = case["query"].as_str().unwrap();
        assert_eq!(
            serde_json::to_value(parse_search(&case["payload"], query).unwrap()).unwrap(),
            case["output"]
        );
        compare_request(
            search_url(query, case["rows"].as_i64().unwrap() as i32).unwrap(),
            &case["request"],
        );
    }
    for case in fixture["reverses"].as_array().unwrap() {
        assert_eq!(
            serde_json::to_value(parse_reverse(&case["payload"]).unwrap()).unwrap(),
            case["output"]
        );
        compare_request(
            reverse_url(155000.1254, 463000.7506, 99).unwrap(),
            &case["request"],
        );
    }
    for (kind, value) in fixture["zooms"].as_object().unwrap() {
        let location = Location {
            label: String::new(),
            x: 155000.,
            y: 463000.,
            location_type: kind.clone(),
        };
        assert_eq!(location.meters_per_pixel(), value.as_f64().unwrap());
    }
}
#[test]
fn malformed_or_nonfinite_service_records_never_reach_the_camera_and_inputs_are_bounded() {
    use serde_json::json;
    let payload = json!({"response":{"docs":[{"centroide_rd":"POINT(..1 --2)"},{"centroide_rd":"POINT(155000 463000)","type":"adres"}]}});
    assert_eq!(parse_search(&payload, "x").unwrap().len(), 1);
    assert!(parse_coordinate(&format!("{} 463000", "9".repeat(500))).is_none());
    assert!(parse_coordinate(&"a".repeat(4097)).is_none());
    assert!(search_url(&"a".repeat(4097), 6).is_err());
    assert!(reverse_url(f64::INFINITY, 1., 1).is_err());
    assert!(wgs84_to_rd(f64::NAN, 0.).is_err());
    for payload in [
        json!([]),
        json!({"response":null}),
        json!({"response":{"docs":null}}),
        json!({"response":{"docs":vec![json!({});1001]}}),
    ] {
        assert!(parse_search(&payload, "x").is_err());
        assert!(parse_reverse(&payload).is_err());
    }
    assert_eq!(
        parse_reverse(&json!({"response":{"docs":[{"afstand":"nan"}]}})).unwrap()[0].distance_m,
        0.
    );
}
