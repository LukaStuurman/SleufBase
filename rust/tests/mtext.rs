use serde::Deserialize;
use serde_json::Value;
use sleufbase_rust::mtext::{self, Content, Context, Error, FontFace};
use std::sync::{atomic::AtomicBool, Arc};

#[derive(Deserialize)]
struct Request {
    content: String,
    initial: Option<Context>,
    properties: bool,
}
pub fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert!(
                (a - b).abs() <= 1e-12 + 8. * f64::EPSILON * a.abs().max(b.abs()),
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
            for (key, b) in b {
                compare(
                    a.get(key)
                        .unwrap_or_else(|| panic!("{path}: missing {key}")),
                    b,
                    &format!("{path}.{key}"),
                );
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}

#[test]
fn original_parser_tokens_contexts_properties_fonts_tabs_and_unicode_digits_match() {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/mtext/parser.json")).unwrap();
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 976);
    let mut tokens = 0;
    let mut errors = 0;
    for (i, case) in cases.iter().enumerate() {
        let r: Request = serde_json::from_value(case["input"].clone()).unwrap();
        let result = mtext::parse(
            &r.content,
            r.initial.unwrap_or_default(),
            r.properties,
            &AtomicBool::new(false),
        );
        if let Some(expected) = case.get("tokens") {
            let result = result.unwrap_or_else(|e| panic!("case {i} / {:?}: {e}", r.content));
            tokens += result.len();
            compare(
                &serde_json::to_value(&result).unwrap(),
                expected,
                &format!("case {i} / {:?}", r.content),
            );
        } else {
            assert!(
                result.is_err(),
                "case {i} / {:?}: expected guard/error",
                r.content
            );
            errors += 1;
        }
    }
    assert_eq!(tokens, 3467);
    assert_eq!(errors, 14);
}
#[test]
fn token_contexts_share_fonts_paragraphs_and_unchanged_state_instead_of_cloning_large_names() {
    let family: Arc<str> = Arc::from("F".repeat(32768));
    let context = Context {
        font_face: Arc::new(FontFace {
            family: family.clone(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let text = "word ".repeat(50_000);
    let tokens = mtext::parse(&text, context, false, &AtomicBool::new(false)).unwrap();
    assert_eq!(tokens.len(), 100_000);
    for t in &tokens {
        assert!(Arc::ptr_eq(&t.context, &tokens[0].context));
        assert!(Arc::ptr_eq(&t.context.font_face.family, &family));
    }
    let tokens = mtext::parse(
        "one \\H2;two \\W.5;three",
        (*tokens[0].context).clone(),
        true,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(!Arc::ptr_eq(
        &tokens[0].context,
        &tokens.last().unwrap().context
    ));
    assert!(Arc::ptr_eq(
        &tokens[0].context.font_face,
        &tokens.last().unwrap().context.font_face
    ));
    assert!(Arc::ptr_eq(
        &tokens[0].context.paragraph,
        &tokens.last().unwrap().context.paragraph
    ));
    assert_eq!(tokens[0].context.cap_height, 1.);
    assert_eq!(tokens.last().unwrap().context.cap_height, 2.);
}
#[test]
fn cancellation_limits_and_nonfinite_values_are_typed_and_publish_no_partial_result() {
    let parse = |s: &str| mtext::parse(s, Context::default(), false, &AtomicBool::new(false));
    assert_eq!(
        mtext::parse("text", Context::default(), false, &AtomicBool::new(true)),
        Err(Error::Cancelled)
    );
    for s in [
        "x".repeat(1_048_577),
        "{".repeat(129),
        " ".repeat(200_001),
        "\\H1;".repeat(100_001),
    ] {
        assert!(matches!(parse(&s), Err(Error::Limit(_))));
    }
    for s in [
        "\\H1e309;text",
        "\\Q-1e309;text",
        "\\H1e308;\\H1e308x;text",
        "\\Sfinal\\",
    ] {
        assert!(matches!(parse(s), Err(Error::Invalid(_))), "{s}");
    }
    assert!(matches!(
        mtext::parse(
            "",
            Context {
                cap_height: f64::NAN,
                ..Default::default()
            },
            false,
            &AtomicBool::new(false)
        ),
        Err(Error::Invalid(_))
    ));
}
#[test]
fn scoped_strokes_restore_snapshots_but_follow_original_continuation_state() {
    let t = mtext::parse(
        "{\\Linner}outer\\H2;after",
        Context::default(),
        false,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(t.len(), 3);
    assert_eq!(t[0].content, Content::Word("inner".into()));
    assert_eq!(t[0].context.stroke, 1);
    assert_eq!(t[1].context.stroke, 0);
    assert!(!t[1].context.continue_stroke);
    assert_eq!(t[2].context.stroke, 0);
    assert!(t[2].context.continue_stroke);
}

#[test]
fn large_tab_arrays_stay_shared_across_height_and_paragraph_changes() {
    let source = format!(
        "\\pt{};{}",
        "1,".repeat(20_000),
        "\\H2;word\\pi1;other".repeat(2000)
    );
    let tokens = mtext::parse(&source, Context::default(), false, &AtomicBool::new(false)).unwrap();
    assert_eq!(tokens.len(), 4000);
    assert_eq!(tokens[0].context.paragraph.tab_stops.len(), 20_000);
    for token in &tokens {
        assert!(Arc::ptr_eq(
            &token.context.paragraph.tab_stops,
            &tokens[0].context.paragraph.tab_stops
        ));
    }
}
