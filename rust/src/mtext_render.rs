//! Complex MTEXT: parse -> real font metrics -> cell layout -> vector contours.
//! The simple-MTEXT route, DXF/source wiring, background fill and font fill are
//! separate integration steps. No one-line escape stripping occurs here.
//! Layout adapter: ezdxf 1.4.4 render/abstract_mtext_renderer.py (2021-2025)
//! and addons/drawing/mtext_complex.py (2021-2022), Manfred Moitzi, MIT.
use crate::{
    cad_geometry::{Affine, V3},
    cad_glyphs::Cache,
    cad_text_path::{Path, MAX_CONTROLS},
    dxf_fonts::Catalog,
    dxf_text_metrics::Font,
    mtext::{self, Content, Context, FontFace},
    mtext_layout::{self as ml, CellSpec, ColumnSpec, Event, ParagraphSpec, TabStop},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc},
};
fn one() -> f64 {
    1.
}
fn top_left() -> u8 {
    1
}
fn identity() -> [[f64; 4]; 3] {
    Affine::IDENTITY.0
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub content: String,
    pub initial: Context,
    /// Defined/estimated MTEXT width, resolved by the source adapter.
    pub width: f64,
    #[serde(default)]
    pub columns: Vec<ColumnSpec>,
    #[serde(default = "one")]
    pub spacing: f64,
    #[serde(default = "top_left")]
    pub align: u8,
    #[serde(default)]
    pub at: [f64; 2],
    #[serde(default = "identity")]
    pub matrix: [[f64; 4]; 3],
}
#[derive(Debug, Serialize)]
pub struct Word {
    pub text: String,
    pub context: Arc<Context>,
    pub selected: Option<String>,
    pub font_kind: &'static str,
    pub width: f64,
    pub space_width: f64,
}
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Draw {
    Text { id: u32, contours: Path },
    Line { id: u32, start: V3, end: V3 },
}
#[derive(Debug, Serialize)]
pub struct Output {
    pub layout: ml::Output,
    /// Text ids are one-based; id zero identifies the entity's divider paint.
    pub words: Vec<Word>,
    pub draws: Vec<Draw>,
}
struct Resolved {
    font: Font,
    selected: Option<String>,
}
pub struct Renderer<'a> {
    catalog: &'a mut Catalog,
    // Fonts stay alive until the physical-source glyph cache is discarded.
    fonts: Vec<Resolved>,
    names: HashMap<Arc<FontFace>, usize>,
    faces: HashMap<usize, (Arc<FontFace>, usize)>,
    glyphs: Cache,
    controls: usize,
    cancel: &'a AtomicBool,
}
impl<'a> Renderer<'a> {
    pub fn new(catalog: &'a mut Catalog, cancel: &'a AtomicBool) -> Self {
        Self {
            catalog,
            fonts: Vec::new(),
            names: HashMap::new(),
            faces: HashMap::new(),
            glyphs: Cache::default(),
            controls: 0,
            cancel,
        }
    }
    fn font(&mut self, face: &Arc<FontFace>) -> Result<usize, String> {
        crate::input::check(self.cancel)?;
        let key = Arc::as_ptr(face) as usize;
        if let Some((_, i)) = self.faces.get(&key) {
            return Ok(*i);
        }
        if self.faces.len() >= 100_000 {
            return Err("Te veel MTEXT-fontcontexten".into());
        }
        let i = if let Some(i) = self.names.get(face) {
            *i
        } else {
            if self.fonts.len() >= 4096 {
                return Err("Te veel MTEXT-fontvarianten".into());
            }
            // The parser represents b1 as Regular+700. The old style-first
            // matcher can then select the sole Regular face before considering
            // weight. Give this contradictory MTEXT descriptor a Bold hint.
            // Measurement and drawing deliberately use the same resolved face;
            // the old UnifiedTextRenderer performed a second, different match.
            let selected_face = if face.weight >= 700
                && face.style.eq_ignore_ascii_case("Regular")
                && face.filename.is_empty()
            {
                FontFace {
                    style: Arc::from("Bold"),
                    ..(**face).clone()
                }
            } else {
                (**face).clone()
            };
            let font = self.catalog.resolve_face(&selected_face, self.cancel)?;
            let selected = self
                .catalog
                .selected_face_path(&selected_face)
                .and_then(|p| p.file_name())
                .map(|p| p.to_string_lossy().into_owned());
            let i = self.fonts.len();
            self.fonts.push(Resolved { font, selected });
            self.names.insert(face.clone(), i);
            i
        };
        // Retain every identity key, including aliases, until this cache drops.
        self.faces.insert(key, (face.clone(), i));
        Ok(i)
    }
    fn space(&mut self, ctx: &Context) -> Result<f64, String> {
        let i = self.font(&ctx.font_face)?;
        self.glyphs.space_width(
            &self.fonts[i].font,
            ctx.cap_height,
            ctx.width_factor,
            self.cancel,
        )
    }
    fn word(
        &mut self,
        text: String,
        ctx: &Arc<Context>,
        words: &mut Vec<Word>,
    ) -> Result<CellSpec, String> {
        let i = self.font(&ctx.font_face)?;
        let resolved = &mut self.fonts[i];
        let font = &mut resolved.font;
        let width = font
            .measure(&text, ctx.cap_height, ctx.width_factor, self.cancel)?
            .0;
        let space_width =
            self.glyphs
                .space_width(font, ctx.cap_height, ctx.width_factor, self.cancel)?;
        if !width.is_finite() || words.len() >= 200_000 {
            return Err("MTEXT-woordbudget of tekstbreedte overschreden".into());
        }
        let id = words.len() as u32 + 1;
        let font_kind = font.kind();
        words.push(Word {
            text,
            context: ctx.clone(),
            font_kind,
            width,
            space_width,
            selected: resolved.selected.clone(),
        });
        Ok(CellSpec::Text {
            id,
            width,
            height: ctx.cap_height,
            valign: ctx.align,
            stroke: ctx.stroke | if ctx.continue_stroke { 8 } else { 0 },
        })
    }
    pub fn render(&mut self, mut request: Request) -> Result<Output, String> {
        crate::input::check(self.cancel)?;
        if !request.width.is_finite()
            || request.width <= 0.
            || !request.spacing.is_finite()
            || request.matrix.iter().flatten().any(|v| !v.is_finite())
            || !(1..=9).contains(&request.align)
        {
            return Err("Ongeldige MTEXT-renderopdracht".into());
        }
        if request.initial.cap_height <= 1e-9 {
            request.initial.cap_height = 2.5;
        }
        request.initial.validate().map_err(|e| e.to_string())?;
        let base_height = request.initial.cap_height;
        let default_tabs = default_tabs(base_height, request.width, self.cancel)?;
        let mut tabs = Tabs::default();
        let tokens = mtext::parse(
            &request.content,
            request.initial.clone(),
            false,
            self.cancel,
        )
        .map_err(|e| e.to_string())?;
        let mut ctx = Arc::new(request.initial);
        let mut cells = Vec::new();
        let mut events = Vec::new();
        let mut words = Vec::new();
        for token in tokens {
            crate::input::check(self.cancel)?;
            ctx = token.context;
            match token.content {
                Content::NewParagraph | Content::NewColumn => {
                    events.push(paragraph(
                        std::mem::take(&mut cells),
                        &ctx,
                        base_height,
                        request.width,
                        request.spacing,
                        &default_tabs,
                        &mut tabs,
                        self.cancel,
                    )?);
                    if matches!(token.content, Content::NewColumn) {
                        events.push(Event::NextColumn);
                    }
                }
                Content::Space | Content::Nbsp | Content::Tabulator => {
                    let width = self.space(&ctx)?;
                    cells.push(match token.content {
                        Content::Space => CellSpec::Space {
                            width,
                            min: None,
                            max: None,
                        },
                        Content::Nbsp => CellSpec::Nbsp {
                            width,
                            min: None,
                            max: None,
                        },
                        _ => CellSpec::Tab {
                            width,
                            min: None,
                            max: None,
                        },
                    });
                }
                Content::Word(text) => {
                    super_glue(&mut cells);
                    cells.push(self.word(text, &ctx, &mut words)?);
                }
                Content::Stack([top, bottom, kind]) => {
                    super_glue(&mut cells);
                    let top = self.word(top, &ctx, &mut words)?;
                    if kind.is_empty() {
                        cells.push(top);
                    } else {
                        let bottom = self.word(bottom, &ctx, &mut words)?;
                        cells.push(CellSpec::Fraction {
                            id: 0,
                            top: Box::new(top),
                            bottom: Box::new(bottom),
                            valign: 0,
                            stacking: match kind.as_str() {
                                "^" => 0,
                                "#" => 2,
                                _ => 1,
                            },
                        });
                    }
                }
                Content::WrapAtDimline | Content::PropertiesChanged(_) => {}
            }
        }
        if !cells.is_empty() {
            events.push(paragraph(
                cells,
                &ctx,
                base_height,
                request.width,
                request.spacing,
                &default_tabs,
                &mut tabs,
                self.cancel,
            )?);
        }
        let columns = if request.columns.is_empty() {
            vec![ColumnSpec::default()]
        } else {
            request.columns
        };
        if columns.iter().any(|c| c.frame.is_some()) {
            return Err("MTEXT-achtergrond wordt nog niet door deze tekenkern ondersteund".into());
        }
        let layout = ml::layout(
            ml::Request {
                width: request.width,
                height: None,
                margins: Vec::new(),
                columns,
                events,
                at: request.at,
                align: request.align,
                frame: None,
            },
            self.cancel,
        )
        .map_err(|e| e.to_string())?;
        let matrix = Affine(request.matrix);
        let mut controls = 0usize;
        let mut draws = Vec::new();
        for p in &layout.primitives {
            crate::input::check(self.cancel)?;
            match p {
                ml::Primitive::Text { id, rect } => {
                    let word = &words[*id as usize - 1];
                    // Drawing's complex renderer calls draw_text with its TEXT
                    // default. It replaces raw controls but does not decode %
                    // or caret escapes again. Empty ink is skipped by backend.
                    if crate::python_values::trim(&word.text).is_empty() {
                        continue;
                    }
                    let text: String = word
                        .text
                        .chars()
                        .map(|c| if (c as u32) < 32 { '?' } else { c })
                        .collect();
                    let ctx = &word.context;
                    let i = self.font(&ctx.font_face)?;
                    let mut contours = self.glyphs.text(
                        &self.fonts[i].font,
                        &text,
                        ctx.cap_height,
                        MAX_CONTROLS
                            .saturating_sub(self.controls)
                            .saturating_sub(controls),
                        self.cancel,
                    )?;
                    let sx =
                        if (ctx.width_factor - 1.).abs() <= 1e-6 * ctx.width_factor.abs().max(1.) {
                            1.
                        } else {
                            ctx.width_factor
                        };
                    let shear = if ctx.oblique.abs() > 1e-3 {
                        ctx.oblique.to_radians().tan()
                    } else {
                        0.
                    };
                    // Unlike one-line TEXT, MTEXT's shear is not scaled by sx.
                    contours.transform(matrix.compose(Affine([
                        [sx, shear, 0., rect.left],
                        [0., 1., 0., rect.bottom],
                        [0., 0., 1., 0.],
                    ]))?)?;
                    controls = controls
                        .checked_add(contours.points.len())
                        .filter(|&v| v <= MAX_CONTROLS)
                        .ok_or("MTEXT-contourbudget overschreden")?;
                    draws.push(Draw::Text { id: *id, contours });
                }
                ml::Primitive::Line { id, start, end } => {
                    controls = controls
                        .checked_add(2)
                        .filter(|&v| v <= MAX_CONTROLS.saturating_sub(self.controls))
                        .ok_or("MTEXT-contourbudget overschreden")?;
                    draws.push(Draw::Line {
                        id: *id,
                        start: matrix.checked_point([start[0], start[1], 0.])?,
                        end: matrix.checked_point([end[0], end[1], 0.])?,
                    });
                }
                ml::Primitive::Background { .. } => {
                    return Err("Onverwachte MTEXT-achtergrond".into())
                }
            }
        }
        crate::input::check(self.cancel)?;
        self.controls += controls;
        Ok(Output {
            layout,
            words,
            draws,
        })
    }
}
fn super_glue(cells: &mut Vec<CellSpec>) {
    if cells
        .last()
        .is_some_and(|c| matches!(c, CellSpec::Text { .. } | CellSpec::Fraction { .. }))
    {
        cells.push(CellSpec::Nbsp {
            width: 0.,
            min: Some(0.),
            max: Some(0.),
        });
    }
}
fn default_tabs(height: f64, width: f64, cancel: &AtomicBool) -> Result<Arc<[TabStop]>, String> {
    let step = 4. * height;
    let mut pos = step;
    let mut tabs = Vec::new();
    while pos < width {
        crate::input::check(cancel)?;
        if tabs.len() >= 100_000 {
            return Err("MTEXT overschrijdt tabstopbudget".into());
        }
        tabs.push(TabStop { pos, kind: 0 });
        let next = pos + step;
        if !next.is_finite() || next <= pos {
            return Err("Ongeldige MTEXT-tabstopafstand".into());
        }
        pos = next;
    }
    Ok(tabs.into())
}
type TabPair = (Arc<[mtext::TabStop]>, Arc<[TabStop]>);
#[derive(Default)]
struct Tabs {
    entries: usize,
    items: HashMap<usize, TabPair>,
}
#[allow(clippy::too_many_arguments)]
fn paragraph(
    cells: Vec<CellSpec>,
    ctx: &Context,
    height: f64,
    width: f64,
    spacing: f64,
    defaults: &Arc<[TabStop]>,
    cache: &mut Tabs,
    cancel: &AtomicBool,
) -> Result<Event, String> {
    if cells.is_empty() {
        return Ok(Event::Empty {
            height: ctx.cap_height,
            spacing,
        });
    }
    let p = &ctx.paragraph;
    // Indent/alignment changes share the same immutable source tab array.
    // Key that array rather than each new paragraph context.
    let key = p.tab_stops.as_ptr() as usize;
    let tabs = if p.tab_stops.is_empty() {
        defaults.clone()
    } else if let Some((_, tabs)) = cache.items.get(&key) {
        tabs.clone()
    } else {
        let mut tabs = Vec::new();
        for t in p.tab_stops.iter() {
            crate::input::check(cancel)?;
            let (v, kind) = match t {
                mtext::TabStop::Number(v) => (*v, 0),
                mtext::TabStop::Encoded(s) => (
                    crate::python_values::number(Some(&serde_json::Value::String(
                        s.get(1..).ok_or("Ongeldige MTEXT-tabstop")?.to_owned(),
                    )))
                    .ok_or("Ongeldige MTEXT-tabstop")?,
                    if s.starts_with('c') { 2 } else { 1 },
                ),
            };
            let pos = v * height;
            if !pos.is_finite() {
                return Err("MTEXT-tabstop valt buiten getalbereik".into());
            }
            if pos < width {
                tabs.push(TabStop { pos, kind });
            }
        }
        let last = tabs.last().map_or(0., |t| t.pos);
        tabs.extend(defaults.iter().filter(|t| t.pos > last).copied());
        cache.entries = cache
            .entries
            .checked_add(tabs.len())
            .filter(|&n| n <= 500_000)
            .ok_or("MTEXT overschrijdt gezamenlijk tabstopbudget")?;
        let tabs: Arc<[TabStop]> = tabs.into();
        cache.items.insert(key, (p.tab_stops.clone(), tabs.clone()));
        tabs
    };
    let left = p.left * height;
    Ok(Event::Paragraph(ParagraphSpec {
        cells,
        align: match p.align {
            2 => 2,
            3 => 3,
            4 | 5 => 4,
            _ => 1,
        },
        indent: [left + p.indent * height, left, p.right * height],
        spacing,
        margins: Vec::new(),
        tabs,
        frame: None,
    }))
}
