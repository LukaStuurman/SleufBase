//! Profile/map primitives and existing-block inserts in a preserved template.
//! Append a collected batch atomically. Source blocks/styles remain shared.
use crate::{
    dxf_document::{AddedBlock, Change, Document, RecordView},
    python_values::casefold,
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt::{Display, Write},
    sync::atomic::AtomicBool,
};
pub type XY = [f64; 2];
#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct TextPlacement {
    pub insert: XY,
    pub height: f64,
    pub rotation: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct AttributeMetrics {
    pub offset: XY,
    pub height: f64,
}
#[derive(Clone, Debug, serde::Deserialize)]
pub struct Layer {
    pub name: String,
    pub color: u16,
    pub true_color: Option<u32>,
    pub lineweight: Option<i16>,
}
#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct Paint {
    pub color: Option<u16>,
    pub true_color: Option<u32>,
    pub lineweight: Option<i16>,
}
pub struct CircleBlock<'a> {
    pub name: &'a str,
    pub center: [f64; 3],
    pub radius: f64,
    pub layer: &'a str,
    pub paint: Paint,
}
/// Declare a reusable circle block atomically, retaining any existing block.
pub fn ensure_circle_block(
    document: &mut Document,
    definition: CircleBlock<'_>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    crate::input::check(cancel)?;
    crate::object_dxf::layer_name(definition.name)?;
    let key = casefold(definition.name);
    if document.added_blocks.contains_key(&key)
        || document
            .records()
            .any(|r| r.kind() == "BLOCK" && r.first(2).is_some_and(|n| casefold(n) == key))
    {
        return Ok(());
    }
    if document.added_blocks.len() >= 10_000 {
        return Err("Te veel nieuwe sjabloonblokken".into());
    }
    finite(&definition.center)?;
    finite(&[definition.radius])?;
    paint(&definition.paint)?;
    if definition.radius <= 0. || !document.has_layer(definition.layer) {
        return Err("Ongeldige blokcirkel of ontbrekende laag".into());
    }
    let mut table = None;
    let mut end = None;
    for r in document.records() {
        if r.kind() == "TABLE" && r.first(2) == Some("BLOCK_RECORD") {
            table = Some(r);
        } else if table.is_some() && r.kind() == "ENDTAB" {
            end = Some(r);
            break;
        }
    }
    let table = table.ok_or("DXF mist bloktabel")?;
    let owner = table.handle().ok_or("Bloktabel mist handle")?;
    let at = document.record_start(end.ok_or("Bloktabel is niet afgesloten")?.index());
    let (id, mut seed) = document.seed()?;
    let next = id.checked_add(4).ok_or("Geen vrije DXF-handles")?;
    let mut record = Tags::default();
    record.record("BLOCK_RECORD", id, owner);
    record.tag(100, "AcDbSymbolTableRecord");
    record.tag(100, "AcDbBlockTableRecord");
    record.tag(2, definition.name);
    record.tag(340, 0);
    record.tag(70, 0);
    record.tag(280, 1);
    record.tag(281, 0);
    let mut block = Tags::default();
    block.record("BLOCK", id + 1, id);
    block.tag(100, "AcDbEntity");
    block.tag(8, "0");
    block.tag(100, "AcDbBlockBegin");
    block.tag(2, definition.name);
    block.tag(70, 0);
    block.point(10, [0.; 3]);
    block.tag(3, definition.name);
    block.tag(1, "");
    block.entity(
        "CIRCLE",
        id + 3,
        id,
        false,
        definition.layer,
        &definition.paint,
    );
    block.tag(100, "AcDbCircle");
    block.point(10, definition.center);
    block.tag(40, definition.radius);
    block.record("ENDBLK", id + 2, id);
    block.tag(100, "AcDbEntity");
    block.tag(8, "0");
    block.tag(100, "AcDbBlockEnd");
    let block_at = document.section_end("BLOCKS")?;
    let count = document
        .records()
        .filter(|r| r.kind() == "BLOCK_RECORD")
        .count()
        + document.added_blocks.len()
        + 1;
    let count = document.tag_change(table.index(), 70, &count.to_string())?;
    seed.text = format!("{next:X}");
    crate::input::check(cancel)?;
    document.apply(vec![
        Change {
            start: at,
            end: at,
            text: record.0,
        },
        Change {
            start: block_at,
            end: block_at,
            text: block.0,
        },
        count,
        seed,
    ])?;
    document.added_blocks.insert(
        key,
        AddedBlock {
            base: [0.; 3],
            attributes: Vec::new(),
        },
    );
    Ok(())
}
#[derive(Clone, Debug, serde::Deserialize)]
pub enum Shape {
    Polyline {
        vertices: Vec<XY>,
        closed: bool,
    },
    Circle {
        center: XY,
        radius: f64,
    },
    Text {
        text: String,
        insert: XY,
        height: f64,
        rotation: f64,
        style: String,
        halign: u8,
        valign: u8,
    },
    Block {
        name: String,
        insert: XY,
        scale: f64,
        attributes: BTreeMap<String, String>,
        #[serde(default)]
        attribute_placements: BTreeMap<String, TextPlacement>,
    },
}
#[derive(Clone, Debug, serde::Deserialize)]
pub struct Entity {
    pub layer: String,
    #[serde(default)]
    pub paint: Paint,
    pub shape: Shape,
    /// None is model space. Active/inactive paper layouts retain correct owners.
    pub layout: Option<String>,
}
#[derive(Default)]
struct Tags(String);
impl Tags {
    fn tag(&mut self, code: i32, value: impl Display) {
        writeln!(self.0, "{code}\n{value}").unwrap();
    }
    fn point(&mut self, code: i32, xyz: [f64; 3]) {
        for (offset, v) in xyz.into_iter().enumerate() {
            self.tag(code + offset as i32 * 10, v);
        }
    }
    fn record(&mut self, kind: &str, id: u64, owner: u64) {
        self.tag(0, kind);
        self.tag(5, format_args!("{id:X}"));
        self.tag(330, format_args!("{owner:X}"));
    }
    fn entity(&mut self, kind: &str, id: u64, owner: u64, paper: bool, layer: &str, paint: &Paint) {
        self.record(kind, id, owner);
        self.tag(100, "AcDbEntity");
        if paper {
            self.tag(67, 1);
        }
        self.tag(8, layer);
        if let Some(v) = paint.color {
            self.tag(62, v);
        }
        if let Some(v) = paint.true_color {
            self.tag(420, v);
        }
        if let Some(v) = paint.lineweight {
            self.tag(370, v);
        }
    }
}
fn finite(values: &[f64]) -> Result<(), String> {
    if values.iter().any(|v| !v.is_finite()) {
        Err("Niet-eindige sjabloongeometrie".into())
    } else {
        Ok(())
    }
}
fn text(value: &str) -> Result<(), String> {
    if value.len() > 1024 * 1024 || value.chars().any(char::is_control) {
        Err("Ongeldige sjabloontekst".into())
    } else {
        Ok(())
    }
}
fn paint(p: &Paint) -> Result<(), String> {
    if p.color.is_some_and(|v| v > 256)
        || p.true_color.is_some_and(|v| v > 0xffffff)
        || p.lineweight.is_some_and(|v| {
            ![
                -3, -2, -1, 0, 5, 9, 13, 15, 18, 20, 25, 30, 35, 40, 50, 53, 60, 70, 80, 90, 100,
                106, 120, 140, 158, 200, 211,
            ]
            .contains(&v)
        })
    {
        Err("Ongeldige sjabloonkleur of lijndikte".into())
    } else {
        Ok(())
    }
}
fn number(tags: &HashMap<i32, &str>, code: i32, default: f64) -> Result<f64, String> {
    let n = tags.get(&code).map_or(Ok(default), |v| {
        v.trim()
            .parse::<f64>()
            .map_err(|_| "Ongeldig numeriek blokattribuut".to_string())
    })?;
    finite(&[n])?;
    Ok(n)
}
fn scoped<'a>(
    pairs: impl Iterator<Item = (i32, &'a str)>,
    subclass: &str,
) -> HashMap<i32, &'a str> {
    let mut active = false;
    let mut tags = HashMap::new();
    for (code, value) in pairs {
        if code == 100 {
            active = value == subclass;
            continue;
        }
        if code == 101 || code >= 1000 {
            break;
        }
        if active {
            tags.insert(code, value);
        }
    }
    tags
}
struct Attribute<'a> {
    entity: HashMap<i32, &'a str>,
    text: HashMap<i32, &'a str>,
    fields: HashMap<i32, &'a str>,
}
impl<'a> Attribute<'a> {
    fn parse(r: RecordView<'a>) -> Result<Self, String> {
        let tags = r.tags().collect::<Vec<_>>();
        let mut attribute = Self::parse_tags(tags.iter().copied())?;
        attribute.text.insert(1, r.current_text()?);
        Ok(attribute)
    }
    fn parse_tags(tags: impl Iterator<Item = (i32, &'a str)> + Clone) -> Result<Self, String> {
        if tags.clone().any(|(c, _)| c == 101) {
            return Err("Multiline blokattributen zijn nog niet ondersteund".into());
        }
        let attribute = Self {
            entity: scoped(tags.clone(), "AcDbEntity"),
            text: scoped(tags.clone(), "AcDbText"),
            fields: scoped(tags, "AcDbAttributeDefinition"),
        };
        if number(&attribute.fields, 71, 1.)? != 1. {
            return Err("Multiline blokattributen zijn nog niet ondersteund".into());
        }
        Ok(attribute)
    }
}
struct Block<'a> {
    base: [f64; 3],
    attributes: Vec<Result<Attribute<'a>, String>>,
}
struct Space {
    owner: u64,
    at: usize,
    paper: bool,
}
/// Read a scaled attribute once when preparing a set of profile sheets.
pub fn block_attribute_metrics(
    document: &Document,
    name: &str,
    tag: &str,
    scale: f64,
    cancel: &AtomicBool,
) -> Result<Option<AttributeMetrics>, String> {
    crate::input::check(cancel)?;
    finite(&[scale])?;
    if scale <= 0. {
        return Err("Blokschaal moet positief zijn".into());
    }
    let name = casefold(name);
    let tag = crate::python_values::uppercase(crate::python_values::trim(tag));
    if let Some(block) = document.added_blocks.get(&name) {
        for tags in &block.attributes {
            crate::input::check(cancel)?;
            if tags.iter().any(|(c, v)| {
                *c == 2 && crate::python_values::uppercase(crate::python_values::trim(v)) == tag
            }) {
                let attr = Attribute::parse_tags(tags.iter().map(|(c, v)| (*c, v.as_str())))?;
                return attribute_metrics(&attr, [block.base[0], block.base[1]], scale).map(Some);
            }
        }
        return Ok(None);
    }
    let mut active = false;
    let mut found = false;
    let mut base = [0.; 2];
    for r in document.records().filter(|r| r.section() == Some("BLOCKS")) {
        if r.index().is_multiple_of(1024) {
            crate::input::check(cancel)?;
        }
        if r.kind() == "BLOCK" {
            active = r.first(2).is_some_and(|v| casefold(v) == name);
            if active {
                found = true;
                for (i, code) in [10, 20].into_iter().enumerate() {
                    base[i] = r
                        .first(code)
                        .unwrap_or("0")
                        .trim()
                        .parse()
                        .map_err(|_| "Ongeldige blokbasis")?;
                }
                finite(&base)?;
            }
        } else if r.kind() == "ENDBLK" {
            active = false;
        } else if active
            && r.kind() == "ATTDEF"
            && r.first(2).is_some_and(|v| {
                crate::python_values::uppercase(crate::python_values::trim(v)) == tag
            })
        {
            let attr = Attribute::parse(r)?;
            return attribute_metrics(&attr, base, scale).map(Some);
        }
    }
    crate::input::check(cancel)?;
    if found {
        Ok(None)
    } else {
        Err("Sjabloonblok ontbreekt".into())
    }
}
fn attribute_metrics(
    attr: &Attribute<'_>,
    base: XY,
    scale: f64,
) -> Result<AttributeMetrics, String> {
    if [
        number(&attr.text, 210, 0.)?,
        number(&attr.text, 220, 0.)?,
        number(&attr.text, 230, 1.)?,
    ] != [0., 0., 1.]
    {
        return Err("Gekantelde blokattributen zijn nog niet ondersteund".into());
    }
    let offset = [
        (number(&attr.text, 10, 0.)? - base[0]) * scale,
        (number(&attr.text, 20, 0.)? - base[1]) * scale,
    ];
    let height = number(&attr.text, 40, 2.5)? * scale;
    finite(&[offset[0], offset[1], height])?;
    if height <= 0. {
        return Err("Ongeldige attribuuthoogte".into());
    }
    Ok(AttributeMetrics { offset, height })
}
struct BlockPlacement<'a> {
    insert: XY,
    scale: f64,
    values: &'a BTreeMap<String, String>,
    attributes: &'a BTreeMap<String, TextPlacement>,
}
fn insertion_spaces(
    document: &Document,
    cancel: &AtomicBool,
) -> Result<HashMap<String, Space>, String> {
    let mut names = HashMap::new();
    for r in document.records() {
        if r.index().is_multiple_of(1024) {
            crate::input::check(cancel)?;
        }
        if r.kind() == "LAYOUT" {
            if let (Some(id), Some((_, name))) =
                (r.handle(), r.tags().filter(|(c, _)| *c == 1).last())
            {
                names.insert(id, casefold(name));
            }
        }
    }
    let mut block_ends = HashMap::new();
    let mut current = None;
    for r in document.records().filter(|r| r.section() == Some("BLOCKS")) {
        if r.index().is_multiple_of(1024) {
            crate::input::check(cancel)?;
        }
        if r.kind() == "BLOCK" {
            current = r.first(2).map(casefold);
        } else if r.kind() == "ENDBLK" {
            if let Some(name) = current.take() {
                block_ends.insert(name, document.record_start(r.index()));
            }
        }
    }
    let mut result = HashMap::new();
    let entities = document.section_end("ENTITIES")?;
    for r in document.records().filter(|r| r.kind() == "BLOCK_RECORD") {
        let Some(owner) = r.handle() else { continue };
        let name = r.first(2).unwrap_or("");
        let key = if name.eq_ignore_ascii_case("*Model_Space") {
            Some("model".to_owned())
        } else {
            r.first(340)
                .and_then(|v| u64::from_str_radix(v.trim(), 16).ok())
                .and_then(|id| names.get(&id).cloned())
        };
        let Some(key) = key else { continue };
        let at = if key == "model" || name.eq_ignore_ascii_case("*Paper_Space") {
            entities
        } else {
            *block_ends
                .get(&casefold(name))
                .ok_or("Papierlayout mist BLOCK-einde")?
        };
        let paper = key != "model";
        if result.insert(key, Space { owner, at, paper }).is_some() {
            return Err("Dubbele sjabloonlayout".into());
        }
    }
    Ok(result)
}
fn attribute(
    out: &mut Tags,
    attribute: &Attribute<'_>,
    id: u64,
    space: &Space,
    block: &Block<'_>,
    placement: &BlockPlacement<'_>,
) -> Result<(), String> {
    let (insert, scale) = (placement.insert, placement.scale);
    let entity = &attribute.entity;
    let t = &attribute.text;
    let a = &attribute.fields;
    let tag = *a.get(&2).ok_or("Blokattribuut mist tag")?;
    let value = placement
        .values
        .get(tag)
        .map_or_else(|| t.get(&1).copied().unwrap_or(""), String::as_str);
    text(tag)?;
    text(value)?;
    if [
        number(t, 210, 0.)?,
        number(t, 220, 0.)?,
        number(t, 230, 1.)?,
    ] != [0., 0., 1.]
    {
        return Err("Gekantelde blokattributen zijn nog niet ondersteund".into());
    }
    let transform = |code: i32, fallback: [f64; 3]| -> Result<[f64; 3], String> {
        let p = [
            number(t, code, fallback[0])?,
            number(t, code + 10, fallback[1])?,
            number(t, code + 20, fallback[2])?,
        ];
        let result = [
            insert[0] + (p[0] - block.base[0]) * scale,
            insert[1] + (p[1] - block.base[1]) * scale,
            p[2] - block.base[2],
        ];
        finite(&result)?;
        Ok(result)
    };
    let mut p = transform(10, [0.; 3])?;
    let raw = [number(t, 10, 0.)?, number(t, 20, 0.)?, number(t, 30, 0.)?];
    let mut align = transform(11, raw)?;
    let mut height = number(t, 40, 2.5)? * scale;
    let changed = if placement.attributes.is_empty() {
        None
    } else {
        placement.attributes.get(&crate::python_values::uppercase(
            crate::python_values::trim(tag),
        ))
    };
    if let Some(value) = changed {
        p = [value.insert[0], value.insert[1], 0.];
        align = p;
        height = value.height;
    }
    finite(&[height])?;
    if height <= 0. {
        return Err("Ongeldige attribuuthoogte".into());
    }
    out.record("ATTRIB", id, space.owner);
    out.tag(100, "AcDbEntity");
    out.tag(67, u8::from(space.paper));
    out.tag(8, entity.get(&8).copied().unwrap_or("0"));
    for code in [6, 62, 48, 60, 370, 420, 430, 440, 347, 390, 284] {
        if let Some(v) = entity.get(&code) {
            out.tag(code, v);
        }
    }
    out.tag(100, "AcDbText");
    out.point(10, p);
    out.tag(40, height);
    out.tag(1, value);
    for (code, default) in [(39, 0.), (50, 0.), (41, 1.), (51, 0.), (71, 0.), (72, 0.)] {
        if t.contains_key(&code) || matches!(code, 50 | 41) {
            let value = match (code, changed) {
                (50, Some(p)) => p.rotation,
                _ => number(t, code, default)?,
            };
            let value = if code == 50 && changed.is_none() {
                let radians = value.to_radians();
                radians.sin().atan2(radians.cos()).to_degrees()
            } else {
                value
            };
            out.tag(code, value);
        }
    }
    out.tag(7, t.get(&7).copied().unwrap_or("Standard"));
    out.point(11, align);
    out.point(210, [0., 0., 1.]);
    out.tag(100, "AcDbAttribute");
    out.tag(2, tag);
    out.tag(70, a.get(&70).copied().unwrap_or("0"));
    for code in [73, 74, 280, 71] {
        if let Some(v) = a.get(&code) {
            out.tag(code, v);
        }
    }
    Ok(())
}

