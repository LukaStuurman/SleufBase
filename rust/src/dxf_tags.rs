//! Incremental ASCII/binary DXF tags; no intermediate vector for the whole file.
//! Binary scalar layouts follow Autodesk's DXF Group Code Value Types Reference.
use encoding_rs::{Encoding, UTF_8, WINDOWS_1252};
use std::borrow::Cow;

pub(crate) const SIGNATURE: &[u8] = b"AutoCAD Binary DXF\r\n\x1a\0";
const MAX_VALUE_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
pub(crate) struct Group<'a> {
    pub code: i32,
    pub value: Cow<'a, str>,
}

pub(crate) fn ascii(text: &str) -> impl Iterator<Item = Result<Group<'_>, String>> {
    let mut lines = text.trim_start_matches('\u{feff}').lines();
    std::iter::from_fn(move || {
        let code = lines.next()?;
        Some((|| {
            let value = lines
                .next()
                .ok_or("DXF mist een waarde bij de laatste groepcode")?;
            let code = code
                .trim()
                .parse::<i32>()
                .map_err(|_| "Ongeldige DXF-groepcode")?;
            if !(0..=1071).contains(&code) || value.len() > MAX_VALUE_BYTES {
                return Err("DXF-groepcode/waardelengte buiten limiet".into());
            }
            if matches!(code,10..=59|110..=149|210..=239|460..=469|1010..=1059)
                && !value.trim().parse::<f64>().is_ok_and(|v| v.is_finite())
            {
                return Err("DXF bevat ongeldig of niet-eindig getal".into());
            }
            let integer_range = match code {
                60..=79 | 170..=179 | 270..=289 | 370..=389 | 400..=409 | 1060..=1070 => {
                    Some((i64::from(i16::MIN), i64::from(u16::MAX)))
                }
                90..=99 | 420..=429 | 440..=459 | 1071 => {
                    Some((i64::from(i32::MIN), i64::from(u32::MAX)))
                }
                160..=169 => Some((i64::MIN, i64::MAX)),
                290..=299 => Some((0, 1)),
                _ => None,
            };
            if let Some((min, max)) = integer_range {
                if !value
                    .trim()
                    .parse::<i64>()
                    .is_ok_and(|v| (min..=max).contains(&v))
                {
                    return Err("DXF bevat ongeldige integer of booleaanse waarde".into());
                }
            }
            Ok(Group {
                code,
                value: Cow::Borrowed(value),
            })
        })())
    })
}

fn codepage(name: &str) -> Result<&'static Encoding, String> {
    let name = name.trim().to_ascii_uppercase();
    let label = match name.as_str() {
        "ANSI_932" | "DOS932" => "shift_jis".into(),
        "ANSI_936" => "gbk".into(),
        "ANSI_949" => "euc-kr".into(),
        "ANSI_950" | "BIG5" => "big5".into(),
        "ANSI_874" => "windows-874".into(),
        "UTF-8" | "UTF8" => "utf-8".into(),
        name if name.starts_with("ANSI_125") => format!("windows-{}", &name[5..]),
        _ => return Err(format!("DXF-codepagina wordt nog niet ondersteund: {name}")),
    };
    Encoding::for_label(label.as_bytes()).ok_or_else(|| format!("Ongeldige DXF-codepagina: {name}"))
}

fn decode<'a>(bytes: &'a [u8], encoding: &'static Encoding) -> Result<Cow<'a, str>, String> {
    let (text, errors) = encoding.decode_without_bom_handling(bytes);
    if errors {
        return Err(format!("Ongeldige DXF-tekst voor {}", encoding.name()));
    }
    Ok(text)
}

/// Inspect HEADER values without decoding arbitrary entity data or replacing
/// invalid bytes. R2007+ uses UTF-8; earlier files use the declared codepage.
pub(crate) fn text(bytes: &[u8]) -> Result<Cow<'_, str>, String> {
    if bytes.starts_with(b"\xef\xbb\xbf") {
        return decode(&bytes[3..], UTF_8);
    }
    let mut lines = bytes.split(|b| *b == b'\n');
    let mut version = None;
    let mut page = None;
    let mut key = "";
    let mut section = "";
    let mut next_section = false;
    while let Some(code) = lines.next() {
        let Some(value) = lines.next() else {
            break;
        };
        let code = std::str::from_utf8(code).unwrap_or("").trim();
        let value = std::str::from_utf8(value).unwrap_or("").trim();
        if code == "0" {
            if value == "SECTION" {
                next_section = true;
            } else if value == "ENDSEC" && section == "HEADER" {
                break;
            }
        } else if next_section && code == "2" {
            section = value;
            next_section = false;
            if section != "HEADER" {
                break;
            }
        } else if section == "HEADER" {
            if code == "9" {
                key = value;
            } else if key == "$ACADVER" && code == "1" {
                version = Some(value);
            } else if key == "$DWGCODEPAGE" && code == "3" {
                page = Some(value);
            }
        }
    }
    let encoding = if version.is_some_and(|v| v >= "AC1021") {
        UTF_8
    } else {
        page.map(codepage).transpose()?.unwrap_or(WINDOWS_1252)
    };
    decode(bytes, encoding)
}

