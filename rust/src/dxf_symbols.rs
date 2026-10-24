//! ezdxf's Unicode-14 table-name lower(), not full case folding. Distinct
//! Straße/STRASSE and final/medial Greek sigma names must stay distinct.
include!("dxf_symbol_unicode.rs");
fn in_ranges(c: char, ranges: &[(u32, u32)]) -> bool {
    let code = c as u32;
    ranges
        .get(ranges.partition_point(|(_, end)| *end < code))
        .is_some_and(|(start, _)| *start <= code)
}
pub(crate) fn key(value: &str) -> String {
    if value.is_ascii() {
        return value.to_ascii_lowercase();
    }
    let mut output = String::with_capacity(value.len());
    let mut before_cased = false;
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == 'Σ'
            && before_cased
            && !chars
                .clone()
                .find(|c| !in_ranges(*c, CASE_IGNORABLE))
                .is_some_and(|c| in_ranges(c, CASED))
        {
            output.push('ς');
        } else if let Ok(i) = LOWER.binary_search_by_key(&ch, |(c, _)| *c) {
            output.push_str(LOWER[i].1);
        } else {
            output.push(ch);
        }
        if !in_ranges(ch, CASE_IGNORABLE) {
            before_cased = in_ranges(ch, CASED);
        }
    }
    output
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn official_unicode14_symbol_keys_match_128358_context_probes() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/template-preparation/symbol-case.json"
        ))
        .unwrap();
        let mut chars = LOWER.iter().map(|(c, _)| *c).collect::<Vec<_>>();
        let mut seed = 73665u64;
        for _ in 0..20000 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            if let Some(c) = char::from_u32(((seed >> 32) % 0x110000) as u32) {
                chars.push(c);
            }
        }
        let mut h = 0xcbf29ce484222325u64;
        let mut count = 0;
        for ch in chars {
            for value in [
                ch.to_string(),
                format!("A{ch}Σ"),
                format!(".{ch}Σ"),
                format!("AΣ{ch}"),
                format!("AΣ{ch}A"),
                format!("ΟΣ{ch}"),
            ] {
                for byte in key(&value).bytes().chain([255]) {
                    h = (h ^ byte as u64).wrapping_mul(0x100000001b3);
                }
                count += 1;
            }
        }
        assert_eq!(count, fixture["count"].as_u64().unwrap());
        assert_eq!(format!("{h:016x}"), fixture["hash"].as_str().unwrap());
        assert_ne!(key("Straße"), key("STRASSE"));
        assert_ne!(key("ΟΣ"), key("οσ"));
    }
}
