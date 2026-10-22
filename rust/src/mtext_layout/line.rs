use super::cell::Glue;
use super::*;

pub(super) struct TabIndex {
    _source: Arc<[TabStop]>,
    raw: Arc<[TabStop]>,
    ordered: [Vec<(f64, usize)>; 3],
    suffix: [Vec<usize>; 3],
}
impl TabIndex {
    pub fn new(
        tabs: &Arc<[TabStop]>,
        offset: f64,
        right: f64,
        budget: &mut Budget,
    ) -> Result<Arc<Self>> {
        let mut raw = Vec::new();
        for tab in tabs.iter() {
            budget.step(1)?;
            let pos = finite(tab.pos + offset)?;
            if pos > 0. && pos <= right {
                raw.push(TabStop {
                    pos,
                    kind: tab.kind,
                });
            }
        }
        let mut ordered: [Vec<(f64, usize)>; 3] = std::array::from_fn(|_| Vec::new());
        for (i, t) in raw.iter().enumerate() {
            if let Some(list) = ordered.get_mut(usize::from(t.kind)) {
                list.push((t.pos, i));
            }
        }
        let mut suffix: [Vec<usize>; 3] = std::array::from_fn(|_| Vec::new());
        for kind in 0..3 {
            let list = &mut ordered[kind];
            if !list.is_empty() {
                budget.step(list.len().saturating_mul(list.len().ilog2() as usize + 1))?;
            }
            list.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            suffix[kind].resize(list.len(), 0);
            let mut minimum = usize::MAX;
            for i in (0..list.len()).rev() {
                minimum = minimum.min(list[i].1);
                suffix[kind][i] = minimum;
            }
        }
        Ok(Arc::new(Self {
            _source: tabs.clone(),
            raw: Arc::from(raw),
            ordered,
            suffix,
        }))
    }
    fn next(&self, left: f64, center: f64, right: f64) -> Option<TabStop> {
        let thresholds = [left, right, center];
        let mut selected = None;
        for (kind, &threshold) in thresholds.iter().enumerate() {
            let at = self.ordered[kind].partition_point(|&(pos, _)| pos <= threshold);
            if let Some(&i) = self.suffix[kind].get(at) {
                selected = Some(selected.map_or(i, |old: usize| old.min(i)));
            }
        }
        selected.map(|i| self.raw[i])
    }
}
pub(super) fn tabs(
    arena: &mut Arena,
    source: &Arc<[TabStop]>,
    offset: f64,
    right: f64,
    budget: &mut Budget,
) -> Result<Option<Arc<TabIndex>>> {
    if source.is_empty() {
        return Ok(None);
    }
    let key = (
        Arc::as_ptr(source) as *const TabStop as usize,
        offset.to_bits(),
        right.to_bits(),
    );
    if let Some(tabs) = arena.tabs.get(&key) {
        return Ok(Some(tabs.clone()));
    }
    let tabs = TabIndex::new(source, offset, right, budget)?;
    arena.tabs.insert(key, tabs.clone());
    Ok(Some(tabs))
}
#[derive(Clone, Copy)]
pub(super) struct Entry {
    pub cell: usize,
    pub offset: f64,
    pub locked: bool,
    pub as_space: bool,
}
#[derive(PartialEq)]
pub(super) enum Append {
    Fail,
    Success,
    Forced,
}
pub(super) struct Line {
    pub align: u8,
    pub available: f64,
    pub cells: Vec<Entry>,
    offset: f64,
    tabs: Option<Arc<TabIndex>>,
    position: [f64; 2],
}
impl Line {
    pub fn new(align: u8, width: f64, tabs: Option<Arc<TabIndex>>) -> Result<Self> {
        finite(width)?;
        if !(1..=4).contains(&align) {
            return Err(Error::Invalid("alinea-uitlijning"));
        }
        Ok(Self {
            align,
            available: width,
            cells: Vec::new(),
            offset: 0.,
            tabs,
            position: [0.; 2],
        })
    }
    pub fn has_tabs(&self) -> bool {
        matches!(self.align, 1 | 4)
    }
    fn push(&mut self, cell: usize, offset: f64, locked: bool, as_space: bool) {
        self.cells.push(Entry {
            cell,
            offset,
            locked,
            as_space,
        });
    }
    pub fn append(&mut self, cell: usize, arena: &Arena) -> Result<Append> {
        let width = arena.cells[cell].w;
        let end = finite(self.offset + width)?;
        let fits = if self.has_tabs() {
            end <= self.available
        } else {
            end < self.available
        };
        let as_space = !self.has_tabs()
            && matches!(
                arena.cells[cell].kind,
                Kind::Glue {
                    kind: Glue::Tab,
                    ..
                }
            );
        if fits {
            self.push(cell, self.offset, false, as_space);
            self.offset = end;
            Ok(Append::Success)
        } else if self.cells.is_empty() {
            self.push(cell, 0., false, as_space);
            Ok(Append::Forced)
        } else {
            Ok(Append::Fail)
        }
    }
    pub fn append_tab(
        &mut self,
        cell: usize,
        tab: usize,
        arena: &Arena,
        budget: &mut Budget,
    ) -> Result<Append> {
        budget.step(1)?;
        let width = arena.cells[cell].w;
        let pos = self.offset;
        let end = finite(pos + width)?;
        if end > self.available {
            return Ok(Append::Fail);
        }
        let target = self
            .tabs
            .as_ref()
            .and_then(|t| t.next(pos, pos + width / 2., end));
        if let Some(target) = target {
            let at = target.pos;
            match target.kind {
                0 => {
                    let end = finite(at + width)?;
                    if end <= self.available {
                        self.push(cell, at, true, false);
                        self.offset = end;
                        return Ok(Append::Success);
                    }
                }
                2 => {
                    let half = width / 2.;
                    if pos + half > at {
                        return self.append(cell, arena);
                    }
                    let end = finite(at + half)?;
                    if end <= self.available {
                        self.push(cell, finite(at - half)?, true, false);
                        self.offset = end;
                        return Ok(Append::Success);
                    }
                }
                _ => {
                    if end > at {
                        return self.append(cell, arena);
                    }
                    self.push(cell, finite(at - width)?, true, false);
                    self.offset = at;
                    return Ok(Append::Success);
                }
            }
            Ok(Append::Fail)
        } else {
            let at = finite(pos + arena.cells[tab].w)?;
            let end = finite(at + width)?;
            if end <= self.available {
                self.push(tab, pos, false, true);
                self.push(cell, at, false, false);
                self.offset = end;
                Ok(Append::Success)
            } else {
                Ok(Append::Fail)
            }
        }
    }
    pub fn trim_space(&mut self, arena: &Arena) {
        if self
            .cells
            .last()
            .is_some_and(|e| e.as_space || arena.cells[e.cell].is_space())
        {
            self.cells.pop();
        }
    }
    pub fn height(&self, arena: &Arena) -> f64 {
        if self.cells.is_empty() {
            0.
        } else {
            self.cells
                .iter()
                .fold(f64::NEG_INFINITY, |h, e| h.max(arena.cells[e.cell].h))
        }
    }
    pub fn width(&self, arena: &Arena) -> Result<f64> {
        self.cells
            .last()
            .map_or(Ok(0.), |e| finite(e.offset + arena.cells[e.cell].w))
    }
    pub fn justify(&mut self, arena: &mut Arena, budget: &mut Budget) -> Result<()> {
        if self.cells.is_empty() {
            return Ok(());
        }
        let mut locked = 0;
        for i in (1..self.cells.len()).rev() {
            if self.cells[i].locked {
                locked = i;
                break;
            }
        }
        let available = finite(
            self.available
                - finite(self.cells[locked].offset + arena.cells[self.cells[locked].cell].w)?,
        )?;
        let tail: Vec<_> = self.cells[locked + 1..].iter().map(|e| e.cell).collect();
        let mut modified = false;
        let groups: Vec<_> = tail
            .iter()
            .copied()
            .filter(|&id| matches!(arena.cells[id].kind, Kind::Group(..)))
            .collect();
        loop {
            let mut growable = Vec::new();
            let mut width = 0.;
            for &id in &tail {
                budget.step(1)?;
                width = finite(width + arena.cells[id].w)?;
                if arena.can_grow(id) {
                    growable.push(id);
                } else if let Kind::Group(children) = &arena.cells[id].kind {
                    for &child in children.iter() {
                        budget.step(1)?;
                        if arena.can_grow(child) {
                            growable.push(child);
                        }
                    }
                }
            }
            if growable.is_empty() {
                break;
            }
            let space = finite(available - width)?;
            if space <= 1e-9 {
                break;
            }
            let delta = space / growable.len() as f64;
            let mut changed = false;
            for id in growable {
                budget.step(1)?;
                let before = arena.cells[id].w;
                arena.resize(id, finite(before + delta)?)?;
                changed |= arena.cells[id].w != before;
            }
            if !changed {
                return Err(Error::Invalid("uitvulling zonder voortgang"));
            }
            for &id in &groups {
                if let Kind::Group(children) = &arena.cells[id].kind {
                    budget.step(children.len())?;
                }
                arena.refresh_group(id)?;
            }
            modified = true;
        }
        if modified {
            let mut offset =
                finite(self.cells[locked].offset + arena.cells[self.cells[locked].cell].w)?;
            for entry in &mut self.cells[locked + 1..] {
                entry.offset = offset;
                entry.locked = false;
                offset = finite(offset + arena.cells[entry.cell].w)?;
            }
        }
        Ok(())
    }
    pub fn place(&mut self, arena: &mut Arena, x: f64, y: f64, budget: &mut Budget) -> Result<()> {
        let x = if self.has_tabs() {
            x
        } else {
            let width = self
                .cells
                .iter()
                .try_fold(0., |s, e| finite(s + arena.cells[e.cell].w))?;
            finite(
                x + if self.align == 3 {
                    (self.available - width) / 2.
                } else {
                    self.available - width
                },
            )?
        };
        self.position = [x, y];
        let height = self.height(arena);
        for e in &self.cells {
            budget.step(1)?;
            let cell = &arena.cells[e.cell];
            let dy = if !cell.is_glue() && cell.valign != 2 {
                let dy = cell.h - height;
                if cell.valign == 1 {
                    dy / 2.
                } else {
                    dy
                }
            } else {
                0.
            };
            arena.place(e.cell, finite(x + e.offset)?, finite(y + dy)?, budget)?;
        }
        Ok(())
    }
    pub fn render(
        &self,
        arena: &Arena,
        out: &mut Vec<Primitive>,
        budget: &mut Budget,
    ) -> Result<()> {
        for e in &self.cells {
            arena.render(e.cell, out, budget)?;
        }
        let ids: Vec<_> = self.cells.iter().map(|e| e.cell).collect();
        arena.strokes(&ids, out, budget)
    }
    pub fn info(&self, arena: &Arena, budget: &mut Budget) -> Result<LineInfo> {
        let mut cells = Vec::new();
        for e in &self.cells {
            let mut info = arena.info(e.cell, e.locked, e.offset, budget)?;
            if e.as_space {
                info.kind = "space";
            }
            cells.push(info);
        }
        Ok(LineInfo {
            rect: Rect::new(
                self.position[0],
                self.position[1],
                self.width(arena)?,
                self.height(arena),
            )?,
            available_width: self.available,
            cells,
        })
    }
}