/// Returns primary entity handles in request order. New resource declarations
/// preserve source spellings/properties when a casefold-equal layer exists.
pub fn add(
    document: &mut Document,
    layers: &[Layer],
    entities: &[Entity],
    cancel: &AtomicBool,
) -> Result<Vec<String>, String> {
    crate::input::check(cancel)?;
    if layers.is_empty() && entities.is_empty() {
        return Ok(Vec::new());
    }
    if entities.len() > 100_000 || layers.len() > 10_000 {
        return Err("Te veel sjabloonentiteiten of lagen".into());
    }
    let mut known: HashSet<_> = document
        .records()
        .filter(|r| r.kind() == "LAYER")
        .filter_map(|r| r.first(2))
        .map(casefold)
        .chain(document.added_layers.iter().cloned())
        .collect();
    let mut new_layers = BTreeMap::new();
    for layer in layers {
        crate::object_dxf::layer_name(&layer.name)?;
        paint(&Paint {
            color: Some(layer.color),
            true_color: layer.true_color,
            lineweight: layer.lineweight,
        })?;
        if !(1..=255).contains(&layer.color) {
            return Err("Laagkleur moet tussen 1 en 255 liggen".into());
        }
        let key = casefold(&layer.name);
        if !known.contains(&key) && new_layers.insert(key, layer).is_some() {
            return Err("Dubbele nieuwe sjabloonlaag".into());
        }
    }
    known.extend(new_layers.keys().cloned());
    let styles: HashSet<_> = document
        .records()
        .filter(|r| r.kind() == "STYLE")
        .filter_map(|r| r.first(2))
        .map(casefold)
        .collect();
    let spaces = insertion_spaces(document, cancel)?;
    let wanted: HashSet<_> = entities
        .iter()
        .filter_map(|e| {
            if let Shape::Block { name, .. } = &e.shape {
                Some(casefold(name))
            } else {
                None
            }
        })
        .collect();
    let mut blocks: HashMap<String, Block<'_>> = HashMap::new();
    let mut current = None;
    for r in document.records().filter(|r| r.section() == Some("BLOCKS")) {
        if r.index().is_multiple_of(1024) {
            crate::input::check(cancel)?;
        }
        if r.kind() == "BLOCK" {
            current = r.first(2).map(casefold).filter(|key| wanted.contains(key));
            if let Some(key) = &current {
                let mut base = [0.; 3];
                for (i, code) in [10, 20, 30].into_iter().enumerate() {
                    base[i] = r
                        .first(code)
                        .unwrap_or("0")
                        .trim()
                        .parse()
                        .map_err(|_| "Ongeldige blokbasis")?;
                }
                finite(&base)?;
                blocks.insert(
                    key.clone(),
                    Block {
                        base,
                        attributes: Vec::new(),
                    },
                );
            }
        } else if r.kind() == "ENDBLK" {
            current = None;
        } else if r.kind() == "ATTDEF" {
            if let Some(key) = &current {
                let attrs = &mut blocks.get_mut(key).unwrap().attributes;
                if attrs.len() >= 10_000 {
                    return Err("Te veel blokattributen".into());
                }
                attrs.push(Attribute::parse(r));
            }
        }
    }
    for (name, definition) in &document.added_blocks {
        if wanted.contains(name) {
            blocks.insert(
                name.clone(),
                Block {
                    base: definition.base,
                    attributes: definition
                        .attributes
                        .iter()
                        .map(|tags| {
                            Attribute::parse_tags(tags.iter().map(|(c, v)| (*c, v.as_str())))
                        })
                        .collect(),
                },
            );
        }
    }
    let (mut next, mut seed) = document.seed()?;
    let mut allocate = || -> Result<u64, String> {
        let id = next;
        next = next.checked_add(1).ok_or("Geen vrije DXF-handles")?;
        Ok(id)
    };
    let mut output: BTreeMap<usize, String> = BTreeMap::new();
    let mut bytes = 0usize;
    let mut points = 0usize;
    let mut handles = Vec::with_capacity(entities.len());
    for entity in entities {
        crate::input::check(cancel)?;
        paint(&entity.paint)?;
        crate::object_dxf::layer_name(&entity.layer)?;
        if !known.contains(&casefold(&entity.layer)) {
            return Err(format!("Sjabloonlaag ontbreekt: {}", entity.layer));
        }
        let key = entity
            .layout
            .as_deref()
            .map_or_else(|| "model".into(), casefold);
        let space = spaces.get(&key).ok_or("Sjabloonlayout ontbreekt")?;
        let id = allocate()?;
        let mut out = Tags::default();
        match &entity.shape {
            Shape::Polyline { vertices, closed } => {
                points = points
                    .checked_add(vertices.len())
                    .filter(|n| *n <= 1_000_000)
                    .ok_or("Te veel sjabloonvertices")?;
                if vertices.len() < 2 {
                    return Err("Sjabloonpolyline vereist twee vertices".into());
                }
                out.entity(
                    "LWPOLYLINE",
                    id,
                    space.owner,
                    space.paper,
                    &entity.layer,
                    &entity.paint,
                );
                out.tag(100, "AcDbPolyline");
                out.tag(90, vertices.len());
                out.tag(70, u8::from(*closed));
                for (i, p) in vertices.iter().enumerate() {
                    if i.is_multiple_of(1024) {
                        crate::input::check(cancel)?;
                    }
                    finite(p)?;
                    out.tag(10, p[0]);
                    out.tag(20, p[1]);
                }
            }
            Shape::Circle { center, radius } => {
                finite(center)?;
                finite(&[*radius])?;
                if *radius <= 0. {
                    return Err("Ongeldige cirkelstraal".into());
                }
                out.entity(
                    "CIRCLE",
                    id,
                    space.owner,
                    space.paper,
                    &entity.layer,
                    &entity.paint,
                );
                out.tag(100, "AcDbCircle");
                out.point(10, [center[0], center[1], 0.]);
                out.tag(40, radius);
            }
            Shape::Text {
                text: content,
                insert,
                height,
                rotation,
                style,
                halign,
                valign,
            } => {
                text(content)?;
                text(style)?;
                finite(insert)?;
                finite(&[*height, *rotation])?;
                if *height <= 0.
                    || !matches!(*halign, 0 | 1 | 2 | 4)
                    || *valign > 3
                    || !styles.contains(&casefold(style))
                {
                    return Err("Ongeldige teksthoogte, uitlijning of ontbrekende stijl".into());
                }
                out.entity(
                    "TEXT",
                    id,
                    space.owner,
                    space.paper,
                    &entity.layer,
                    &entity.paint,
                );
                out.tag(100, "AcDbText");
                out.point(10, [insert[0], insert[1], 0.]);
                out.tag(40, height);
                out.tag(1, content);
                out.tag(50, rotation);
                out.tag(7, style);
                out.tag(72, halign);
                out.point(11, [insert[0], insert[1], 0.]);
                out.tag(100, "AcDbText");
                out.tag(73, valign);
            }
            Shape::Block {
                name,
                insert,
                scale,
                attributes,
                attribute_placements,
            } => {
                text(name)?;
                finite(insert)?;
                finite(&[*scale])?;
                if *scale <= 0. {
                    return Err("Blokschaal moet positief zijn".into());
                }
                for (k, v) in attributes {
                    text(k)?;
                    text(v)?;
                }
                let mut placements = BTreeMap::new();
                for (tag, p) in attribute_placements {
                    text(tag)?;
                    finite(&[p.insert[0], p.insert[1], p.height, p.rotation])?;
                    if p.height <= 0. {
                        return Err("Ongeldige attribuuthoogte".into());
                    }
                    let key = crate::python_values::uppercase(crate::python_values::trim(tag));
                    if placements.insert(key, *p).is_some() {
                        return Err("Dubbele attribuutplaatsing".into());
                    }
                }
                let block = blocks
                    .get(&casefold(name))
                    .ok_or("Sjabloonblok ontbreekt")?;
                let auto = !attributes.is_empty() && !block.attributes.is_empty();
                out.entity(
                    "INSERT",
                    id,
                    space.owner,
                    space.paper,
                    &entity.layer,
                    &entity.paint,
                );
                out.tag(100, "AcDbBlockReference");
                if auto {
                    out.tag(66, 1);
                }
                out.tag(2, name);
                out.point(10, [insert[0], insert[1], 0.]);
                out.tag(41, scale);
                out.tag(42, scale);
                out.tag(50, 0);
                if auto {
                    let seqend = allocate()?;
                    for r in &block.attributes {
                        crate::input::check(cancel)?;
                        attribute(
                            &mut out,
                            r.as_ref().map_err(Clone::clone)?,
                            allocate()?,
                            space,
                            block,
                            &BlockPlacement {
                                insert: *insert,
                                scale: *scale,
                                values: attributes,
                                attributes: &placements,
                            },
                        )?;
                        if bytes + out.0.len() > 128 * 1024 * 1024 {
                            return Err("Te veel sjabloongeometrie".into());
                        }
                    }
                    out.record("SEQEND", seqend, id);
                    out.tag(100, "AcDbEntity");
                    out.tag(67, u8::from(space.paper));
                    out.tag(8, &entity.layer);
                }
            }
        }
        bytes = bytes
            .checked_add(out.0.len())
            .filter(|n| *n <= 128 * 1024 * 1024)
            .ok_or("Te veel sjabloongeometrie")?;
        output.entry(space.at).or_default().push_str(&out.0);
        handles.push(format!("{id:X}"));
    }
    let mut changes = Vec::new();
    if !new_layers.is_empty() {
        let mut table = None;
        let mut end = None;
        for r in document.records() {
            if r.kind() == "TABLE" && r.first(2) == Some("LAYER") {
                table = Some(r);
            } else if table.is_some() && r.kind() == "ENDTAB" {
                end = Some(r);
                break;
            }
        }
        let table = table.ok_or("DXF mist laagtabel")?;
        let at = document.record_start(end.ok_or("DXF mist laagtabeleinde")?.index());
        let owner = table.handle().ok_or("Laagtabel mist handle")?;
        let mut tags = Tags::default();
        for layer in new_layers.values() {
            tags.record("LAYER", allocate()?, owner);
            tags.tag(100, "AcDbSymbolTableRecord");
            tags.tag(100, "AcDbLayerTableRecord");
            tags.tag(2, &layer.name);
            tags.tag(70, 0);
            tags.tag(62, layer.color);
            tags.tag(6, "Continuous");
            tags.tag(290, 1);
            if let Some(lineweight) = layer.lineweight {
                tags.tag(370, lineweight);
            }
            if let Some(color) = layer.true_color {
                tags.tag(420, color);
            }
        }
        changes.push(Change {
            start: at,
            end: at,
            text: tags.0,
        });
        changes.push(document.tag_change(
            table.index(),
            70,
            &(document.layer_count() + new_layers.len()).to_string(),
        )?);
    }
    for (at, text) in output {
        changes.push(Change {
            start: at,
            end: at,
            text,
        });
    }
    seed.text = format!("{next:X}");
    changes.push(seed);
    crate::input::check(cancel)?;
    // Release source borrows before committing staged edits and resource names.
    drop(blocks);
    let added: Vec<_> = new_layers.into_keys().collect();
    document.apply(changes)?;
    document.added_layers.extend(added);
    Ok(handles)
}
