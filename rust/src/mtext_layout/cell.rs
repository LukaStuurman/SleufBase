use super::*;
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Glue {
    Space,
    Nbsp,
    Tab,
}
#[derive(Clone, Debug)]
pub(super) enum Kind {
    Text {
        id: u32,
        stroke: u8,
    },
    Glue {
        kind: Glue,
        min: f64,
        max: Option<f64>,
    },
    Fraction {
        id: u32,
        top: usize,
        bottom: usize,
        stacking: u8,
    },
    Group(Arc<[usize]>),
}
#[derive(Clone, Debug)]
pub(super) struct Cell {
    pub w: f64,
    pub h: f64,
    pub valign: u8,
    pub kind: Kind,
    pub position: [f64; 2],
}
impl Cell {
    pub fn is_glue(&self) -> bool {
        matches!(self.kind, Kind::Glue { .. })
    }
    pub fn is_space(&self) -> bool {
        matches!(
            self.kind,
            Kind::Glue {
                kind: Glue::Space,
                ..
            }
        )
    }
    pub fn content(&self) -> bool {
        matches!(self.kind, Kind::Text { .. } | Kind::Fraction { .. })
    }
    pub fn no_break(&self) -> bool {
        matches!(
            self.kind,
            Kind::Text { .. }
                | Kind::Glue {
                    kind: Glue::Nbsp,
                    ..
                }
        )
    }
    pub fn name(&self) -> &'static str {
        match self.kind {
            Kind::Text { .. } => "text",
            Kind::Glue {
                kind: Glue::Space, ..
            } => "space",
            Kind::Glue {
                kind: Glue::Nbsp, ..
            } => "nbsp",
            Kind::Glue {
                kind: Glue::Tab, ..
            } => "tab",
            Kind::Fraction { .. } => "fraction",
            Kind::Group(..) => "rigid",
        }
    }
}
#[derive(Default)]
pub(super) struct Arena {
    pub cells: Vec<Cell>,
    pub tabs: std::collections::BTreeMap<(usize, u64, u64), Arc<super::line::TabIndex>>,
}
impl Arena {
    fn add(&mut self, cell: Cell) -> Result<usize> {
        if self.cells.len() >= 500_000 {
            return Err(Error::Limit("layoutcellen"));
        }
        finite(cell.w)?;
        finite(cell.h)?;
        let id = self.cells.len();
        self.cells.push(cell);
        Ok(id)
    }
    pub fn push_spec(
        &mut self,
        spec: CellSpec,
        depth: usize,
        budget: &mut Budget,
    ) -> Result<usize> {
        budget.step(1)?;
        if depth > 64 {
            return Err(Error::Limit("geneste layoutcellen"));
        }
        let cell = match spec {
            CellSpec::Text {
                id,
                width,
                height,
                valign,
                stroke,
            } => Cell {
                w: width,
                h: height,
                valign,
                kind: Kind::Text { id, stroke },
                position: [0.; 2],
            },
            CellSpec::Space { width, min, max }
            | CellSpec::Nbsp { width, min, max }
            | CellSpec::Tab { width, min, max } => {
                let kind = match &spec {
                    CellSpec::Space { .. } => Glue::Space,
                    CellSpec::Nbsp { .. } => Glue::Nbsp,
                    _ => Glue::Tab,
                };
                if let Some(v) = max {
                    finite(v)?;
                }
                let min = min.filter(|v| *v != 0.).unwrap_or(width);
                finite(min)?;
                Cell {
                    w: width,
                    h: 0.,
                    valign: 0,
                    kind: Kind::Glue { kind, min, max },
                    position: [0.; 2],
                }
            }
            CellSpec::Fraction {
                id,
                top,
                bottom,
                stacking,
                valign,
            } => {
                let top = self.push_spec(*top, depth + 1, budget)?;
                let bottom = self.push_spec(*bottom, depth + 1, budget)?;
                let (a, b) = (&self.cells[top], &self.cells[bottom]);
                let h = if stacking == 2 {
                    a.h + b.h
                } else {
                    1.2 * (a.h + b.h)
                };
                let w = if stacking == 2 {
                    a.w + b.w
                } else {
                    a.w.max(b.w)
                };
                Cell {
                    w,
                    h,
                    valign,
                    kind: Kind::Fraction {
                        id,
                        top,
                        bottom,
                        stacking,
                    },
                    position: [0.; 2],
                }
            }
        };
        if cell.valign > 2 {
            return Err(Error::Invalid("verticale celuitlijning"));
        }
        self.add(cell)
    }
    pub fn normalize(&mut self, ids: Vec<usize>, budget: &mut Budget) -> Result<Arc<[usize]>> {
        let mut normalized: Vec<usize> = Vec::new();
        for (at, &id) in ids.iter().enumerate() {
            budget.step(1)?;
            if self.cells[id].content()
                && normalized.last().is_some_and(|&p| self.cells[p].content())
            {
                return Err(Error::Invalid("geen lijm tussen tekstcellen"));
            }
            if matches!(
                self.cells[id].kind,
                Kind::Glue {
                    kind: Glue::Nbsp,
                    ..
                }
            ) {
                let keep = normalized.last().is_some_and(|&p| self.cells[p].no_break())
                    && ids.get(at + 1).is_some_and(|&p| self.cells[p].no_break());
                if !keep {
                    self.convert_space(id);
                    for &p in normalized.iter().rev() {
                        if matches!(
                            self.cells[p].kind,
                            Kind::Glue {
                                kind: Glue::Nbsp,
                                ..
                            }
                        ) {
                            self.convert_space(p);
                        } else {
                            break;
                        }
                    }
                }
            }
            normalized.push(id);
        }
        while normalized
            .last()
            .is_some_and(|&id| self.cells[id].is_glue())
        {
            normalized.pop();
        }
        let mut out = Vec::new();
        let mut index = 0;
        while index < normalized.len() {
            budget.step(1)?;
            let start = index;
            if self.cells[normalized[index]].no_break() {
                index += 1;
                while index < normalized.len() && self.cells[normalized[index]].no_break() {
                    index += 1;
                }
            } else {
                index += 1;
            }
            if index - start == 1 {
                out.push(normalized[start]);
            } else {
                let children: Arc<[usize]> = Arc::from(normalized[start..index].to_vec());
                let w = children
                    .iter()
                    .try_fold(0., |s, &i| finite(s + self.cells[i].w))?;
                let h = children
                    .iter()
                    .fold(f64::NEG_INFINITY, |s, &i| s.max(self.cells[i].h));
                out.push(self.add(Cell {
                    w,
                    h,
                    valign: 0,
                    kind: Kind::Group(children),
                    position: [0.; 2],
                })?);
            }
        }
        Ok(Arc::from(out))
    }
    fn convert_space(&mut self, id: usize) {
        if let Kind::Glue { kind, .. } = &mut self.cells[id].kind {
            *kind = Glue::Space;
        }
    }
    pub fn can_grow(&self, id: usize) -> bool {
        match self.cells[id].kind {
            Kind::Glue { max, .. } => max.is_none_or(|m| self.cells[id].w < m),
            _ => false,
        }
    }
    pub fn resize(&mut self, id: usize, width: f64) -> Result<()> {
        if let Kind::Glue { min, max, .. } = self.cells[id].kind {
            self.cells[id].w = finite(max.map_or(width, |m| width.min(m)).max(min))?;
        }
        Ok(())
    }
    pub fn refresh_group(&mut self, id: usize) -> Result<()> {
        if let Kind::Group(ids) = &self.cells[id].kind {
            let w = ids
                .iter()
                .try_fold(0., |s, &i| finite(s + self.cells[i].w))?;
            self.cells[id].w = w;
        }
        Ok(())
    }
    pub fn place(&mut self, id: usize, x: f64, y: f64, budget: &mut Budget) -> Result<()> {
        budget.step(1)?;
        finite(x)?;
        finite(y)?;
        if self.cells[id].is_glue() {
            return Ok(());
        }
        self.cells[id].position = [x, y];
        match self.cells[id].kind.clone() {
            Kind::Group(ids) => {
                let mut x = x;
                for &child in ids.iter() {
                    self.place(child, x, y, budget)?;
                    x = finite(x + self.cells[child].w)?;
                }
            }
            Kind::Fraction {
                top,
                bottom,
                stacking,
                ..
            } => {
                let (w, h) = (self.cells[id].w, self.cells[id].h);
                let (a, b) = (self.cells[top].w, self.cells[bottom].w);
                if stacking == 2 {
                    self.place(top, x, y, budget)?;
                    self.place(
                        bottom,
                        finite(x + w - b)?,
                        finite(y - h + self.cells[bottom].h)?,
                        budget,
                    )?;
                } else {
                    let center = finite(x + w / 2.)?;
                    self.place(top, finite(center - a / 2.)?, y, budget)?;
                    self.place(
                        bottom,
                        finite(center - b / 2.)?,
                        finite(y - h + self.cells[bottom].h)?,
                        budget,
                    )?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub fn rect(&self, id: usize) -> Result<Rect> {
        let c = &self.cells[id];
        Rect::new(c.position[0], c.position[1], c.w, c.h)
    }
    pub fn info(
        &self,
        id: usize,
        locked: bool,
        offset: f64,
        budget: &mut Budget,
    ) -> Result<CellInfo> {
        budget.step(1)?;
        let children = match &self.cells[id].kind {
            Kind::Group(ids) => ids.to_vec(),
            Kind::Fraction { top, bottom, .. } => vec![*top, *bottom],
            _ => Vec::new(),
        };
        let mut info = Vec::new();
        for child in children {
            info.push(self.info(child, false, 0., budget)?);
        }
        Ok(CellInfo {
            kind: self.cells[id].name(),
            rect: self.rect(id)?,
            locked,
            offset,
            children: info,
        })
    }
    pub fn render(&self, id: usize, out: &mut Vec<Primitive>, budget: &mut Budget) -> Result<()> {
        budget.step(1)?;
        if out.len() >= 1_000_000 {
            return Err(Error::Limit("layoutprimitieven"));
        }
        let c = &self.cells[id];
        match &c.kind {
            Kind::Text { id, .. } => out.push(Primitive::Text {
                id: *id,
                rect: Rect::new(c.position[0], c.position[1], c.w, c.h)?,
            }),
            Kind::Group(ids) => {
                for &child in ids.iter() {
                    self.render(child, out, budget)?;
                }
                self.strokes(ids, out, budget)?;
            }
            Kind::Fraction {
                id,
                top,
                bottom,
                stacking,
            } => {
                self.render(*top, out, budget)?;
                self.render(*bottom, out, budget)?;
                if *stacking != 0 {
                    let (x, y) = (c.position[0], c.position[1]);
                    let (a, b) = if *stacking == 1 {
                        ([x, y - c.h / 2.], [x + c.w, y - c.h / 2.])
                    } else {
                        let d = c.w.min(c.h) / 2.;
                        let cx = x + self.cells[*top].w;
                        let cy = y - self.cells[*top].h;
                        ([cx - d, cy - d], [cx + d, cy + d])
                    };
                    self.push_line(*id, a, b, out)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn push_line(
        &self,
        id: u32,
        start: [f64; 2],
        end: [f64; 2],
        out: &mut Vec<Primitive>,
    ) -> Result<()> {
        if out.len() >= 1_000_000 {
            return Err(Error::Limit("layoutprimitieven"));
        }
        for &v in start.iter().chain(end.iter()) {
            finite(v)?;
        }
        out.push(Primitive::Line { id, start, end });
        Ok(())
    }
    pub fn strokes(
        &self,
        ids: &[usize],
        out: &mut Vec<Primitive>,
        budget: &mut Budget,
    ) -> Result<()> {
        for (i, &index) in ids.iter().enumerate() {
            budget.step(1)?;
            let c = &self.cells[index];
            if let Kind::Text { id, stroke } = c.kind {
                if stroke == 0 {
                    continue;
                }
                let mut extend = 0.;
                if stroke & 8 != 0 {
                    for &next in &ids[i + 1..] {
                        budget.step(1)?;
                        if !self.cells[next].is_glue() {
                            break;
                        }
                        extend = finite(extend + self.cells[next].w)?;
                    }
                }
                let [left, top] = c.position;
                let bottom = finite(top - c.h)?;
                let right = finite(left + c.w + extend)?;
                let spacing = c.h / 5.;
                if stroke & 1 != 0 {
                    let y = finite(bottom - spacing)?;
                    self.push_line(id, [left, y], [right, y], out)?;
                }
                if stroke & 2 != 0 {
                    let y = finite((top + bottom) / 2.)?;
                    self.push_line(id, [left, y], [right, y], out)?;
                }
                if stroke & 4 != 0 {
                    let y = finite(top + spacing)?;
                    self.push_line(id, [left, y], [right, y], out)?;
                }
            }
        }
        Ok(())
    }
}
