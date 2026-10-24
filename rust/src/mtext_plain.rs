//! Original fast MTEXT decoding, without repeated copies of the remaining text.
//! ezdxf tools/text.py, Copyright 2021-2024 Manfred Moitzi, MIT.
use std::sync::atomic::AtomicBool;
fn push(out: &mut String, ch: char) -> Result<(), String> {
    if out.len() + ch.len_utf8() > 2 * 1024 * 1024 {
        return Err("MTEXT overschrijdt gedecodeerd tekstbudget".into());
    }
    out.push(ch);
    Ok(())
}
pub fn decode(text: &str, cancel: &AtomicBool) -> Result<String, String> {
    crate::input::check(cancel)?;
    if text.len() > 1_048_576 {
        return Err("MTEXT overschrijdt invoerbudget".into());
    }
    let mut chars = Vec::with_capacity(text.len());
    let mut input = text.chars().peekable();
    let mut work = 0usize;
    while let Some(ch) = input.next() {
        if ch == '^' && input.peek().is_some_and(|c| *c != '\n') {
            let next = input.next().unwrap() as i64;
            chars.push(char::from_u32((next - 64).rem_euclid(126) as u32).unwrap());
        } else {
            chars.push(ch);
        }
        work += 1;
        if work.is_multiple_of(1024) {
            crate::input::check(cancel)?;
        }
    }
    let semicolons: Vec<_> = chars
        .iter()
        .enumerate()
        .filter_map(|(i, c)| (*c == ';').then_some(i))
        .collect();
    let mut semi = 0;
    let mut at = 0;
    let mut out = String::new();
    while at < chars.len() {
        crate::input::check(cancel)?;
        let ch = chars[at];
        at += 1;
        match ch {
            '\\' => {
                let Some(&cmd) = chars.get(at) else {
                    break;
                };
                at += 1;
                match cmd {
                    '\\' | '{' | '}' => push(&mut out, cmd)?,
                    'P' => push(&mut out, '\n')?,
                    'N' => push(&mut out, ' ')?,
                    'L' | 'l' | 'O' | 'o' | 'K' | 'k' | 'X' => {}
                    _ => {
                        while semi < semicolons.len() && semicolons[semi] < at {
                            semi += 1;
                        }
                        let end = semicolons.get(semi).copied();
                        if cmd == 'S' {
                            for &c in &chars[at..end.unwrap_or(chars.len())] {
                                push(&mut out, c)?;
                            }
                        }
                        if let Some(end) = end {
                            at = end + 1;
                        } else {
                            push(&mut out, '\\')?;
                            push(&mut out, cmd)?;
                        }
                    }
                }
            }
            '{' | '}' => {}
            '%' if chars.get(at) == Some(&'%') => {
                at += 1;
                if let Some(&code) = chars.get(at) {
                    at += 1;
                    if let Some(c) = match code {
                        'c' | 'C' => Some('Ø'),
                        'd' | 'D' => Some('°'),
                        'p' | 'P' => Some('±'),
                        _ => None,
                    } {
                        push(&mut out, c)?;
                    } else {
                        for c in ['%', '%', code] {
                            push(&mut out, c)?;
                        }
                    }
                }
            }
            _ => push(&mut out, ch)?,
        }
    }
    crate::input::check(cancel)?;
    Ok(out)
}
