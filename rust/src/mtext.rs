//! Native MTEXT inline/paragraph parser, preceding the full layout engine.
//! Semantics follow ezdxf 1.4.4 tools/text.py (MIT, Copyright 2021-2024
//! Manfred Moitzi; see third_party/ezdxf-LICENSE.txt). No one-line stripping.
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

const MAX_INPUT: usize = 1_048_576;
const MAX_TOKENS: usize = 200_000;
const MAX_CONTEXTS: usize = 100_000;
const MAX_DEPTH: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Cancelled,
    Limit(&'static str),
    Invalid(&'static str),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(f, "MTEXT afgebroken"),
            Self::Limit(s) => write!(f, "MTEXT overschrijdt {s}"),
            Self::Invalid(s) => write!(f, "Ongeldige MTEXT: {s}"),
        }
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontFace {
    pub filename: Arc<str>,
    pub family: Arc<str>,
    pub style: Arc<str>,
    pub weight: u16,
    pub width: u8,
}
impl Default for FontFace {
    fn default() -> Self {
        Self {
            filename: Arc::from(""),
            family: Arc::from("sans-serif"),
            style: Arc::from("Regular"),
            weight: 400,
            width: 5,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TabStop {
    Number(f64),
    Encoded(Arc<str>),
}
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paragraph {
    pub indent: f64,
    pub left: f64,
    pub right: f64,
    pub align: u8,
    pub tab_stops: Arc<[TabStop]>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub stroke: u8,
    pub continue_stroke: bool,
    pub aci: u16,
    pub rgb: Option<[u8; 3]>,
    pub align: u8,
    pub font_face: Arc<FontFace>,
    pub cap_height: f64,
    pub width_factor: f64,
    pub char_tracking_factor: f64,
    pub oblique: f64,
    pub paragraph: Arc<Paragraph>,
}
impl Default for Context {
    fn default() -> Self {
        Self {
            stroke: 0,
            continue_stroke: false,
            aci: 7,
            rgb: None,
            align: 0,
            font_face: Arc::default(),
            cap_height: 1.,
            width_factor: 1.,
            char_tracking_factor: 1.,
            oblique: 0.,
            paragraph: Arc::default(),
        }
    }
}
impl Context {
    pub fn validate(&self) -> Result<()> {
        self.validate_scalars()?;
        validate_tabs(&self.paragraph.tab_stops)
    }
    fn validate_scalars(&self) -> Result<()> {
        let p = &self.paragraph;
        if self.stroke > 7 || self.aci > 256 || self.align > 2 || p.align > 5 {
            return Err(Error::Invalid("opmaakvlaggen"));
        }
        if [
            self.cap_height,
            self.width_factor,
            self.char_tracking_factor,
            self.oblique,
            p.indent,
            p.left,
            p.right,
        ]
        .iter()
        .any(|x| !x.is_finite())
        {
            return Err(Error::Invalid("niet-eindige opmaakwaarde"));
        }
        if self.cap_height < 0. || self.width_factor < 0. || self.char_tracking_factor < 0. {
            return Err(Error::Invalid("negatieve tekstfactor"));
        }
        if [
            self.font_face.filename.len(),
            self.font_face.family.len(),
            self.font_face.style.len(),
        ]
        .into_iter()
        .any(|n| n > 32768)
            || p.tab_stops.len() > 100_000
        {
            return Err(Error::Limit("font-/tabstopbudget"));
        }
        Ok(())
    }
}
fn validate_tabs(tabs: &[TabStop]) -> Result<()> {
    for t in tabs {
        match t {
            TabStop::Number(v) if !v.is_finite() => return Err(Error::Invalid("tabstop")),
            TabStop::Encoded(s) if s.len() > 32768 => return Err(Error::Limit("tabstoptekst")),
            _ => {}
        }
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum Content {
    Word(String),
    Stack([String; 3]),
    Space,
    Nbsp,
    Tabulator,
    NewParagraph,
    NewColumn,
    WrapAtDimline,
    PropertiesChanged(String),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Token {
    pub content: Content,
    pub context: Arc<Context>,
}

// Decimal digit zeroes from Python 3.11 / Unicode 14 (the original runtime).
// Regex \d accepts Nd, including non-ASCII digits; int/float accept them too.
const ZEROES: &[u32] = &[
    0x30, 0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66, 0xbe6, 0xc66, 0xce6, 0xd66,
    0xde6, 0xe50, 0xed0, 0xf20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946, 0x19d0, 0x1a80, 0x1a90,
    0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0, 0xa9f0, 0xaa50, 0xabf0, 0xff10,
    0x104a0, 0x10d30, 0x11066, 0x110f0, 0x11136, 0x111d0, 0x112f0, 0x11450, 0x114d0, 0x11650,
    0x116c0, 0x11730, 0x118e0, 0x11950, 0x11c50, 0x11d50, 0x11da0, 0x16a60, 0x16ac0, 0x16b50,
    0x1d7ce, 0x1d7d8, 0x1d7e2, 0x1d7ec, 0x1d7f6, 0x1e140, 0x1e2f0, 0x1e950, 0x1fbf0,
];
fn digit(c: char) -> Option<u8> {
    let n = c as u32;
    let i = ZEROES.partition_point(|&z| z <= n);
    i.checked_sub(1)
        .map(|i| n - ZEROES[i])
        .filter(|&v| v < 10)
        .map(|v| v as u8)
}
fn finite(v: f64) -> Result<f64> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(Error::Invalid("numerieke opmaak valt buiten bereik"))
    }
}

struct Scanner {
    text: Vec<char>,
    at: usize,
}
impl Scanner {
    fn new(text: Vec<char>) -> Self {
        Self { text, at: 0 }
    }
    fn peek(&self, offset: usize) -> Option<char> {
        self.text.get(self.at + offset).copied()
    }
    fn get(&mut self) -> Option<char> {
        let c = self.peek(0);
        if c.is_some() {
            self.at += 1;
        }
        c
    }
    fn take(&mut self) -> char {
        self.get().unwrap_or('\0')
    }
    fn skip_comma(&mut self) {
        while self.peek(0) == Some(',') {
            self.at += 1;
        }
    }
    fn terminator(&mut self) {
        if self.peek(0) == Some(';') {
            self.at += 1;
        }
    }
    fn float_expr(&mut self, relative: bool) -> String {
        let start = self.at;
        let mut i = start;
        if matches!(self.text.get(i), Some('+' | '-')) {
            i += 1;
        }
        let digits = i;
        while self.text.get(i).is_some_and(|&c| digit(c).is_some()) {
            i += 1;
        }
        let mut count = i - digits;
        if self.text.get(i) == Some(&'.') {
            i += 1;
            let first = i;
            while self.text.get(i).is_some_and(|&c| digit(c).is_some()) {
                i += 1;
            }
            count += i - first;
        }
        if count == 0 {
            return String::new();
        }
        if matches!(self.text.get(i), Some('e' | 'E')) {
            let exponent = i;
            i += 1;
            if matches!(self.text.get(i), Some('+' | '-')) {
                i += 1;
            }
            let first = i;
            while self.text.get(i).is_some_and(|&c| digit(c).is_some()) {
                i += 1;
            }
            if i == first {
                i = exponent;
            }
        }
        if relative && self.text.get(i) == Some(&'x') {
            i += 1;
        }
        self.at = i;
        self.text[start..i].iter().collect()
    }
    fn value(&mut self, old: f64, relative: bool) -> Result<f64> {
        let expr = self.float_expr(relative);
        if expr.is_empty() {
            return Ok(old);
        }
        let factor = expr.ends_with('x');
        let value = number(expr.trim_end_matches('x'))?;
        if relative {
            finite(if factor {
                old * value.abs()
            } else {
                value.abs()
            })
        } else {
            Ok(value)
        }
    }
    fn expression(&mut self, escape: bool) -> Vec<char> {
        let start = self.at;
        while let Some(c) = self.peek(0) {
            if escape && c == '\\' && self.peek(1) == Some(';') {
                self.at += 2;
                continue;
            }
            if c == ';' {
                break;
            }
            self.at += 1;
        }
        let result = self.text[start..self.at].to_vec();
        self.terminator();
        result
    }
    fn integer(&mut self) -> Result<Option<u32>> {
        let start = self.at;
        let mut value = 0;
        while let Some(d) = self.peek(0).and_then(digit) {
            value = (value * 10 + u32::from(d)) & 0xffffff;
            self.at += 1;
        }
        if self.at - start > 4300 {
            return Err(Error::Limit("4300 cijfers in kleurwaarde"));
        }
        Ok((self.at > start).then_some(value))
    }
}
fn number(expr: &str) -> Result<f64> {
    let s: String = expr
        .chars()
        .map(|c| digit(c).map_or(c, |d| char::from(b'0' + d)))
        .collect();
    finite(s.parse().map_err(|_| Error::Invalid("getal"))?)
}
struct Parser<'a> {
    scan: Scanner,
    ctx: Arc<Context>,
    stack: Vec<Arc<Context>>,
    continue_stroke: bool,
    tokens: Vec<Token>,
    contexts: usize,
    properties: bool,
    cancel: &'a AtomicBool,
    steps: usize,
}
impl Parser<'_> {
    fn check(&mut self) -> Result<()> {
        self.steps += 1;
        if self.steps.is_multiple_of(1024) && self.cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        Ok(())
    }
    fn emit(&mut self, content: Content) -> Result<()> {
        if self.tokens.len() >= MAX_TOKENS {
            return Err(Error::Limit("tokenbudget"));
        }
        self.tokens.push(Token {
            content,
            context: self.ctx.clone(),
        });
        Ok(())
    }
    fn word(&mut self, word: &mut String) -> Result<()> {
        if !word.is_empty() {
            self.emit(Content::Word(std::mem::take(word)))?;
        }
        Ok(())
    }
    fn run(mut self) -> Result<Vec<Token>> {
        let mut word = String::new();
        while let Some(mut c) = self.scan.peek(0) {
            self.check()?;
            let start = self.scan.at;
            let mut escaped = false;
            if c == '\\' {
                // Python's empty-string membership also treats a lone final
                // backslash as an escape; its missing character becomes space.
                if self
                    .scan
                    .peek(1)
                    .is_none_or(|c| matches!(c, '\\' | '{' | '}'))
                {
                    escaped = true;
                    self.scan.at += 1;
                    c = self.scan.peek(0).unwrap_or('\0');
                } else {
                    self.word(&mut word)?;
                    self.scan.at += 1;
                    let cmd = self.scan.take();
                    let control = match cmd {
                        '~' => Some(Content::Nbsp),
                        'P' => Some(Content::NewParagraph),
                        'N' => Some(Content::NewColumn),
                        'X' => Some(Content::WrapAtDimline),
                        'S' => Some(Content::Stack(self.stacking()?)),
                        _ => None,
                    };
                    if let Some(token) = control {
                        self.emit(token)?;
                    } else if self.property(cmd)? {
                        if self.properties {
                            self.emit(Content::PropertiesChanged(
                                self.scan.text[start..self.scan.at].iter().collect(),
                            ))?;
                        }
                    } else {
                        word.push('\\');
                        word.push(cmd);
                    }
                    continue;
                }
            }
            if c < ' ' {
                let control = match c {
                    '\t' => Content::Tabulator,
                    '\n' => Content::NewParagraph,
                    _ => Content::Space,
                };
                self.word(&mut word)?;
                self.scan.at = self.scan.at.saturating_add(1);
                self.emit(control)?;
                continue;
            }
            if c == '%' && self.scan.peek(1) == Some('%') {
                let special = match self.scan.peek(2).map(|c| c.to_ascii_lowercase()) {
                    Some('c') => Some('Ø'),
                    Some('d') => Some('°'),
                    Some('p') => Some('±'),
                    _ => None,
                };
                if let Some(s) = special {
                    self.scan.at += 2;
                    c = s;
                }
            }
            if c == ' ' {
                self.word(&mut word)?;
                self.scan.at += 1;
                self.emit(Content::Space)?;
                continue;
            }
            if !escaped && matches!(c, '{' | '}') {
                self.word(&mut word)?;
                self.scan.at += 1;
                if c == '{' {
                    if self.stack.len() >= MAX_DEPTH {
                        return Err(Error::Limit("128 opmaakniveaus"));
                    }
                    self.stack.push(self.ctx.clone());
                } else if let Some(ctx) = self.stack.pop() {
                    self.ctx = ctx;
                }
                continue;
            }
            self.scan.at += 1;
            word.push(c);
        }
        self.word(&mut word)?;
        if self.cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        Ok(self.tokens)
    }
    fn stacking(&mut self) -> Result<[String; 3]> {
        let mut scan = Scanner::new(self.scan.expression(true));
        let mut upper = String::new();
        let mut lower = String::new();
        let mut mode = String::new();
        while let Some(mut c) = scan.get() {
            self.check()?;
            let mut escaped = false;
            if c == '\\' {
                escaped = true;
                c = scan
                    .get()
                    .ok_or(Error::Invalid("los escape in stapeling"))?;
            }
            if c < ' ' {
                c = ' ';
            }
            if !escaped && mode.is_empty() && matches!(c, '^' | '/' | '#') {
                mode.push(c);
                continue;
            }
            if mode.is_empty() {
                upper.push(c);
            } else {
                lower.push(c);
            }
        }
        Ok([upper, lower, mode])
    }
    fn paragraph(&mut self, old: &Paragraph) -> Result<Paragraph> {
        let mut scan = Scanner::new(self.scan.expression(false));
        let mut p = old.clone();
        while let Some(cmd) = scan.get() {
            self.check()?;
            match cmd {
                'i' | 'l' | 'r' => {
                    let expr = scan.float_expr(false);
                    let value = if expr.is_empty() {
                        0.
                    } else {
                        scan.skip_comma();
                        number(&expr)?
                    };
                    match cmd {
                        'i' => p.indent = value,
                        'l' => p.left = value,
                        _ => p.right = value,
                    }
                }
                'q' => {
                    p.align = match scan.take() {
                        'l' => 1,
                        'r' => 2,
                        'c' => 3,
                        'j' => 4,
                        'd' => 5,
                        _ => 0,
                    };
                    scan.skip_comma();
                }
                't' => {
                    let mut tabs = Vec::new();
                    while let Some(c) = scan.peek(0) {
                        self.check()?;
                        if matches!(c, 'r' | 'c') {
                            scan.at += 1;
                            let expr = scan.float_expr(false);
                            if !expr.is_empty() {
                                scan.skip_comma();
                            }
                            tabs.push(TabStop::Encoded(Arc::from(format!("{c}{expr}"))));
                        } else {
                            let expr = scan.float_expr(false);
                            if expr.is_empty() {
                                scan.at += 1;
                            } else {
                                tabs.push(TabStop::Number(number(&expr)?));
                                scan.skip_comma();
                            }
                        }
                        if tabs.len() > 100_000 {
                            return Err(Error::Limit("tabstopbudget"));
                        }
                    }
                    p.tab_stops = Arc::from(tabs);
                }
                _ => {}
            }
        }
        Ok(p)
    }
    fn property(&mut self, cmd: char) -> Result<bool> {
        if !matches!(
            cmd,
            'L' | 'l'
                | 'O'
                | 'o'
                | 'K'
                | 'k'
                | 'A'
                | 'C'
                | 'c'
                | 'H'
                | 'W'
                | 'Q'
                | 'T'
                | 'p'
                | 'f'
                | 'F'
        ) {
            return Ok(false);
        }
        self.contexts += 1;
        if self.contexts > MAX_CONTEXTS {
            return Err(Error::Limit("opmaakwijzigingen"));
        }
        let mut ctx = (*self.ctx).clone();
        match cmd {
            'L' | 'O' | 'K' => {
                ctx.stroke |= match cmd {
                    'L' => 1,
                    'K' => 2,
                    _ => 4,
                };
                self.continue_stroke = true;
            }
            'l' | 'o' | 'k' => {
                ctx.stroke &= !match cmd {
                    'l' => 1,
                    'k' => 2,
                    _ => 4,
                };
                if ctx.stroke == 0 {
                    self.continue_stroke = false;
                }
            }
            'A' => {
                ctx.align = match self
                    .scan
                    .get()
                    .ok_or(Error::Invalid("uitlijning mist waarde"))?
                {
                    '1' => 1,
                    '2' => 2,
                    _ => 0,
                };
                self.scan.terminator();
            }
            'C' => {
                let start = self.scan.at;
                let v = self.scan.integer()?;
                if let Some(v) = v {
                    // ACI is range checked, not modulo 2^24 like RGB.
                    let mut aci = 0u32;
                    for &c in &self.scan.text[start..self.scan.at] {
                        aci = (aci * 10 + u32::from(digit(c).unwrap())).min(257);
                    }
                    if aci < 257 {
                        ctx.aci = v as u16;
                        ctx.rgb = None;
                    }
                }
                self.scan.terminator();
            }
            'c' => {
                if let Some(v) = self.scan.integer()? {
                    ctx.rgb = Some([
                        (v & 255) as u8,
                        ((v >> 8) & 255) as u8,
                        ((v >> 16) & 255) as u8,
                    ]);
                }
                self.scan.terminator();
            }
            'H' => {
                ctx.cap_height = self.scan.value(ctx.cap_height, true)?;
                self.scan.terminator();
            }
            'W' => {
                ctx.width_factor = self.scan.value(ctx.width_factor, true)?;
                self.scan.terminator();
            }
            'T' => {
                ctx.char_tracking_factor = self.scan.value(ctx.char_tracking_factor, true)?;
                self.scan.terminator();
            }
            'Q' => {
                ctx.oblique = self.scan.value(ctx.oblique, false)?;
                self.scan.terminator();
            }
            'p' => {
                let paragraph = self.paragraph(&ctx.paragraph)?;
                // Tab arrays are immutable/shared. Revalidate only a newly
                // parsed array, not on every scalar/indent/font transition.
                if !Arc::ptr_eq(&paragraph.tab_stops, &ctx.paragraph.tab_stops) {
                    validate_tabs(&paragraph.tab_stops)?;
                }
                ctx.paragraph = Arc::new(paragraph);
            }
            'f' | 'F' => {
                let expression: String = self.scan.expression(false).iter().collect();
                let mut parts = expression.split('|');
                let name = parts.next().unwrap_or("");
                if !name.is_empty() {
                    let mut face = FontFace {
                        family: Arc::from(name),
                        ..Default::default()
                    };
                    for part in parts {
                        if part.starts_with("b1") {
                            face.weight = 700;
                        } else if part.starts_with("i1") {
                            face.style = Arc::from("Italic");
                        }
                    }
                    ctx.font_face = Arc::new(face);
                }
            }
            _ => unreachable!(),
        }
        ctx.continue_stroke = self.continue_stroke;
        ctx.validate_scalars()?;
        self.ctx = Arc::new(ctx);
        Ok(true)
    }
}

pub fn parse(
    content: &str,
    initial: Context,
    yield_properties: bool,
    cancel: &AtomicBool,
) -> Result<Vec<Token>> {
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    if content.len() > MAX_INPUT {
        return Err(Error::Limit("1 MiB tekstbron"));
    }
    initial.validate()?;
    let mut chars = content.chars().peekable();
    let mut decoded = Vec::with_capacity(content.len().min(MAX_INPUT));
    while let Some(c) = chars.next() {
        let c = if c == '^' && chars.peek().is_some_and(|&c| c != '\n') {
            char::from_u32((chars.next().unwrap() as i64 - 64).rem_euclid(126) as u32).unwrap()
        } else {
            c
        };
        decoded.push(c);
        if decoded.len().is_multiple_of(1024) && cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
    }
    Parser {
        scan: Scanner::new(decoded),
        ctx: Arc::new(initial),
        stack: Vec::new(),
        continue_stroke: false,
        tokens: Vec::new(),
        contexts: 0,
        properties: yield_properties,
        cancel,
        steps: 0,
    }
    .run()
}
