//! Python's JSON-property truth/str/repr and float coercion, without a runtime.
use serde_json::Value;
mod unicode {
    include!("python_repr_unicode.rs");
    include!("python_case_unicode.rs");
}
fn mapped_case(value: &str, table: &[(char, &str)]) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match table.binary_search_by_key(&ch, |(c, _)| *c) {
            Ok(i) => out.push_str(table[i].1),
            Err(_) => out.push(ch),
        }
    }
    out
}
pub(crate) fn uppercase(value: &str) -> String {
    if value.is_ascii() {
        value.to_ascii_uppercase()
    } else {
        mapped_case(value, unicode::UPPER)
    }
}
pub(crate) fn casefold(value: &str) -> String {
    if value.is_ascii() {
        value.to_ascii_lowercase()
    } else {
        mapped_case(value, unicode::CASEFOLD)
    }
}
const MAX_PROPERTY: usize = 1024 * 1024;
pub(crate) fn trim(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'))
}
pub(crate) fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(n) => n.as_f64() != Some(0.),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}
fn float_text(number: f64) -> String {
    if number.is_infinite() {
        return if number.is_sign_negative() {
            "-inf"
        } else {
            "inf"
        }
        .into();
    }
    // Ryū resolves equally short decimal choices with ties-to-even, like
    // Python repr; Rust Debug can choose the other decimal at exact ties.
    let mut buffer = ryu::Buffer::new();
    let text = buffer.format(number);
    let result = if number != 0. && number.abs() < 1e-4 && !text.contains('e') {
        let fraction = text.split_once('.').expect("small finite decimal").1;
        let at = fraction.find(|c| c != '0').expect("nonzero decimal");
        let digits = &fraction[at..];
        let mantissa = if digits.len() == 1 {
            digits.into()
        } else {
            format!("{}.{}", &digits[..1], &digits[1..])
        };
        format!(
            "{}{mantissa}e-{:02}",
            if number.is_sign_negative() { "-" } else { "" },
            at + 1
        )
    } else {
        text.into()
    };
    if let Some((mantissa, exponent)) = result.split_once('e') {
        let exponent: i32 = exponent.parse().expect("formatted float exponent");
        format!(
            "{mantissa}e{}{:02}",
            if exponent < 0 { '-' } else { '+' },
            exponent.unsigned_abs()
        )
    } else {
        result
    }
}
fn append(out: &mut String, text: &str) -> Result<(), String> {
    if text.len() > MAX_PROPERTY.saturating_sub(out.len()) {
        return Err("Objecteigenschap is groter dan 1 MiB".into());
    }
    out.try_reserve(text.len())
        .map_err(|_| "Onvoldoende geheugen voor objecteigenschap")?;
    out.push_str(text);
    Ok(())
}
fn printable(ch: char) -> bool {
    let n = ch as u32;
    unicode::PRINTABLE
        .get(unicode::PRINTABLE.partition_point(|(_, b)| *b < n))
        .is_some_and(|(a, _)| *a <= n)
}
fn repr(value: &Value, out: &mut String) -> Result<(), String> {
    match value {
        Value::Null => append(out, "None")?,
        Value::Bool(b) => append(out, if *b { "True" } else { "False" })?,
        Value::Number(n) => {
            let raw = n.to_string();
            if raw.contains(['.', 'e', 'E']) {
                append(
                    out,
                    &float_text(n.as_f64().unwrap_or(if raw.starts_with('-') {
                        f64::NEG_INFINITY
                    } else {
                        f64::INFINITY
                    })),
                )?;
            } else {
                append(out, if raw == "-0" { "0" } else { &raw })?;
            }
        }
        Value::String(s) => {
            let quote = if s.contains('\'') && !s.contains('"') {
                '"'
            } else {
                '\''
            };
            append(out, &quote.to_string())?;
            for ch in s.chars() {
                match ch {
                    '\\' => append(out, "\\\\")?,
                    '\n' => append(out, "\\n")?,
                    '\r' => append(out, "\\r")?,
                    '\t' => append(out, "\\t")?,
                    c if c == quote => {
                        append(out, "\\")?;
                        append(out, &c.to_string())?;
                    }
                    c if !printable(c) => {
                        let n = c as u32;
                        append(
                            out,
                            &if n <= 0xff {
                                format!("\\x{n:02x}")
                            } else if n <= 0xffff {
                                format!("\\u{n:04x}")
                            } else {
                                format!("\\U{n:08x}")
                            },
                        )?;
                    }
                    c => {
                        let mut buffer = [0; 4];
                        append(out, c.encode_utf8(&mut buffer))?;
                    }
                }
            }
            append(out, &quote.to_string())?;
        }
        Value::Array(items) => {
            append(out, "[")?;
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    append(out, ", ")?;
                }
                repr(item, out)?;
            }
            append(out, "]")?;
        }
        Value::Object(items) => {
            append(out, "{")?;
            for (i, (key, value)) in items.iter().enumerate() {
                if i > 0 {
                    append(out, ", ")?;
                }
                repr(&Value::String(key.clone()), out)?;
                append(out, ": ")?;
                repr(value, out)?;
            }
            append(out, "}")?;
        }
    }
    Ok(())
}
pub(crate) fn string(value: Option<&Value>) -> Result<String, String> {
    match value {
        None => Ok(String::new()),
        Some(Value::String(s)) => {
            let value = trim(s);
            let mut out = String::new();
            append(&mut out, value)?;
            Ok(out)
        }
        Some(value) => {
            let mut out = String::new();
            repr(value, &mut out)?;
            Ok(trim(&out).into())
        }
    }
}
pub(crate) fn coordinate(value: Option<&Value>) -> Option<f64> {
    number(value).filter(|n| n.is_finite())
}
pub(crate) fn number(value: Option<&Value>) -> Option<f64> {
    let number = match value? {
        Value::Number(n) => n.as_f64().or_else(|| n.to_string().parse().ok())?,
        Value::Bool(v) => {
            if *v {
                1.
            } else {
                0.
            }
        }
        Value::String(s) => {
            let text = s.trim();
            let needs_translation = text.chars().any(|c| !c.is_ascii() || c == '_');
            if needs_translation {
                let translated: String = text
                    .chars()
                    .map(|c| {
                        crate::object_layout::decimal(c).map_or(c, |n| char::from(b'0' + n as u8))
                    })
                    .collect();
                let bytes = translated.as_bytes();
                for (i, &c) in bytes.iter().enumerate() {
                    if c == b'_'
                        && (i == 0
                            || i + 1 == bytes.len()
                            || !bytes[i - 1].is_ascii_digit()
                            || !bytes[i + 1].is_ascii_digit())
                    {
                        return None;
                    }
                }
                translated.replace('_', "").parse::<f64>().ok()?
            } else {
                text.parse::<f64>().ok()?
            }
        }
        _ => return None,
    };
    Some(number)
}
