use serde::Deserialize;
use sleufbase_rust::{leader_index::LeaderIndex, marxact::MarXactLeader};

#[derive(Deserialize)]
struct Case {
    leaders: Vec<MarXactLeader>,
    queries: Vec<[f64; 2]>,
    expected: Vec<Option<usize>>,
    original: Vec<Option<usize>>,
}
#[test]
fn spatial_lookup_matches_exhaustive_reference_and_fixes_python_rounding_misses() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/leader_lookup.json")).unwrap();
    for (number, case) in cases.iter().enumerate() {
        let index = LeaderIndex::new(&case.leaders);
        for (query, expected) in case.queries.iter().zip(&case.expected) {
            let actual = index.nearest(query[0], query[1]);
            assert_eq!(
                actual,
                expected.map(|i| &case.leaders[i]),
                "case {number}, query {query:?}"
            );
        }
    }
    // The old native linear scan chose source index 1. Python visits the
    // positive bucket last and therefore chooses index 0 at equal distance.
    assert_eq!(cases[0].expected, [Some(0)]);
    assert!(cases.iter().any(|c| c.expected != c.original));
    // Both signs of the exact 0.04/0.12 boundary now find the in-range label.
    for case in &cases[3..5] {
        assert_eq!(case.original[0], None);
        assert_eq!(case.expected[0], Some(0));
    }
}
