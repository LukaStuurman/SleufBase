use super::cell::Glue;
use super::line::{self, Append};
use super::*;

#[derive(Clone)]
pub(super) struct RawParagraph {
    source: usize,
    ids: Arc<[usize]>,
    start: usize,
    align: u8,
    indent: [f64; 3],
    spacing: f64,
    m: [f64; 4],
    tabs: Arc<[TabStop]>,
    frame: Option<u32>,
    empty: Option<f64>,
}
impl RawParagraph {
    pub fn new(
        source: usize,
        spec: ParagraphSpec,
        arena: &mut Arena,
        budget: &mut Budget,
    ) -> Result<Self> {
        for &v in &spec.indent {
            finite(v)?;
        }
        finite(spec.spacing)?;
        let m = margins(&spec.margins)?;
        let mut ids = Vec::with_capacity(spec.cells.len());
        for cell in spec.cells {
            ids.push(arena.push_spec(cell, 0, budget)?);
        }
        Ok(Self {
            source,
            ids: arena.normalize(ids, budget)?,
            start: 0,
            align: spec.align,
            indent: spec.indent,
            spacing: spec.spacing,
            m,
            tabs: spec.tabs,
            frame: spec.frame,
            empty: None,
        })
    }
    pub fn empty(source: usize, height: f64, spacing: f64) -> Result<Self> {
        leading(height, spacing)?;
        Ok(Self {
            source,
            ids: Arc::default(),
            start: 0,
            align: 1,
            indent: [0.; 3],
            spacing,
            m: [0.; 4],
            tabs: Arc::default(),
            frame: None,
            empty: Some(height),
        })
    }
    fn distribute(
        self,
        width: f64,
        height: Option<f64>,
        arena: &mut Arena,
        budget: &mut Budget,
    ) -> Result<(Paragraph, Option<Self>)> {
        if let Some(h) = self.empty {
            return Ok((
                Paragraph {
                    source: self.source,
                    width,
                    content_height: h,
                    m: [0.; 4],
                    spacing: self.spacing,
                    indent: self.indent,
                    frame: None,
                    empty: true,
                    lines: Vec::new(),
                    distance: finite(leading(h, self.spacing)? - h)?,
                },
                None,
            ));
        }
        let content_width = finite(width - self.m[3] - self.m[1])?;
        if content_width < 1e-6 {
            return Err(Error::Invalid("geen bruikbare alineabreedte"));
        }
        let mut lines: Vec<Line> = Vec::new();
        let mut index = self.start;
        let mut first = true;
        let mut paragraph_height = finite(self.m[0] + self.m[2])?;
        while index < self.ids.len() {
            budget.step(1)?;
            let undo = index;
            let indent = self.indent[usize::from(!first)];
            let available = finite(content_width - finite(self.indent[2] + indent)?)?;
            let tabs = if matches!(self.align, 1 | 4) {
                line::tabs(arena, &self.tabs, -indent, available, budget)?
            } else {
                None
            };
            let mut line = Line::new(self.align, available, tabs)?;
            while index < self.ids.len() {
                budget.step(1)?;
                let cell = self.ids[index];
                let state = if matches!(
                    arena.cells[cell].kind,
                    Kind::Glue {
                        kind: Glue::Tab,
                        ..
                    }
                ) && line.has_tabs()
                {
                    let next = *self
                        .ids
                        .get(index + 1)
                        .ok_or(Error::Invalid("tab zonder volgende cel"))?;
                    let state = line.append_tab(next, cell, arena, budget)?;
                    if state == Append::Success || line.cells.is_empty() {
                        index += 1;
                    }
                    state
                } else {
                    line.append(cell, arena)?
                };
                match state {
                    Append::Success => index += 1,
                    Append::Fail => break,
                    Append::Forced => {
                        index += 1;
                        break;
                    }
                }
            }
            if !line.cells.is_empty() {
                line.trim_space(arena);
                if self
                    .ids
                    .get(index)
                    .is_some_and(|&id| arena.cells[id].is_space())
                {
                    index += 1;
                }
                let h = line.height(arena);
                if height.is_some_and(|limit| paragraph_height + h > limit) {
                    index = undo;
                    break;
                }
                first = false;
                paragraph_height = finite(paragraph_height + leading(h, self.spacing)?)?;
                lines.push(line);
            }
        }
        let rest = index < self.ids.len();
        if self.align == 4 {
            let end = if rest {
                lines.len()
            } else {
                lines.len().saturating_sub(1)
            };
            for line in &mut lines[..end] {
                line.justify(arena, budget)?;
            }
        }
        let mut content_height = 0.;
        let mut distance = 0.;
        if let Some(last) = lines.last() {
            for line in &lines[..lines.len() - 1] {
                content_height =
                    finite(content_height + leading(line.height(arena), self.spacing)?)?;
            }
            let h = last.height(arena);
            distance = finite(leading(h, self.spacing)? - h)?;
            content_height = finite(content_height + h)?;
        }
        let placed = Paragraph {
            source: self.source,
            width: finite(content_width + self.m[1] + self.m[3])?,
            content_height,
            m: self.m,
            spacing: self.spacing,
            indent: self.indent,
            frame: self.frame,
            empty: false,
            lines,
            distance,
        };
        let rest = if rest {
            let mut p = self;
            p.start = index;
            if !first {
                p.indent[0] = p.indent[1];
            }
            Some(p)
        } else {
            None
        };
        Ok((placed, rest))
    }
}
struct Paragraph {
    source: usize,
    width: f64,
    content_height: f64,
    m: [f64; 4],
    spacing: f64,
    indent: [f64; 3],
    frame: Option<u32>,
    empty: bool,
    lines: Vec<Line>,
    distance: f64,
}
impl Paragraph {
    fn height(&self) -> Result<f64> {
        finite(self.content_height + self.m[0] + self.m[2])
    }
    fn place(
        mut self,
        x: f64,
        y: f64,
        arena: &mut Arena,
        primitives: &mut Vec<Primitive>,
        budget: &mut Budget,
    ) -> Result<ParagraphInfo> {
        budget.step(1)?;
        if self.empty {
            return Ok(ParagraphInfo {
                source: self.source,
                empty: true,
                rect: Rect::new(0., 0., self.width, self.content_height)?,
                distance: self.distance,
                lines: Vec::new(),
            });
        }
        let rect = Rect::new(x, y, self.width, self.height()?)?;
        if let Some(id) = self.frame {
            if primitives.len() >= 1_000_000 {
                return Err(Error::Limit("layoutprimitieven"));
            }
            primitives.push(Primitive::Background { id, rect });
        }
        let left = finite(x + self.m[3])?;
        let mut top = finite(y - self.m[0])?;
        let mut info = Vec::new();
        for (i, line) in self.lines.iter_mut().enumerate() {
            budget.step(1)?;
            let indent = self.indent[usize::from(i != 0)];
            line.place(arena, finite(left + indent)?, top, budget)?;
            top = finite(top - leading(line.height(arena), self.spacing)?)?;
        }
        for line in self.lines {
            line.render(arena, primitives, budget)?;
            info.push(line.info(arena, budget)?);
        }
        Ok(ParagraphInfo {
            source: self.source,
            empty: false,
            rect,
            distance: self.distance,
            lines: info,
        })
    }
}
pub(super) struct Column {
    width: f64,
    height: Option<f64>,
    pub gutter: f64,
    m: [f64; 4],
    frame: Option<u32>,
    paragraphs: Vec<Paragraph>,
    completed: f64,
    used: f64,
}
impl Column {
    pub fn new(spec: ColumnSpec, reference: f64) -> Result<Self> {
        let width = spec.width.filter(|&w| w != 0.).unwrap_or(reference);
        finite(width)?;
        if let Some(h) = spec.height {
            finite(h)?;
        }
        finite(spec.gutter)?;
        Ok(Self {
            width,
            height: spec.height,
            gutter: spec.gutter,
            m: margins(&spec.margins)?,
            frame: spec.frame,
            paragraphs: Vec::new(),
            completed: 0.,
            used: 0.,
        })
    }
    pub fn width(&self) -> f64 {
        self.width + self.m[1] + self.m[3]
    }
    pub fn height(&self) -> Result<f64> {
        finite(self.height.unwrap_or(self.used) + self.m[0] + self.m[2])
    }
    fn clone_empty(&self) -> Result<Self> {
        // The original uses content_height here, including the current used
        // height of flexible columns. Keep that continuation contract.
        Ok(Self {
            width: self.width,
            height: Some(self.height.unwrap_or(self.used)),
            gutter: self.gutter,
            m: self.m,
            frame: self.frame,
            paragraphs: Vec::new(),
            completed: 0.,
            used: 0.,
        })
    }
    fn append(
        &mut self,
        raw: RawParagraph,
        arena: &mut Arena,
        budget: &mut Budget,
    ) -> Result<Option<RawParagraph>> {
        budget.step(1)?;
        let available = self.height.map(|h| finite(h - self.used)).transpose()?;
        let (paragraph, rest) = raw.distribute(self.width, available, arena, budget)?;
        if let Some(previous) = self.paragraphs.last() {
            self.completed =
                finite(self.completed + finite(previous.height()? + previous.distance)?)?;
        }
        self.used = finite(self.completed + paragraph.height()?)?;
        self.paragraphs.push(paragraph);
        Ok(rest)
    }
    pub fn place(
        self,
        x: f64,
        y: f64,
        arena: &mut Arena,
        primitives: &mut Vec<Primitive>,
        budget: &mut Budget,
    ) -> Result<ColumnInfo> {
        budget.step(1)?;
        let rect = Rect::new(x, y, self.width(), self.height()?)?;
        if let Some(id) = self.frame {
            if primitives.len() >= 1_000_000 {
                return Err(Error::Limit("layoutprimitieven"));
            }
            primitives.push(Primitive::Background { id, rect });
        }
        let left = finite(x + self.m[3])?;
        let mut top = finite(y - self.m[0])?;
        let mut info = Vec::new();
        for p in self.paragraphs {
            let h = p.height()?;
            let distance = p.distance;
            info.push(p.place(left, top, arena, primitives, budget)?);
            top = finite(top - finite(h + distance)?)?;
        }
        Ok(ColumnInfo {
            rect,
            used_height: self.used,
            paragraphs: info,
        })
    }
}
pub(super) fn new_column(columns: &mut Vec<Column>) -> Result<()> {
    if columns.len() >= 1024 {
        return Err(Error::Limit("layoutkolommen"));
    }
    let last = columns
        .last()
        .ok_or(Error::Invalid("geen layoutkolom"))?
        .clone_empty()?;
    columns.push(last);
    Ok(())
}
pub(super) fn append(
    raw: RawParagraph,
    columns: &mut Vec<Column>,
    current: &mut usize,
    arena: &mut Arena,
    budget: &mut Budget,
) -> Result<()> {
    let mut rest = raw;
    while *current < columns.len() {
        budget.step(1)?;
        if let Some(next) = columns[*current].append(rest, arena, budget)? {
            rest = next;
            *current += 1;
        } else {
            return Ok(());
        }
    }
    loop {
        budget.step(1)?;
        new_column(columns)?;
        *current = columns.len() - 1;
        let next = columns[*current].append(rest, arena, budget)?;
        if *current > 100 {
            return Err(Error::Invalid("onvoldoende kolomruimte"));
        }
        if let Some(next) = next {
            rest = next;
        } else {
            return Ok(());
        }
    }
}
