//! Simple MTEXT uses the original separate wrapping/alignment route.
//! ezdxf addons/drawing/text.py, Copyright 2020-2023 Matthew Broadway, MIT.
use super::*;
use crate::dxf_text_metrics::{AdvanceReader, Measurements};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimpleRequest {
    pub content: String,
    pub face: Arc<FontFace>,
    pub height: f64,
    pub width: f64,
    pub spacing: f64,
    pub align: u8,
    pub insert: V3,
    pub rotation: f64,
    pub normal: V3,
    #[serde(default)]
    pub direction: Option<V3>,
}
#[derive(Debug, Serialize)]
pub struct SimpleLine {
    pub text: String,
    pub width: f64,
    pub matrix: [[f64; 4]; 3],
    pub contours: Path,
}
#[derive(Debug, Serialize)]
pub struct SimpleOutput {
    pub plain: String,
    pub measurements: Measurements,
    pub selected: Option<String>,
    pub font_kind: &'static str,
    pub lines: Vec<SimpleLine>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstimateRequest {
    pub content: String,
    pub font: String,
    pub height: f64,
    pub width: f64,
    pub spacing: f64,
}
#[derive(Debug, Serialize)]
pub struct Estimate {
    pub width: f64,
    pub height: f64,
}
fn whitespace(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '\u{1c}'..='\u{1f}')
}
fn trailing(text: &str) -> &str {
    text.trim_end_matches(whitespace)
}
/// Original decisions with streaming raw advances, preserving the addition
/// order even for fractional stroke-font advances. No world-width prefix sums.
fn wrap(
    text: &str,
    width: Option<f64>,
    reader: &mut AdvanceReader,
    cancel: &AtomicBool,
) -> Result<Vec<(String, f64)>, String> {
    if text.is_empty() || text.chars().all(whitespace) {
        return Ok(Vec::new());
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut raw = 0.;
    let mut ink = 0.;
    let mut just_wrapped = false;
    let mut work = 0usize;
    let mut tokens = text.char_indices().peekable();
    while let Some((start, ch)) = tokens.next() {
        crate::input::check(cancel)?;
        let white = whitespace(ch);
        // re.split at newlines first: each newline is an independent token.
        while ch != '\n'
            && tokens
                .peek()
                .is_some_and(|(_, next)| *next != '\n' && whitespace(*next) == white)
        {
            tokens.next();
        }
        let end = tokens.peek().map_or(text.len(), |(i, _)| *i);
        let token = &text[start..end];
        if token == "\n" && just_wrapped {
            continue;
        }
        just_wrapped = false;
        if token == "\n" {
            lines.push((trailing(&current).to_owned(), reader.width(ink)?));
            current.clear();
            raw = 0.;
            ink = 0.;
        } else if white {
            if !current.is_empty() || lines.is_empty() {
                raw = reader.append(token, raw, &mut work, cancel)?;
                current.push_str(token);
            }
        } else {
            let candidate = reader.append(token, raw, &mut work, cancel)?;
            let candidate_width = reader.width(candidate)?;
            if width.is_some_and(|w| candidate_width > w) && !current.is_empty() {
                lines.push((trailing(&current).to_owned(), reader.width(ink)?));
                current.clear();
                raw = reader.append(token, 0., &mut work, cancel)?;
                just_wrapped = true;
            } else {
                raw = candidate;
            }
            current.push_str(token);
            ink = raw;
        }
        if lines.len() > 200_000 {
            return Err("MTEXT overschrijdt regelbudget".into());
        }
    }
    if !current.is_empty() && !current.chars().all(whitespace) {
        lines.push((trailing(&current).to_owned(), reader.width(ink)?));
    }
    Ok(lines)
}
impl Renderer<'_> {
    /// Original fast extent estimate for a single column. The source adapter
    /// resolves STYLE filename/height; this is intentionally not precise layout.
    pub fn estimate(&mut self, request: EstimateRequest) -> Result<Estimate, String> {
        crate::input::check(self.cancel)?;
        if [request.height, request.width, request.spacing]
            .iter()
            .any(|v| !v.is_finite())
            || request.font.len() > 32768
        {
            return Err("Ongeldige MTEXT-breedteschatting".into());
        }
        let plain = crate::mtext_plain::decode(&request.content, self.cancel)?;
        let mut font = self.catalog.resolve(&request.font, self.cancel)?;
        let mono = font.kind() == "MonospaceFont";
        let (_, fm) = font.measure("", request.height, 1., self.cancel)?;
        let space = self
            .glyphs
            .space_width(&font, request.height, 1., self.cancel)?;
        let mut reader = font.advance_reader(request.height)?;
        let mut max_width = 0f64;
        let mut count = 0f64;
        let mut work = 0usize;
        if plain.split('\n').all(str::is_empty) {
            return Ok(Estimate {
                width: 0.,
                height: 0.,
            });
        }
        for line in plain.split('\n') {
            crate::input::check(self.cancel)?;
            let raw = reader.append(line, 0., &mut work, self.cancel)?;
            let mut width = if !mono && line.chars().all(whitespace) {
                0.
            } else {
                reader.width(raw)?
            };
            if width == 0. && !line.is_empty() {
                width = line.chars().count() as f64 * space;
            }
            if request.width > 0. {
                count += (width / request.width).ceil();
                width = width.min(request.width);
            } else {
                count += 1.;
            }
            if !count.is_finite() || count.abs() > 200_000. {
                return Err("MTEXT overschrijdt schattingsregelbudget".into());
            }
            max_width = max_width.max(width);
        }
        let spacing = fm.cap_height * 1.667 * request.spacing - fm.cap_height;
        let height = fm.cap_height * count + spacing * (count - 1.);
        let width = max_width * 1.01;
        if !width.is_finite() || !height.is_finite() {
            return Err("MTEXT-schatting valt buiten getalbereik".into());
        }
        Ok(Estimate { width, height })
    }
    pub fn render_simple(&mut self, request: SimpleRequest) -> Result<SimpleOutput, String> {
        crate::input::check(self.cancel)?;
        if [
            request.height,
            request.width,
            request.spacing,
            request.rotation,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || !(1..=9).contains(&request.align)
        {
            return Err("Ongeldige eenvoudige MTEXT".into());
        }
        crate::cad_geometry::finite(request.insert)?;
        crate::cad_geometry::finite(request.normal)?;
        if let Some(d) = request.direction {
            crate::cad_geometry::finite(d)?;
        }
        let height = if request.height > 1e-9 {
            request.height
        } else {
            2.5
        };
        let plain = crate::mtext_plain::decode(&request.content, self.cancel)?;
        let i = self.font(&request.face)?;
        let font = &mut self.fonts[i].font;
        let (_, measurements) = font.measure("", height, 1., self.cancel)?;
        let lines = wrap(
            &plain,
            if request.width == 0. {
                None
            } else {
                Some(request.width)
            },
            &mut font.advance_reader(height)?,
            self.cancel,
        )?;
        let max_width = lines.iter().map(|(_, w)| *w).reduce(f64::max).unwrap_or(0.);
        let box_width = if request.width == 0. {
            max_width
        } else {
            request.width
        };
        let spacing = height * (5. / 3.) * request.spacing;
        let baseline =
            |n: usize| -measurements.baseline - (measurements.cap_height + n as f64 * spacing);
        let last = baseline(lines.len().saturating_sub(1));
        let ax = match (request.align - 1) % 3 {
            1 => box_width / 2.,
            2 => box_width,
            _ => 0.,
        };
        let ay = match (request.align - 1) / 3 {
            1 => (baseline(0) + measurements.x_height + last) / 2.,
            2 => last - measurements.descender_height,
            _ => 0.,
        };
        let angle = if let Some(d) = request.direction {
            d[1].atan2(d[0])
        } else {
            request.rotation.to_radians()
        };
        let (s, c) = angle.sin_cos();
        let sign = if request.normal[2] < 0. { -1. } else { 1. };
        let whole = Affine([
            [c, -s * sign, 0., request.insert[0]],
            [s, c * sign, 0., request.insert[1]],
            [0., 0., 1., request.insert[2]],
        ]);
        let mut rendered = Vec::new();
        let mut controls = 0usize;
        for (n, (text, width)) in lines.into_iter().enumerate() {
            crate::input::check(self.cancel)?;
            let x = match (request.align - 1) % 3 {
                1 => ax - width / 2.,
                2 => ax - width,
                _ => 0.,
            };
            let matrix = whole.compose(Affine([
                [1., 0., 0., x - ax],
                [0., 1., 0., baseline(n) - ay],
                [0., 0., 1., 0.],
            ]))?;
            // The simple frontend passes dxftype MTEXT to draw_text: controls
            // become the rectangle replacement; tab becomes eight spaces.
            let mut content = String::new();
            for ch in text.chars() {
                if ch == '\t' {
                    content.push_str("        ");
                } else {
                    content.push(if (ch as u32) < 32 { '▯' } else { ch });
                }
            }
            let mut contours = if text.chars().all(whitespace) {
                Path::default()
            } else {
                self.glyphs.text(
                    &self.fonts[i].font,
                    &content,
                    height,
                    MAX_CONTROLS
                        .saturating_sub(self.controls)
                        .saturating_sub(controls),
                    self.cancel,
                )?
            };
            contours.transform(matrix)?;
            controls += contours.points.len();
            rendered.push(SimpleLine {
                text,
                width,
                matrix: matrix.0,
                contours,
            });
        }
        crate::input::check(self.cancel)?;
        self.controls += controls;
        Ok(SimpleOutput {
            plain,
            measurements,
            selected: self.fonts[i].selected.clone(),
            font_kind: self.fonts[i].font.kind(),
            lines: rendered,
        })
    }
}