pub(crate) struct Binary<'a> {
    bytes: &'a [u8],
    offset: usize,
    wide_codes: bool,
    encoding: &'static Encoding,
    utf8: bool,
    key: String,
    page: Option<String>,
    header: bool,
    next_section: bool,
    failed: bool,
}
impl<'a> Binary<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, String> {
        if !bytes.starts_with(SIGNATURE) {
            return Err("Ongeldige binaire DXF-signatuur".into());
        }
        let first = bytes.get(SIGNATURE.len()..).ok_or("Lege binaire DXF")?;
        // Binary files start with group 0/SECTION. Distinguish one-byte codes
        // (R12) from two-byte codes by the actual encoding, not a version guess.
        if first.len() < 2 || first[0] != 0 {
            return Err("Binaire DXF mist eerste SECTION-groep".into());
        }
        Ok(Self {
            bytes,
            offset: SIGNATURE.len(),
            wide_codes: first[1] == 0,
            encoding: WINDOWS_1252,
            utf8: false,
            key: String::new(),
            page: None,
            header: false,
            next_section: false,
            failed: false,
        })
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or("DXF-bytepositie te groot")?;
        let data = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| format!("Binaire DXF afgebroken bij byte {}", self.offset))?;
        self.offset = end;
        Ok(data)
    }
    fn group(&mut self) -> Result<Group<'a>, String> {
        let code = if self.wide_codes {
            u16::from_le_bytes(self.take(2)?.try_into().unwrap())
        } else {
            let first = self.take(1)?[0];
            if first == 255 {
                u16::from_le_bytes(self.take(2)?.try_into().unwrap())
            } else {
                u16::from(first)
            }
        };
        let value: Cow<'a, str> = match code {
            10..=59 | 110..=149 | 210..=239 | 460..=469 | 1010..=1059 => {
                let value = f64::from_le_bytes(self.take(8)?.try_into().unwrap());
                if !value.is_finite() {
                    return Err("Binaire DXF bevat niet-eindig getal".into());
                }
                value.to_string().into()
            }
            60..=79 | 170..=179 | 270..=289 | 370..=389 | 400..=409 | 1060..=1070 => {
                i16::from_le_bytes(self.take(2)?.try_into().unwrap())
                    .to_string()
                    .into()
            }
            90..=99 | 420..=429 | 440..=459 | 1071 => {
                i32::from_le_bytes(self.take(4)?.try_into().unwrap())
                    .to_string()
                    .into()
            }
            160..=169 => i64::from_le_bytes(self.take(8)?.try_into().unwrap())
                .to_string()
                .into(),
            290..=299 => {
                let value = self.take(1)?[0];
                if value > 1 {
                    return Err("Binaire DXF bevat ongeldige booleaanse waarde".into());
                }
                value.to_string().into()
            }
            310..=319 | 1004 => {
                let count = usize::from(self.take(1)?[0]);
                let data = self.take(count)?;
                const HEX: &[u8] = b"0123456789ABCDEF";
                let mut hex = String::with_capacity(data.len() * 2);
                for byte in data {
                    hex.push(HEX[(byte >> 4) as usize] as char);
                    hex.push(HEX[(byte & 15) as usize] as char);
                }
                hex.into()
            }
            0..=9
            | 100..=102
            | 105
            | 300..=309
            | 320..=369
            | 390..=399
            | 410..=419
            | 430..=439
            | 470..=481
            | 1000..=1003
            | 1005..=1009 => {
                let tail = &self.bytes[self.offset..];
                let end = tail
                    .iter()
                    .take(MAX_VALUE_BYTES + 1)
                    .position(|b| *b == 0)
                    .ok_or("Binaire DXF mist stringafsluiting of tekst is te lang")?;
                let data = self.take(end + 1)?;
                decode(&data[..end], self.encoding)?
            }
            _ => return Err(format!("Niet-ondersteunde binaire DXF-groepcode {code}")),
        };
        if code == 0 {
            self.key.clear();
            if value == "SECTION" {
                self.next_section = true;
            } else if value == "ENDSEC" {
                if self.header && !self.utf8 {
                    self.encoding = self
                        .page
                        .as_deref()
                        .map(codepage)
                        .transpose()?
                        .unwrap_or(WINDOWS_1252);
                }
                self.header = false;
            }
        } else if self.next_section && code == 2 {
            self.header = value == "HEADER";
            self.next_section = false;
        } else if self.header {
            if code == 9 {
                self.key = value.to_string();
            } else if self.key == "$ACADVER" && code == 1 {
                self.utf8 = value.as_ref() >= "AC1021";
                if self.utf8 {
                    self.encoding = UTF_8;
                }
            } else if self.key == "$DWGCODEPAGE" && code == 3 {
                self.page = Some(value.to_string());
            }
        }
        Ok(Group {
            code: i32::from(code),
            value,
        })
    }
}
impl<'a> Iterator for Binary<'a> {
    type Item = Result<Group<'a>, String>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.failed || self.offset == self.bytes.len() {
            return None;
        }
        let result = self.group();
        if result.is_err() {
            self.failed = true;
        }
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ascii_integers_and_boolean_flags_are_validated_before_entity_interpretation() {
        for (code, value, valid) in [
            (70, "NaN", false),
            (70, "65535", true),
            (70, "65536", false),
            (90, "4294967295", true),
            (90, "4294967296", false),
            (160, "-9223372036854775808", true),
            (160, "9223372036854775808", false),
            (290, "2", false),
            (290, "01", true),
        ] {
            let text = format!("{code}\n{value}\n");
            assert_eq!(
                ascii(&text).next().unwrap().is_ok(),
                valid,
                "{code}: {value}"
            );
        }
    }
    #[test]
    fn modern_utf8_overrides_header_codepage_regardless_of_tag_order() {
        let mut bytes = SIGNATURE.to_vec();
        for (code, value) in [
            (0u16, "SECTION"),
            (2, "HEADER"),
            (9, "$DWGCODEPAGE"),
            (3, "ANSI_1361"),
            (9, "$ACADVER"),
            (1, "AC1021"),
            (0, "ENDSEC"),
            (0, "SECTION"),
            (2, "ENTITIES"),
            (3, "not a codepage"),
            (8, "水_ë"),
            (0, "ENDSEC"),
            (0, "EOF"),
        ] {
            bytes.extend(code.to_le_bytes());
            bytes.extend(value.as_bytes());
            bytes.push(0);
        }
        let groups = Binary::new(&bytes)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(groups[10].value, "水_ë");
    }
    #[test]
    fn binary_scalars_chunks_and_r12_extended_codes_are_exact() {
        for wide in [false, true] {
            let mut bytes = SIGNATURE.to_vec();
            let codes: [(u16, Vec<u8>); 8] = [
                (0, b"SECTION\0".to_vec()),
                (70, (-17i16).to_le_bytes().to_vec()),
                (90, (-2_000_000_000i32).to_le_bytes().to_vec()),
                (160, (-9_000_000_000i64).to_le_bytes().to_vec()),
                (10, 12.25f64.to_le_bytes().to_vec()),
                (290, vec![1]),
                (1004, vec![3, 0, 128, 255]),
                (1071, i32::MAX.to_le_bytes().to_vec()),
            ];
            for (code, value) in codes {
                if wide {
                    bytes.extend(code.to_le_bytes());
                } else if code >= 255 {
                    bytes.push(255);
                    bytes.extend(code.to_le_bytes());
                } else {
                    bytes.push(code as u8);
                }
                bytes.extend(value);
            }
            let values = Binary::new(&bytes)
                .unwrap()
                .map(|v| v.unwrap().value.into_owned())
                .collect::<Vec<_>>();
            assert_eq!(
                values,
                [
                    "SECTION",
                    "-17",
                    "-2000000000",
                    "-9000000000",
                    "12.25",
                    "1",
                    "0080FF",
                    "2147483647"
                ]
            );
        }
    }
    #[test]
    fn truncated_binary_values_return_errors_instead_of_panicking() {
        let mut bytes = SIGNATURE.to_vec();
        bytes.extend(b"\0\0SECTION\0");
        let start = bytes.len();
        bytes.extend(10u16.to_le_bytes());
        bytes.extend(1.5f64.to_le_bytes());
        for end in start + 1..bytes.len() {
            let results = Binary::new(&bytes[..end]).unwrap().collect::<Vec<_>>();
            assert!(results.last().unwrap().is_err());
        }
    }
}
