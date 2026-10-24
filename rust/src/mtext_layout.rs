//! Native measured-cell MTEXT layout. Font measurement and CAD transforms are
//! applied by the caller. Semantics follow ezdxf 1.4.4 tools/text_layout.py
//! (MIT, Copyright 2021-2023 Manfred Moitzi; see third_party/ezdxf-LICENSE.txt).
pub use crate::mtext::Error;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
type Result<T> = std::result::Result<T, Error>;
mod cell;
mod flow;
mod line;
use cell::{Arena, Kind};
use flow::{Column, RawParagraph};
use line::Line;

fn one() -> f64 {
    1.
}
fn left() -> u8 {
    1
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CellSpec {
    Text {
        id: u32,
        width: f64,
        height: f64,
        #[serde(default)]
        valign: u8,
        #[serde(default)]
        stroke: u8,
    },
    Space {
        width: f64,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
    },
    Nbsp {
        width: f64,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
    },
    Tab {
        width: f64,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
    },
    Fraction {
        id: u32,
        top: Box<CellSpec>,
        bottom: Box<CellSpec>,
        #[serde(default)]
        stacking: u8,
        #[serde(default)]
        valign: u8,
    },
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TabStop {
    pub pos: f64,
    #[serde(default)]
    pub kind: u8,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnSpec {
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub height: Option<f64>,
    #[serde(default)]
    pub gutter: f64,
    #[serde(default)]
    pub margins: Vec<f64>,
    #[serde(default)]
    pub frame: Option<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParagraphSpec {
    pub cells: Vec<CellSpec>,
    #[serde(default = "left")]
    pub align: u8,
    #[serde(default)]
    pub indent: [f64; 3],
    #[serde(default = "one")]
    pub spacing: f64,
    #[serde(default)]
    pub margins: Vec<f64>,
    #[serde(default)]
    pub tabs: Arc<[TabStop]>,
    #[serde(default)]
    pub frame: Option<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Paragraph(ParagraphSpec),
    Empty {
        height: f64,
        #[serde(default = "one")]
        spacing: f64,
    },
    NextColumn,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub width: f64,
    #[serde(default)]
    pub height: Option<f64>,
    #[serde(default)]
    pub margins: Vec<f64>,
    pub columns: Vec<ColumnSpec>,
    pub events: Vec<Event>,
    #[serde(default)]
    pub at: [f64; 2],
    #[serde(default = "left")]
    pub align: u8,
    #[serde(default)]
    pub frame: Option<u32>,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Rect {
    pub left: f64,
    pub bottom: f64,
    pub right: f64,
    pub top: f64,
}
impl Rect {
    fn new(x: f64, y: f64, w: f64, h: f64) -> Result<Self> {
        Ok(Self {
            left: finite(x)?,
            top: finite(y)?,
            right: finite(x + w)?,
            bottom: finite(y - h)?,
        })
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Primitive {
    Text {
        id: u32,
        rect: Rect,
    },
    Background {
        id: u32,
        rect: Rect,
    },
    Line {
        id: u32,
        start: [f64; 2],
        end: [f64; 2],
    },
}
#[derive(Clone, Debug, Serialize)]
pub struct CellInfo {
    pub kind: &'static str,
    pub rect: Rect,
    pub locked: bool,
    pub offset: f64,
    pub children: Vec<CellInfo>,
}
#[derive(Clone, Debug, Serialize)]
pub struct LineInfo {
    pub rect: Rect,
    pub available_width: f64,
    pub cells: Vec<CellInfo>,
}
#[derive(Clone, Debug, Serialize)]
pub struct ParagraphInfo {
    pub source: usize,
    pub empty: bool,
    pub rect: Rect,
    pub distance: f64,
    pub lines: Vec<LineInfo>,
}
#[derive(Clone, Debug, Serialize)]
pub struct ColumnInfo {
    pub rect: Rect,
    pub used_height: f64,
    pub paragraphs: Vec<ParagraphInfo>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Output {
    pub rect: Rect,
    pub current_column: usize,
    pub columns: Vec<ColumnInfo>,
    pub primitives: Vec<Primitive>,
}

fn finite(v: f64) -> Result<f64> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(Error::Invalid("niet-eindige layoutgeometrie"))
    }
}
fn leading(h: f64, spacing: f64) -> Result<f64> {
    finite(h * 1.667 * spacing)
}
fn margins(v: &[f64]) -> Result<[f64; 4]> {
    let m = match v {
        [a, b, c, d] => [*a, *b, *c, *d],
        [a, b, c] => [*a, *b, *c, *b],
        [a, b] => [*a, *b, *a, *b],
        [a] => [*a; 4],
        _ => [0.; 4],
    };
    for &n in &m {
        finite(n)?;
    }
    Ok(m)
}
struct Budget<'a> {
    cancel: &'a AtomicBool,
    work: usize,
}
impl Budget<'_> {
    fn step(&mut self, n: usize) -> Result<()> {
        self.work = self
            .work
            .checked_add(n)
            .filter(|&v| v <= 20_000_000)
            .ok_or(Error::Limit("layoutwerkbudget"))?;
        if self.cancel.load(Ordering::Relaxed) {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
}

pub fn layout(request: Request, cancel: &AtomicBool) -> Result<Output> {
    let mut budget = Budget { cancel, work: 0 };
    budget.step(0)?;
    if request.events.len() > 200_000 {
        return Err(Error::Limit("layoutalinea's"));
    }
    finite(request.width)?;
    if let Some(h) = request.height {
        finite(h)?;
    }
    let m = margins(&request.margins)?;
    for &n in &request.at {
        finite(n)?;
    }
    let mut arena = Arena::default();
    let mut columns = Vec::new();
    for spec in request.columns {
        if columns.len() >= 1024 {
            return Err(Error::Limit("layoutkolommen"));
        }
        columns.push(Column::new(spec, request.width)?);
    }
    let mut current = 0usize;
    for (source, event) in request.events.into_iter().enumerate() {
        budget.step(1)?;
        match event {
            Event::NextColumn => {
                current += 1;
                if current >= columns.len() {
                    flow::new_column(&mut columns)?;
                }
            }
            Event::Paragraph(spec) => {
                let paragraph = RawParagraph::new(source, spec, &mut arena, &mut budget)?;
                flow::append(
                    paragraph,
                    &mut columns,
                    &mut current,
                    &mut arena,
                    &mut budget,
                )?;
            }
            Event::Empty { height, spacing } => {
                finite(height)?;
                finite(spacing)?;
                flow::append(
                    RawParagraph::empty(source, height, spacing)?,
                    &mut columns,
                    &mut current,
                    &mut arena,
                    &mut budget,
                )?;
            }
        }
    }
    let width = if columns.is_empty() {
        request.width
    } else {
        let mut w = 0.;
        for c in &columns[..columns.len() - 1] {
            w = finite(w + finite(c.width() + c.gutter)?)?;
        }
        finite(w + columns.last().unwrap().width())?
    };
    let height = if columns.is_empty() {
        request.height.unwrap_or(0.)
    } else {
        columns
            .iter()
            .map(Column::height)
            .try_fold(f64::NEG_INFINITY, |h, v| Ok::<_, Error>(h.max(v?)))?
    };
    let tw = finite(width + m[1] + m[3])?;
    let th = finite(height + m[0] + m[2])?;
    let dx = match request.align {
        2 | 5 | 8 => -tw / 2.,
        3 | 6 | 9 => -tw,
        _ => 0.,
    };
    let dy = match request.align {
        4..=6 => th / 2.,
        7..=9 => th,
        _ => 0.,
    };
    let x = finite(request.at[0] + dx)?;
    let y = finite(request.at[1] + dy)?;
    let rect = Rect::new(x, y, tw, th)?;
    let mut primitives = Vec::new();
    if let Some(id) = request.frame {
        primitives.push(Primitive::Background { id, rect });
    }
    let mut cx = finite(x + m[3])?;
    let cy = finite(y - m[0])?;
    let mut info = Vec::new();
    for column in columns {
        let w = column.width();
        let gutter = column.gutter;
        info.push(column.place(cx, cy, &mut arena, &mut primitives, &mut budget)?);
        cx = finite(cx + w + gutter)?;
    }
    Ok(Output {
        rect,
        current_column: current,
        columns: info,
        primitives,
    })
}
