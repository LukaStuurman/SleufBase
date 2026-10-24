use sleufbase_rust::{
    dxf_document::Document,
    template_variants::{self, Pair},
};
use std::sync::atomic::{AtomicBool, Ordering};
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/template-variants/reference.json")).unwrap()
}
fn doc() -> Document {
    Document::from_bytes(
        include_bytes!("fixtures/template-variants/input.dxf").to_vec(),
        &AtomicBool::new(false),
    )
    .unwrap()
}
fn bytes(doc: &Document) -> Vec<u8> {
    let mut out = Vec::new();
    doc.write_to(&mut out, &AtomicBool::new(false)).unwrap();
    out
}
fn pairs(case: usize) -> Vec<Pair> {
    serde_json::from_value(fixture()["cases"][case]["pairs"].clone()).unwrap()
}
#[test]
fn original_unicode_names_and_finalized_pairs_keep_complete_graph_references() {
    let f = fixture();
    for c in f["names"].as_array().unwrap() {
        assert_eq!(
            serde_json::to_value(template_variants::names(
                c["label"].as_str().unwrap(),
                c["index"].as_i64().unwrap()
            ))
            .unwrap(),
            c["expected"]
        );
    }
    for i in 0..f["cases"].as_array().unwrap().len() {
        let mut d = doc();
        let selected = pairs(i)
            .into_iter()
            .flat_map(|p| p.normal.into_iter().chain(p.reverse))
            .collect::<std::collections::HashSet<_>>();
        let mut moving = false;
        let retained = d
            .records()
            .filter_map(|r| {
                if !matches!(r.kind(), "ATTRIB" | "VERTEX" | "SEQEND") {
                    moving = r.handle().is_some_and(|h| selected.contains(&h));
                }
                if moving || matches!(r.kind(), "TABLE" | "LAYER") {
                    return None;
                }
                r.handle().map(|h| {
                    (
                        h,
                        r.tags().map(|(c, v)| (c, v.to_owned())).collect::<Vec<_>>(),
                    )
                })
            })
            .collect::<Vec<_>>();
        let original = d
            .records()
            .filter_map(|r| r.handle())
            .collect::<std::collections::HashSet<_>>();
        let names = if f["cases"][i]["automatic"] == true {
            template_variants::assemble_automatic(
                &mut d,
                &pairs(i),
                &mut |_| Ok(sleufbase_rust::dxf_text_metrics::Font::monospace()),
                &AtomicBool::new(false),
            )
            .unwrap()
        } else {
            template_variants::assemble(&mut d, &pairs(i), &AtomicBool::new(false)).unwrap()
        };
        assert_eq!(
            serde_json::to_value(&names).unwrap(),
            f["cases"][i]["wrappers"]
        );
        let parsed = Document::from_bytes(bytes(&d), &AtomicBool::new(false)).unwrap();
        for (h, tags) in retained {
            assert_eq!(
                parsed
                    .by_handle(&format!("{h:X}"))
                    .unwrap()
                    .unwrap()
                    .tags()
                    .map(|(c, v)| (c, v.to_owned()))
                    .collect::<Vec<_>>(),
                tags,
                "Shared resource/annotation {h:X} changed"
            );
        }
        let mut dictionaries = 0;
        for r in parsed
            .records()
            .filter(|r| r.handle().is_some_and(|h| !original.contains(&h)))
        {
            dictionaries += usize::from(r.kind() == "DICTIONARY");
            for (c, v) in r
                .tags()
                .filter(|(c, _)| matches!(c,320..=369|390..=399|480|481|1005))
            {
                let h = u64::from_str_radix(v.trim(), 16).unwrap();
                assert!(
                    h == 0 || parsed.by_handle(v.trim()).unwrap().is_some(),
                    "Dangling {c}: {v}"
                );
            }
        }
        assert_eq!(dictionaries, names.len());
        let mut parent = None;
        for r in parsed.records().filter(|r| r.section() == Some("BLOCKS")) {
            if matches!(r.kind(), "ATTRIB" | "VERTEX" | "SEQEND") {
                if let Some(p) = parent {
                    assert_eq!(
                        u64::from_str_radix(r.first(330).unwrap().trim(), 16).unwrap(),
                        p
                    );
                }
            } else {
                parent = r.handle().filter(|h| selected.contains(h));
            }
        }
    }
}
#[test]
fn invalid_groups_and_late_cancellation_or_panic_preserve_staged_source_edits() {
    let mut source = doc();
    let text = source
        .records()
        .find(|r| r.kind() == "ATTRIB")
        .unwrap()
        .handle()
        .unwrap();
    source
        .replace_text(&format!("{text:X}"), "Nog niet opgeslagen Ø")
        .unwrap();
    let initial = bytes(&source);
    let valid = pairs(1);
    for case in 0..5 {
        let mut request = valid.clone();
        let first = request[0].normal[0];
        match case {
            0 => request[1].reverse.push(u64::MAX - 1),
            1 => request[1].normal.push(first),
            2 => request[1].normal = vec![text],
            3 => request[1].selector = Some([f64::NAN, 1., 0.]),
            _ => request.push(request[0].clone()),
        }
        assert!(
            template_variants::assemble(&mut source, &request, &AtomicBool::new(false)).is_err()
        );
        assert_eq!(bytes(&source), initial);
    }
    let cancel = AtomicBool::new(false);
    assert!(
        template_variants::assemble_with_progress(&mut source, &valid, &cancel, |n, _| {
            if n == 1 {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .is_err()
    );
    assert_eq!(bytes(&source), initial);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        template_variants::assemble_with_progress(
            &mut source,
            &valid,
            &AtomicBool::new(false),
            |n, _| {
                if n == 1 {
                    panic!("observer failed");
                }
            },
        )
    }));
    assert!(panic.is_err());
    assert_eq!(bytes(&source), initial);
}
#[test]
fn automatic_selector_font_failure_cancel_and_panic_keep_staged_source_edits() {
    use sleufbase_rust::dxf_text_metrics::Font;
    let mut source = doc();
    let h = source
        .records()
        .find(|r| r.kind() == "ATTRIB")
        .unwrap()
        .handle()
        .unwrap();
    source
        .replace_text(&format!("{h:X}"), "Gewijzigde langere profieltekst")
        .unwrap();
    let initial = bytes(&source);
    let request = pairs(5);
    let cancel = AtomicBool::new(false);
    assert!(template_variants::assemble_automatic(
        &mut source,
        &request,
        &mut |_| Err("Font ontbreekt".into()),
        &cancel
    )
    .is_err());
    assert_eq!(bytes(&source), initial);
    assert!(template_variants::assemble_automatic(
        &mut source,
        &request,
        &mut |_| {
            cancel.store(true, Ordering::Relaxed);
            Ok(Font::monospace())
        },
        &cancel
    )
    .is_err());
    assert_eq!(bytes(&source), initial);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        template_variants::assemble_automatic(
            &mut source,
            &request,
            &mut |_| panic!("font callback failed"),
            &AtomicBool::new(false),
        )
    }));
    assert!(result.is_err());
    assert_eq!(bytes(&source), initial);
}
#[test]
fn missing_donor_and_exhausted_handles_fail_without_mutating_the_drawing() {
    let input = include_str!("fixtures/template-variants/input.dxf");
    let changed = input.replace("BLOCKVISIBILITYPARAMETER", "UNSUPPORTED_PARAMETER");
    let mut d = Document::from_bytes(changed.into_bytes(), &AtomicBool::new(false)).unwrap();
    let original = bytes(&d);
    assert!(template_variants::assemble(&mut d, &pairs(0), &AtomicBool::new(false)).is_err());
    assert_eq!(bytes(&d), original);
    let mut lines = input.lines().map(str::to_owned).collect::<Vec<_>>();
    let at = lines.iter().position(|l| l == "$HANDSEED").unwrap();
    lines[at + 2] = "FFFFFFFFFFFFFFFF".into();
    let mut d = Document::from_bytes(
        (lines.join("\n") + "\n").into_bytes(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let original = bytes(&d);
    assert!(template_variants::assemble(&mut d, &pairs(0), &AtomicBool::new(false)).is_err());
    assert_eq!(bytes(&d), original);
}
#[test]
fn actual_cad_template_keeps_all_original_objects_and_classes_when_the_pair_is_installed() {
    use sleufbase_rust::template_geometry::{self, Entity, Paint, Shape};
    use std::{
        collections::HashMap,
        hash::{Hash, Hasher},
    };
    let cancel = AtomicBool::new(false);
    let mut d = Document::load(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/cadastral_template.dxf"),
        &cancel,
    )
    .unwrap();
    let signature = |r: sleufbase_rust::dxf_document::RecordView<'_>| {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for t in r.tags() {
            t.hash(&mut hash);
        }
        hash.finish()
    };
    let original = d
        .records()
        .filter(|r| r.kind() != "TABLE")
        .filter_map(|r| r.handle().map(|h| (h, signature(r))))
        .collect::<HashMap<_, _>>();
    let classes = d
        .records()
        .filter(|r| r.kind() == "CLASS")
        .map(signature)
        .collect::<Vec<_>>();
    template_geometry::add(
        &mut d,
        &[],
        &[
            Entity {
                layer: "0".into(),
                layout: None,
                paint: Paint::default(),
                shape: Shape::Polyline {
                    vertices: vec![[1., 2.], [4., 5.]],
                    closed: false,
                },
            },
            Entity {
                layer: "0".into(),
                layout: None,
                paint: Paint::default(),
                shape: Shape::Circle {
                    center: [3., 4.],
                    radius: 1.,
                },
            },
        ],
        &cancel,
    )
    .unwrap();
    let mut d = Document::from_bytes(bytes(&d), &cancel).unwrap();
    let new = d
        .records()
        .filter(|r| r.section() == Some("ENTITIES"))
        .filter_map(|r| r.handle().filter(|h| !original.contains_key(h)))
        .collect::<Vec<_>>();
    assert_eq!(new.len(), 2);
    template_variants::assemble(
        &mut d,
        &[Pair {
            label: "PS_NATIVE_REAL".into(),
            slot_index: 1,
            normal: vec![new[0]],
            reverse: vec![new[1]],
            selector: Some([6., 7., 0.]),
        }],
        &cancel,
    )
    .unwrap();
    let parsed = Document::from_bytes(bytes(&d), &cancel).unwrap();
    for (handle, hash) in &original {
        assert_eq!(
            signature(parsed.by_handle(&format!("{handle:X}")).unwrap().unwrap()),
            *hash,
            "Original object {handle:X} changed"
        );
    }
    assert_eq!(
        parsed
            .records()
            .filter(|r| r.kind() == "CLASS")
            .map(signature)
            .collect::<Vec<_>>(),
        classes
    );
    let parameter = parsed
        .records()
        .find(|r| r.kind() == "BLOCKVISIBILITYPARAMETER" && r.first(301) == Some("Versie"))
        .unwrap();
    assert_eq!(
        parameter
            .tags()
            .filter(|(c, _)| *c == 303)
            .map(|(_, v)| v.to_owned())
            .collect::<Vec<_>>(),
        ["Normaal", "Reverse"]
    );
}
