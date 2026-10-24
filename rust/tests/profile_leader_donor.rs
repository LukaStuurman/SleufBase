use sleufbase_rust::{
    dxf_document::Document,
    profile_leader_donor::{self, BLOCK_NAME},
    template_geometry::{self, CircleBlock, Entity, Paint, Shape},
};
use std::{collections::HashMap, sync::atomic::AtomicBool};
const SOURCE: &[u8] = include_bytes!("fixtures/profile-leader/leader.dxf");
const EMPTY: &[u8] = include_bytes!("fixtures/profile-leader/empty.dxf");
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
#[test]
fn installed_attributes_and_multiple_reference_batches_work_without_document_reparse() {
    let cancel = AtomicBool::new(false);
    let mut d = doc(EMPTY);
    assert!(profile_leader_donor::install(&mut d, &cancel).unwrap());
    let installed = bytes(&d);
    assert!(!profile_leader_donor::install(&mut d, &cancel).unwrap());
    assert!(!profile_leader_donor::promote(&mut d, &cancel).unwrap());
    assert_eq!(bytes(&d), installed);
    let metrics =
        template_geometry::block_attribute_metrics(&d, BLOCK_NAME, "omschrijving", 0.02, &cancel)
            .unwrap()
            .unwrap();
    assert_eq!(metrics.offset, [-0.01, 0.2]);
    assert_eq!(metrics.height, 0.03);
    let layer = template_geometry::Layer {
        name: "X-XX-AL-VERWIJZING-SD".into(),
        color: 7,
        true_color: None,
        lineweight: Some(0),
    };
    for (scale, value) in [(-1., "Minimale schaal Ø"), (0.02, "Tweede batch 漢😀")] {
        let entity = profile_leader_donor::reference(
            value.into(),
            "-0.50".into(),
            [10., 20.],
            scale,
            Some(3),
        )
        .unwrap();
        template_geometry::add(&mut d, std::slice::from_ref(&layer), &[entity], &cancel).unwrap();
    }
    let mut written = doc(&bytes(&d));
    let inserts = written
        .records()
        .filter(|r| r.kind() == "INSERT" && r.first(2) == Some(BLOCK_NAME))
        .collect::<Vec<_>>();
    assert_eq!(inserts.len(), 2);
    for (r, scale) in inserts.iter().zip([0.005, 0.02]) {
        assert_eq!(r.first(41).unwrap().parse::<f64>().unwrap(), scale);
        assert_eq!(r.first(20).unwrap().parse::<f64>().unwrap(), 20. - scale);
    }
    let attrs = written
        .records()
        .filter(|r| r.kind() == "ATTRIB")
        .collect::<Vec<_>>();
    assert_eq!(attrs.len(), 4);
    assert_eq!(attrs[0].first(1), Some("Minimale schaal Ø"));
    assert_eq!(attrs[2].first(1), Some("Tweede batch 漢😀"));
    for (a, scale) in attrs.iter().zip([0.005, 0.005, 0.02, 0.02]) {
        assert!((a.first(40).unwrap().parse::<f64>().unwrap() - scale * 1.5).abs() < 1e-12);
    }
    let before = bytes(&written);
    assert!(!profile_leader_donor::install(&mut written, &cancel).unwrap());
    assert_eq!(bytes(&written), before);
}
#[test]
fn install_composes_with_circle_blocks_and_registers_missing_xdata_apps_atomically() {
    let cancel = AtomicBool::new(false);
    for (input, before) in [EMPTY, SOURCE]
        .into_iter()
        .flat_map(|input| [true, false].into_iter().map(move |before| (input, before)))
    {
        let source = doc(input);
        let mut d = rewrite(&source, |kind, _, tags| {
            if kind == "APPID" && tags.iter().any(|(c, v)| *c == 2 && v.starts_with("AcDb")) {
                tags.clear();
            }
        });
        let add = |d: &mut Document| {
            template_geometry::ensure_circle_block(
                d,
                CircleBlock {
                    name: "Other marker",
                    center: [0.; 3],
                    radius: 1.,
                    layer: "0",
                    paint: Paint::default(),
                },
                &cancel,
            )
            .unwrap()
        };
        if before {
            add(&mut d);
        }
        profile_leader_donor::install(&mut d, &cancel).unwrap();
        if !before {
            add(&mut d);
        }
        let written = doc(&bytes(&d));
        for name in ["APPID", "BLOCK_RECORD"] {
            let table = written
                .records()
                .find(|r| r.kind() == "TABLE" && r.first(2) == Some(name))
                .unwrap();
            assert_eq!(
                table.first(70).unwrap().parse::<usize>().unwrap(),
                written.records().filter(|r| r.kind() == name).count()
            );
        }
        for app in [
            "AcDbBlockRepETag",
            "AcDbDynamicBlockTrueName",
            "AcDbDynamicBlockGUID",
        ] {
            assert!(written
                .records()
                .any(|r| r.kind() == "APPID" && r.first(2) == Some(app)));
        }
    }
}
#[test]
fn failed_installation_preserves_previous_document_and_resource_cache() {
    let cancel = AtomicBool::new(false);
    let source = doc(EMPTY);
    for variant in 0..4 {
        let mut d = rewrite(&source, |kind, _, tags| {
            if variant == 1 && kind == "STYLE" {
                for (c, v) in tags.iter_mut() {
                    if *c == 2 && v == "NLCS-ISO" {
                        *v = "MISSING".into();
                    }
                }
            }
            if variant == 2 && kind == "LTYPE" {
                for (c, v) in tags.iter_mut() {
                    if *c == 2 && v == "Continuous" {
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
        });
        let before = bytes(&d);
        assert!(profile_leader_donor::install(&mut d, &AtomicBool::new(variant == 0)).is_err());
        assert_eq!(bytes(&d), before);
        assert!(template_geometry::block_attribute_metrics(
            &d, BLOCK_NAME, "HOOGTE", 0.02, &cancel
        )
        .is_err());
    }
    for (start, scale, color) in [
        ([f64::NAN, 0.], 0.02, None),
        ([0., 0.], f64::INFINITY, None),
        ([0., -f64::MAX], f64::MAX, None),
        ([0., 0.], 0.02, Some(257)),
    ] {
        assert!(
            profile_leader_donor::reference("".into(), "".into(), start, scale, color).is_err()
        );
    }
}
#[test]
fn existing_authored_blocks_and_unrelated_xdata_are_retained_during_installation() {
    let cancel = AtomicBool::new(false);
    let mut d = doc(include_bytes!("fixtures/profile-leader/tagged.dxf"));
    let old = doc(&bytes(&d))
        .records()
        .filter(|r| r.first(1001) == Some("KEEP_ME"))
        .count();
    assert_eq!(old, 5);
    assert!(profile_leader_donor::install(&mut d, &cancel).unwrap());
    let written = doc(&bytes(&d));
    assert_eq!(
        written
            .records()
            .filter(|r| r.tags().any(|(c, v)| c == 1001 && v == "KEEP_ME"))
            .count(),
        old
    );
    assert_eq!(
        written
            .records()
            .filter(|r| r.kind() == "BLOCK_RECORD" && r.first(2) == Some(BLOCK_NAME))
            .count(),
        1
    );
}
