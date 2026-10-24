use serde_json::{json, Value};
use sleufbase_rust::profile_context::{self, Kind};
use std::sync::atomic::{AtomicBool, Ordering};
#[test]
fn original_requests_geometry_rings_and_complete_paginated_batches_match() {
    let data: Value =
        serde_json::from_str(include_str!("fixtures/profile-context/reference.json")).unwrap();
    for c in data["cases"].as_array().unwrap() {
        let kind: Kind = serde_json::from_value(c["kind"].clone()).unwrap();
        let bounds = serde_json::from_value(c["bounds"].clone()).unwrap();
        let mut index = 0;
        let paths = profile_context::fetch_with(kind, bounds, &AtomicBool::new(false), |url| {
            let call = &c["calls"][index];
            if index == 0 {
                assert_eq!(
                    url.as_str().split('?').next().unwrap(),
                    call["url"].as_str().unwrap()
                );
                let query = url
                    .query_pairs()
                    .into_owned()
                    .collect::<std::collections::HashMap<_, _>>();
                for (k, v) in call["params"].as_object().unwrap() {
                    assert_eq!(
                        &query[k],
                        &v.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| v.to_string())
                    );
                }
            } else {
                assert_eq!(url.as_str(), call["url"].as_str().unwrap());
            }
            let value = c["pages"][index].clone();
            index += 1;
            Ok(value)
        })
        .unwrap();
        assert_eq!(index, c["calls"].as_array().unwrap().len());
        assert_eq!(serde_json::to_value(&*paths).unwrap(), c["paths"]);
    }
}
#[test]
fn unsafe_pagination_cycles_malformed_shapes_and_cancellation_publish_no_partial_paths() {
    let bounds = sleufbase_rust::Bounds {
        min_x: 155000.,
        min_y: 463000.,
        max_x: 155100.,
        max_y: 463100.,
    };
    let base = profile_context::url(Kind::Roads, bounds).unwrap();
    for next in [
        "https://example.com/next",
        "http://api.pdok.nl/brt/top10nl/ogc/v1/collections/wegdeel_hartlijn/items",
        "https://user@api.pdok.nl/brt/top10nl/ogc/v1/collections/wegdeel_hartlijn/items",
        "https://api.pdok.nl/lv/bgt/ogc/v1/collections/onbegroeidterreindeel/items",
        "file:///C:/private.json",
    ] {
        assert!(profile_context::next(Kind::Roads, &base, next).is_err());
    }
    let cancel = AtomicBool::new(false);
    assert!(
        profile_context::fetch_with(Kind::Roads, bounds, &cancel, |url| Ok(
            json!({"features":[],"links":[{"rel":"next","href":url.as_str()}]})
        ))
        .is_err()
    );
    assert!(profile_context::page(Kind::Terrain, &json!({"features":null}), &cancel).is_err());
    assert!(profile_context::page(
        Kind::Roads,
        &json!({"features":[{"geometry":{"type":"LineString","coordinates":[[1]]}}]}),
        &cancel
    )
    .is_err());
    assert!(
        profile_context::fetch_with(Kind::Roads, bounds, &cancel, |_| {
            cancel.store(true, Ordering::Relaxed);
            Ok(json!({"features":[]}))
        })
        .is_err()
    );
}
