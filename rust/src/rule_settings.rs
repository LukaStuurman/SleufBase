//! Owned, ordered object rules shared by settings, rendering and exports.
use crate::disciplines::{Rule, RULES};
use serde_json::Value;
pub fn defaults() -> Vec<Rule> {
    // AppSettings differs from the bare exporter: 13 rules and an ONBEKEND
    // profile label for monitoring. Keep the library exporter defaults intact.
    let mut rules = RULES.to_vec();
    rules[7].label = "ONBEKEND".into();
    rules.push(Rule::new(
        vec!["warm".into()],
        "B-WE-KL-WARMTENET-G".into(),
        80,
        "Warmtenet".into(),
    ));
    rules.push(Rule::new(
        vec!["hs".into()],
        "B-WE-KL-ET_HS-G".into(),
        1,
        "Hoogspanning".into(),
    ));
    rules
}
pub fn validate(rules: &[Rule]) -> Result<(), String> {
    if rules.len() > 256 {
        return Err("Maximaal 256 objectregels toegestaan".into());
    }
    for (i, rule) in rules.iter().enumerate() {
        let context = |error| format!("Objectregel {}: {error}", i + 1);
        crate::object_dxf::layer_name(&rule.target_layer).map_err(context)?;
        if rule.keywords.is_empty()
            || rule
                .keywords
                .iter()
                .any(|k| crate::python_values::trim(k).is_empty())
        {
            return Err(context("Vul minstens één niet-lege codering in".into()));
        }
        if rule.keywords.len() > 256
            || rule.keywords.iter().map(|s| s.len()).sum::<usize>() > 32 * 1024
            || rule.label.len() > 32 * 1024
        {
            return Err(context("Coderingen of label zijn te groot".into()));
        }
        if !(1..=256).contains(&rule.color_aci) {
            return Err(context("ACI-kleur moet tussen 1 en 256 liggen".into()));
        }
    }
    Ok(())
}
/// Missing keeps defaults; an explicit empty list disables all rules.
pub fn from_legacy(value: Option<&Value>) -> Result<Vec<Rule>, String> {
    let Some(value) = value else {
        return Ok(defaults());
    };
    let rows = value
        .as_array()
        .ok_or("Objectregels moeten een lijst zijn")?;
    if rows.len() > 256 {
        return Err("Maximaal 256 objectregels toegestaan".into());
    }
    let mut rules = Vec::new();
    for row in rows {
        let (words, layer, color, label) = match row {
            Value::Array(items) if items.len() >= 3 => {
                (items.first(), items.get(1), items.get(2), items.get(3))
            }
            Value::Object(items) => (
                items.get("keywords"),
                items.get("target_layer"),
                items.get("color"),
                items.get("profile_label"),
            ),
            _ => continue,
        };
        let words = crate::python_values::string(words)?;
        let keywords: Vec<String> = words
            .split([',', ';', '\n'])
            .map(crate::python_values::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        let layer = crate::python_values::string(layer)?;
        if keywords.is_empty() || layer.is_empty() {
            continue;
        }
        rules.push(Rule::new(
            keywords,
            layer,
            color_value(color),
            crate::python_values::string(label)?,
        ));
    }
    validate(&rules)?;
    Ok(rules)
}
fn color_value(value: Option<&Value>) -> u16 {
    match value {
        Some(Value::Bool(b)) => u16::from(*b).max(1),
        Some(Value::Number(n)) => match n.as_f64() {
            Some(v) => v.trunc().clamp(1., 256.) as u16,
            None => {
                let raw = n.to_string();
                if raw.starts_with('-') && !raw.contains(['.', 'e', 'E']) {
                    1
                } else {
                    256
                }
            }
        },
        Some(Value::String(s)) => {
            // Python int/float accept Unicode whitespace but reject the four
            // C0 separators additionally stripped by Python str.strip.
            let s = s.trim();
            let (negative, digits) = match s.as_bytes().first() {
                Some(b'-') => (true, &s[1..]),
                Some(b'+') => (false, &s[1..]),
                _ => (false, s),
            };
            let chars: Vec<_> = digits.chars().collect();
            let mut total = 0u16;
            for (i, &ch) in chars.iter().enumerate() {
                if ch == '_'
                    && i > 0
                    && i + 1 < chars.len()
                    && crate::object_layout::decimal(chars[i - 1]).is_some()
                    && crate::object_layout::decimal(chars[i + 1]).is_some()
                {
                    continue;
                }
                let Some(d) = crate::object_layout::decimal(ch) else {
                    return 256;
                };
                total = total.saturating_mul(10).saturating_add(d as u16);
            }
            if chars.is_empty() {
                256
            } else if negative {
                1
            } else {
                total.clamp(1, 256)
            }
        }
        _ => 256,
    }
}
