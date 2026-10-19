use sleufbase_rust::{
    dxf_document::Document,
    profile_leader_donor::{self, BLOCK_NAME},
    template_geometry::{self, CircleBlock, Entity, Paint, Shape},
};
use std::{collections::HashMap, sync::atomic::AtomicBool};
const SOURCE: &[u8] = include_bytes!("fixtures/profile-leader/leader.dxf");
fn doc(bytes: &[u8]) -> Document {
    Document::from_bytes(bytes.to_vec(), &AtomicBool::new(false)).unwrap()
}
fn bytes(doc: &Document) -> Vec<u8> {
    let mut v = Vec::new();
    doc.write_to(&mut v, &AtomicBool::new(false)).unwrap();
    v
}
fn rewrite(
    doc: &Document,
    mut f: impl FnMut(&str, Option<u64>, &mut Vec<(i32, String)>),
) -> Document {
    let mut out = String::new();
    for r in doc.records() {
        let mut tags = r.tags().map(|(c, v)| (c, v.to_owned())).collect();
        f(r.kind(), r.handle(), &mut tags);
        for (c, v) in tags {
            out.push_str(&format!("{c:>3}\n{v}\n"));
        }
    }
    self::doc(out.as_bytes())
}
fn block_id(doc: &Document) -> u64 {
    doc.records()
        .find(|r| r.kind() == "BLOCK_RECORD" && r.first(2) == Some(BLOCK_NAME))
        .unwrap()
        .handle()
        .unwrap()
}
#[test]
fn exact_approved_graph_and_all_entity_references_are_remapped() {
    let cancel = AtomicBool::new(false);
    let mut source = doc(SOURCE);
    let source_handles = source
        .records()
        .filter_map(|r| r.handle())
        .collect::<Vec<_>>();
    let block = block_id(&source);
    let geometry = source
        .records()
        .filter(|r| {
            r.section() == Some("BLOCKS")
                && r.first(330).and_then(|v| u64::from_str_radix(v, 16).ok()) == Some(block)
        })
        .filter(|r| matches!(r.kind(), "CIRCLE" | "ATTDEF" | "LWPOLYLINE"))
        .map(|r| r.handle().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(geometry.len(), 4);
    assert!(profile_leader_donor::promote(&mut source, &cancel).unwrap());
    let written = doc(&bytes(&source));
    let added = written
        .records()
        .filter(|r| r.handle().is_some_and(|h| !source_handles.contains(&h)))
        .collect::<Vec<_>>();
    assert_eq!(added.len(), 15);
    let donor: Vec<Vec<(i32, String)>> =
        serde_json::from_str(include_str!("../src/data/profile-leader-donor.json")).unwrap();
    let mut map = HashMap::from([
        (0x2E5, block),
        (0x2F6, geometry[0]),
        (0x2F7, geometry[1]),
        (0x2FB, geometry[2]),
        (0x2FC, geometry[3]),
    ]);
    for (d, r) in donor.iter().zip(&added) {
        map.insert(
            u64::from_str_radix(&d.iter().find(|(c, _)| *c == 5).unwrap().1, 16).unwrap(),
            r.handle().unwrap(),
        );
    }
    for (d, r) in donor.iter().zip(added) {
        let actual = r.tags().collect::<Vec<_>>();
        assert_eq!(d.len(), actual.len());
        for ((c, v), (ac, av)) in d.iter().zip(actual) {
            assert_eq!(*c, ac);
            if matches!(c,5|320..=369|390..=399|480|481|1005) {
                let old = u64::from_str_radix(v.trim(), 16).unwrap();
                let actual = u64::from_str_radix(av, 16).unwrap();
                assert_eq!(actual, if old == 0 { 0 } else { map[&old] });
                assert!(
                    actual == 0 || written.by_handle(&format!("{actual:X}")).unwrap().is_some()
                );
            } else {
                assert_eq!(v, av);
            }
        }
    }
}
#[test]
fn idempotent_before_and_after_serialization_preserves_existing_dynamic_graph() {
    for source in [SOURCE, include_bytes!("fixtures/profile-leader/tagged.dxf")] {
        let mut d = doc(source);
        let cancel = AtomicBool::new(false);
        assert!(profile_leader_donor::promote(&mut d, &cancel).unwrap());
        let promoted = bytes(&d);
        assert!(!profile_leader_donor::promote(&mut d, &cancel).unwrap());
        assert_eq!(bytes(&d), promoted);
        let mut d = doc(&promoted);
        assert!(!profile_leader_donor::promote(&mut d, &cancel).unwrap());
        assert_eq!(bytes(&d), promoted);
    }
}
#[test]
fn cancellation_missing_or_ambiguous_geometry_and_overflow_leave_source_untouched() {
    let source = doc(SOURCE);
    let mut d = doc(SOURCE);
    let before = bytes(&d);
    assert!(profile_leader_donor::promote(&mut d, &AtomicBool::new(true)).is_err());
    assert_eq!(bytes(&d), before);
    for variant in 0..5 {
        let mut d = rewrite(&source, |kind, _, tags| {
            if variant == 0 && kind == "ATTDEF" {
                for (c, v) in tags.iter_mut() {
                    if *c == 2 && v == "HOOGTE" {
                        *v = "MISSING".into();
                    }
                }
            }
            if variant == 1 && kind == "CIRCLE" {
                for (c, v) in tags.iter_mut() {
                    if *c == 40 {
                        *v = "100".into();
                    }
                }
            }
            if variant == 2 && kind == "BLOCK_RECORD" {
                for (c, v) in tags.iter_mut() {
                    if *c == 2 && v == BLOCK_NAME {
                        *v = "MISSING".into();
                    }
                }
            }
            if variant == 3 && kind == "SECTION" {
                let mut seed = false;
                for (c, v) in tags.iter_mut() {
                    if *c == 9 {
                        seed = v == "$HANDSEED";
                    } else if seed && *c == 5 {
                        *v = "FFFFFFFFFFFFFFF8".into();
                    }
                }
            }
            if variant == 4 && kind == "ATTDEF" {
                for (c, v) in tags.iter_mut() {
                    if *c == 2 && v == "HOOGTE" {
                        *v = "OMSCHRIJVING".into();
                    }
                }
            }
        });
        let before = bytes(&d);
        assert!(
            profile_leader_donor::promote(&mut d, &AtomicBool::new(false)).is_err(),
            "variant {variant}"
        );
        assert_eq!(bytes(&d), before);
    }
}
#[test]
fn incomplete_existing_graph_including_missing_scale_action_is_never_overwritten() {
    let cancel = AtomicBool::new(false);
    let mut d = doc(SOURCE);
    profile_leader_donor::promote(&mut d, &cancel).unwrap();
    let full = doc(&bytes(&d));
    for remove in ["BLOCKSCALEACTION", "BLOCKMOVEACTION", "DICTIONARY"] {
        let mut incomplete = rewrite(&full, |kind, _, tags| {
            if kind == remove {
                tags.clear();
            }
        });
        let before = bytes(&incomplete);
        assert!(profile_leader_donor::promote(&mut incomplete, &cancel).is_err());
        assert_eq!(bytes(&incomplete), before);
    }
}
#[test]
fn shared_seed_and_unrelated_text_edits_compose_with_multiple_geometry_batches() {
    let cancel = AtomicBool::new(false);
    for before in [true, false] {
        let mut d = doc(SOURCE);
        let label = d
            .records()
            .find(|r| r.kind() == "MTEXT")
            .unwrap()
            .handle()
            .unwrap();
        d.replace_text(&format!("{label:X}"), "Nieuwe onafhankelijke tekst Ø 漢😀")
            .unwrap();
        let insert = |d: &mut Document| {
            template_geometry::ensure_circle_block(
                d,
                CircleBlock {
                    name: "Unrelated marker",
                    center: [0., 0., 0.],
                    radius: 1.,
                    layer: "0",
                    paint: Paint::default(),
                },
                &cancel,
            )
            .unwrap();
            template_geometry::add(
                d,
                &[],
                &[Entity {
                    layer: "0".into(),
                    paint: Paint::default(),
                    layout: None,
                    shape: Shape::Circle {
                        center: [7., 8.],
                        radius: 2.,
                    },
                }],
                &cancel,
            )
            .unwrap();
        };
        if before {
            insert(&mut d);
        }
        profile_leader_donor::promote(&mut d, &cancel).unwrap();
        if !before {
            insert(&mut d);
        }
        let written = doc(&bytes(&d));
        assert!(written.records().any(
            |r| r.kind() == "MTEXT" && r.first(1) == Some("Nieuwe onafhankelijke tekst Ø 漢😀")
        ));
        assert!(written
            .records()
            .any(|r| r.kind() == "BLOCK" && r.first(2) == Some("Unrelated marker")));
        let maximum = written.records().filter_map(|r| r.handle()).max().unwrap();
        let text = String::from_utf8(bytes(&d)).unwrap().replace("\r\n", "\n");
        let handseed = text
            .split("$HANDSEED\n")
            .nth(1)
            .unwrap()
            .lines()
            .nth(1)
            .unwrap();
        assert!(u64::from_str_radix(handseed, 16).unwrap() > maximum);
    }
}
