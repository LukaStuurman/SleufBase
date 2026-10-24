//! Modelspace selection for the map overlay, independent of section order.
//! Owner-priority semantics follow ezdxf 1.4.4 sections/entities.py (MIT,
//! Copyright 2011-2022 Manfred Moitzi; see third_party/ezdxf-LICENSE.txt).
use crate::dxf_tags::Group;
use std::sync::atomic::AtomicBool;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Scope {
    #[default]
    Base,
    Entity,
    BlockRecord,
    BlockBegin,
    Insert,
    Other,
}
#[derive(Default)]
struct Primary {
    scope: Scope,
    depth: usize,
    stopped: bool,
    has_entity: bool,
    has_insert: bool,
}
impl Primary {
    fn take(&mut self, g: &Group<'_>) -> Result<bool, String> {
        if self.stopped {
            return Ok(false);
        }
        if g.code == 102 {
            if g.value.starts_with('{') {
                self.depth += 1;
                if self.depth > 128 {
                    return Err("DXF APPDATA is te diep genest".into());
                }
            } else if g.value == "}" {
                self.depth = self
                    .depth
                    .checked_sub(1)
                    .ok_or("DXF APPDATA sluit zonder begin")?;
            }
            return Ok(false);
        }
        if self.depth > 0 {
            return Ok(false);
        }
        if g.code == 1001 || g.code == 101 {
            self.stopped = true;
            return Ok(false);
        }
        if g.code == 100 {
            self.scope = match g.value.as_ref() {
                "AcDbEntity" => Scope::Entity,
                "AcDbBlockTableRecord" => Scope::BlockRecord,
                "AcDbBlockBegin" => Scope::BlockBegin,
                "AcDbBlockReference" | "AcDbMInsertBlock" => Scope::Insert,
                _ => Scope::Other,
            };
            self.has_entity |= self.scope == Scope::Entity;
            self.has_insert |= self.scope == Scope::Insert;
            return Ok(false);
        }
        Ok(true)
    }
    fn finish(&self) -> Result<(), String> {
        if self.depth != 0 {
            Err("DXF APPDATA mist sluiting".into())
        } else {
            Ok(())
        }
    }
}
pub(crate) fn model_block(name: &str) -> bool {
    name.eq_ignore_ascii_case("*Model_Space") || name.eq_ignore_ascii_case("$Model_Space")
}
pub(crate) fn paper_block(name: &str) -> bool {
    name.get(..12)
        .is_some_and(|p| p.eq_ignore_ascii_case("*Paper_Space"))
        || name.eq_ignore_ascii_case("$Paper_Space")
}
pub(crate) fn block_name<'a>(groups: &'a [Group<'_>]) -> Result<&'a str, String> {
    let mut primary = Primary::default();
    let mut name = None;
    for g in groups {
        if primary.take(g)?
            && g.code == 2
            && matches!(primary.scope, Scope::Base | Scope::BlockBegin)
            && name.is_none()
        {
            name = Some(g.value.as_ref());
        }
    }
    primary.finish()?;
    name.ok_or_else(|| "DXF BLOCK mist naam".into())
}
pub(crate) fn attributes_follow(groups: &[Group<'_>]) -> Result<bool, String> {
    let mut primary = Primary::default();
    let mut base = false;
    let mut insert = false;
    for g in groups {
        if primary.take(g)? && g.code == 66 {
            if primary.scope == Scope::Base {
                base = g.value.trim() == "1";
            }
            if primary.scope == Scope::Insert {
                insert = g.value.trim() == "1";
            }
        }
        if primary.stopped {
            break;
        }
    }
    primary.finish()?;
    Ok(if primary.has_insert { insert } else { base })
}
#[derive(Default)]
pub(crate) struct Spaces {
    model: Option<String>,
    paper: Option<String>,
}
impl Spaces {
    pub(crate) fn scan<'a>(
        records: impl Iterator<Item = Result<Group<'a>, String>>,
        cancel: &AtomicBool,
    ) -> Result<Self, String> {
        let mut out = Self::default();
        let mut tables = false;
        let mut section = false;
        let mut record = false;
        let mut primary = Primary::default();
        let mut name = None;
        let mut handle = None;
        for (i, g) in records.enumerate() {
            if i.is_multiple_of(1024) {
                crate::input::check(cancel)?;
            }
            if i >= 80_000_000 {
                return Err("DXF bevat te veel velden".into());
            }
            let g = g?;
            if g.code == 0 {
                if record {
                    primary.finish()?;
                    out.record(name.take(), handle.take())?;
                }
                record = false;
                match g.value.as_ref() {
                    "SECTION" => {
                        section = true;
                        tables = false;
                    }
                    "ENDSEC" if tables => {
                        crate::input::check(cancel)?;
                        return Ok(out);
                    }
                    "ENDSEC" | "EOF" => {
                        tables = false;
                        section = false;
                    }
                    "BLOCK_RECORD" if tables => {
                        record = true;
                        primary = Primary::default();
                    }
                    _ => {}
                }
            } else if section && g.code == 2 {
                tables = g.value == "TABLES";
                section = false;
            } else if record && primary.take(&g)? {
                if g.code == 5 && primary.scope == Scope::Base && handle.is_none() {
                    handle = Some(g.value.to_string());
                }
                if g.code == 2
                    && matches!(primary.scope, Scope::Base | Scope::BlockRecord)
                    && name.is_none()
                {
                    name = Some(g.value.to_string());
                }
            }
        }
        if record {
            primary.finish()?;
            out.record(name, handle)?;
        }
        crate::input::check(cancel)?;
        Ok(out)
    }
    fn record(&mut self, name: Option<String>, handle: Option<String>) -> Result<(), String> {
        let Some(name) = name else {
            return Err("DXF BLOCK_RECORD mist naam".into());
        };
        let destination = if model_block(&name) {
            &mut self.model
        } else if name.eq_ignore_ascii_case("*Paper_Space")
            || name.eq_ignore_ascii_case("$Paper_Space")
        {
            &mut self.paper
        } else {
            return Ok(());
        };
        if let Some(handle) = handle.filter(|h| !h.is_empty()) {
            if destination.is_some() {
                return Err("DXF bevat dubbele model-/papierruimterecords".into());
            }
            *destination = Some(handle);
        }
        Ok(())
    }
    pub(crate) fn is_paper(&self, groups: &[Group<'_>]) -> Result<bool, String> {
        let mut primary = Primary::default();
        let mut owner = None;
        let mut base_paper = false;
        let mut entity_paper = false;
        for g in groups {
            if !primary.take(g)? {
                if primary.stopped
                    || primary.has_entity && !matches!(primary.scope, Scope::Base | Scope::Entity)
                {
                    break;
                }
                continue;
            }
            if g.code == 330 && primary.scope == Scope::Base && owner.is_none() {
                owner = Some(g.value.as_ref());
            }
            if g.code == 67 && matches!(primary.scope, Scope::Entity | Scope::Base) {
                let paper = g
                    .value
                    .trim()
                    .parse::<i32>()
                    .map_err(|_| "Ongeldige CAD-ruimtevlag")?
                    != 0;
                if primary.scope == Scope::Entity {
                    entity_paper = paper;
                } else {
                    base_paper = paper;
                }
            }
        }
        primary.finish()?;
        // Match the original loader's literal handle priority. Layout tab names
        // (410) do not override a known owner or the fallback 67 flag.
        if owner.is_some() && owner == self.model.as_deref() {
            return Ok(false);
        }
        if owner.is_some() && owner == self.paper.as_deref() {
            return Ok(true);
        }
        Ok(if primary.has_entity {
            entity_paper
        } else {
            base_paper
        })
    }
}
#[derive(Clone, Copy)]
enum Sequence {
    Attributes,
    Vertices,
}
#[derive(Default)]
pub(crate) struct Skipped {
    sequence: Option<Sequence>,
}
impl Skipped {
    pub(crate) fn pending(&self) -> bool {
        self.sequence.is_some()
    }
    pub(crate) fn push(&mut self, kind: &str, groups: &[Group<'_>]) -> Result<(), String> {
        if let Some(sequence) = self.sequence {
            if kind == "SEQEND" {
                self.sequence = None;
                return Ok(());
            }
            let allowed = match sequence {
                Sequence::Attributes => "ATTRIB",
                Sequence::Vertices => "VERTEX",
            };
            if kind != allowed {
                return Err("DXF-papierruimtesequentie mist SEQEND".into());
            }
        } else if kind == "POLYLINE" {
            self.sequence = Some(Sequence::Vertices);
        } else if kind == "INSERT" && attributes_follow(groups)? {
            self.sequence = Some(Sequence::Attributes);
        }
        Ok(())
    }
}
