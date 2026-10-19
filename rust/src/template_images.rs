//! Native template raster insertion, including ownership, reactors and classes.
//! Collect all page/logo/map images, then append one batch without reparsing DXF.
use crate::{
    dxf_document::{Change, Document, RecordView},
    model::Bounds,
};
use std::{
    collections::{HashMap, HashSet},
    fmt::{Display, Write},
    path::PathBuf,
    sync::atomic::AtomicBool,
};
const LAYER: &str = "PROEFSLEUF_TEMPLATE_IMAGES";
pub struct Image {
    pub path: PathBuf,
    pub bounds: Bounds,
    pub definition_name: String,
    pub inset: f64,
    pub preserve_aspect: bool,
    /// None is modelspace; a named layout supports paper-space logos.
    pub layout: Option<String>,
}
#[derive(Default)]
struct Tags(String);
impl Tags {
    fn tag(&mut self, code: i32, value: impl Display) {
        writeln!(self.0, "{code}\n{value}").unwrap();
    }
    fn id(&mut self, code: i32, id: u64) {
        self.tag(code, format_args!("{id:X}"));
    }
    fn record(&mut self, kind: &str, id: u64, owner: u64) {
        self.tag(0, kind);
        self.id(5, id);
        self.id(330, owner);
    }
}
fn dict_entries(r: RecordView<'_>) -> HashMap<String, u64> {
    let mut result = HashMap::new();
    let mut key = None;
    for (c, v) in r.tags() {
        if c == 3 {
            key = Some(v.to_owned());
        } else if matches!(c, 350 | 360) {
            if let Some(k) = key.take() {
                if let Ok(id) = u64::from_str_radix(v.trim(), 16) {
                    result.insert(k, id);
                }
            }
        }
    }
    result
}
pub fn fit(
    box_bounds: Bounds,
    pixels: [u32; 2],
    inset: f64,
    preserve_aspect: bool,
) -> Result<Bounds, String> {
    if [
        box_bounds.min_x,
        box_bounds.min_y,
        box_bounds.max_x,
        box_bounds.max_y,
        inset,
        box_bounds.max_x - box_bounds.min_x,
        box_bounds.max_y - box_bounds.min_y,
    ]
    .iter()
    .any(|v| !v.is_finite())
        || pixels.contains(&0)
        || box_bounds.max_x <= box_bounds.min_x
        || box_bounds.max_y <= box_bounds.min_y
    {
        return Err("Ongeldige rasterafmetingen of sjabloonbegrenzing".into());
    }
    let padded = Bounds {
        min_x: box_bounds.min_x + inset,
        min_y: box_bounds.min_y + inset,
        max_x: box_bounds.max_x - inset,
        max_y: box_bounds.max_y - inset,
    };
    let result = if !preserve_aspect {
        if inset > 0. {
            padded
        } else {
            box_bounds
        }
    } else if padded.max_x <= padded.min_x || padded.max_y <= padded.min_y {
        box_bounds
    } else {
        let aspect = pixels[0] as f64 / pixels[1] as f64;
        let width = padded.max_x - padded.min_x;
        let height = padded.max_y - padded.min_y;
        let (width, height) = if aspect >= width / height.max(1e-9) {
            (width, width / aspect.max(1e-9))
        } else {
            (height * aspect, height)
        };
        let x = (padded.min_x + padded.max_x) * 0.5;
        let y = (padded.min_y + padded.max_y) * 0.5;
        Bounds {
            min_x: x - width * 0.5,
            min_y: y - height * 0.5,
            max_x: x + width * 0.5,
            max_y: y + height * 0.5,
        }
    };
    if [result.min_x, result.min_y, result.max_x, result.max_y]
        .iter()
        .any(|v| !v.is_finite())
        || result.max_x <= result.min_x
        || result.max_y <= result.min_y
    {
        return Err("Raster past niet in het sjabloonvak".into());
    }
    Ok(result)
}
pub fn add_images(
    document: &mut Document,
    images: &[Image],
    cancel: &AtomicBool,
) -> Result<(), String> {
    crate::input::check(cancel)?;
    if images.is_empty() {
        return Ok(());
    }
    if images.len() > 10_000 {
        return Err("Te veel sjabloonrasters".into());
    }
    if document.image_batch_added {
        return Err("Verzamel alle sjabloonrasters in Ã©Ã©n batch".into());
    }
    // Read/validate all PNG dimensions before changing any source tags.
    let mut prepared = Vec::with_capacity(images.len());
    let mut estimated = 0usize;
    for image in images {
        crate::input::check(cancel)?;
        let path = image
            .path
            .to_str()
            .ok_or("Rasterpad is geen geldige Unicode")?;
        if !image.path.is_absolute() {
            return Err("Sjabloonraster vereist een absoluut pad".into());
        }
        estimated = estimated
            .checked_add(path.len() + image.definition_name.len() + 2048)
            .ok_or("Te veel rastermetadata")?;
        if estimated > 128 * 1024 * 1024 {
            return Err("Te veel rastermetadata".into());
        }
        if path.len() > 1024 * 1024
            || path.chars().any(char::is_control)
            || image.definition_name.len() > 1024 * 1024
            || image.definition_name.chars().any(char::is_control)
            || image.definition_name.is_empty()
        {
            return Err("Ongeldig rasterpad of afbeeldingsnaam".into());
        }
        let (w, h) = image::image_dimensions(&image.path)
            .map_err(|e| format!("Sjabloonraster niet leesbaar: {e}"))?;
        prepared.push((
            [w, h],
            fit(image.bounds, [w, h], image.inset, image.preserve_aspect)?,
        ));
    }
    let (mut next, mut seed) = document.seed()?;
    let mut allocate = || -> Result<u64, String> {
        let id = next;
        next = next.checked_add(1).ok_or("Geen vrije DXF-handles")?;
        Ok(id)
    };
    let model = document
        .records()
        .find(|r| {
            r.kind() == "BLOCK_RECORD"
                && r.first(2)
                    .is_some_and(|v| v.eq_ignore_ascii_case("*Model_Space"))
        })
        .and_then(RecordView::handle)
        .ok_or("DXF mist modelspace")?;
    let root = document
        .records()
        .find(|r| {
            r.section() == Some("OBJECTS")
                && r.kind() == "DICTIONARY"
                && r.first(330).is_some_and(|v| v.trim() == "0")
        })
        .ok_or("DXF mist hoofddictionary")?;
    let root_id = root.handle().ok_or("Hoofddictionary mist handle")?;
    let root_index = root.index();
    let root_entries = dict_entries(root);
    let root_pointer = if root.first(280) == Some("1") {
        360
    } else {
        350
    };
    let mut changes = Vec::new();
    let mut classes = Tags::default();
    let mut objects = Tags::default();
    let mut entity_sections: HashMap<usize, String> = HashMap::new();
    let mut root_tags = Tags::default();
    for (kind, cpp, flags, entity) in [
        ("IMAGE", "AcDbRasterImage", 2175, 1),
        ("IMAGEDEF", "AcDbRasterImageDef", 0, 0),
        ("IMAGEDEF_REACTOR", "AcDbRasterImageDefReactor", 1, 0),
        ("RASTERVARIABLES", "AcDbRasterVariables", 0, 0),
    ] {
        let source_count = document.records().filter(|r| r.kind() == kind).count();
        let count = source_count
            + if kind == "RASTERVARIABLES" {
                usize::from(!root_entries.contains_key("ACAD_IMAGE_VARS"))
            } else {
                images.len()
            };
        if let Some(class) = document
            .records()
            .find(|r| r.kind() == "CLASS" && r.first(1) == Some(kind))
        {
            if class.first(91).is_some() {
                changes.push(document.tag_change(class.index(), 91, &count.to_string())?);
            }
        } else {
            classes.tag(0, "CLASS");
            classes.tag(1, kind);
            classes.tag(2, cpp);
            classes.tag(3, "ISM");
            classes.tag(90, flags);
            classes.tag(91, count);
            classes.tag(280, 0);
            classes.tag(281, entity);
        }
    }
    if !classes.0.is_empty() {
        let at = document.section_end("CLASSES")?;
        changes.push(Change {
            start: at,
            end: at,
            text: classes.0,
        });
    }
    let add_layer = !document.has_layer(LAYER);
    let mut added_layer = None;
    if add_layer {
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
        let end = end.ok_or("DXF-laagtabel is niet afgesloten")?;
        let mut layer = Tags::default();
        let layer_id = allocate()?;
        layer.record(
            "LAYER",
            layer_id,
            table.handle().ok_or("Laagtabel mist handle")?,
        );
        layer.tag(100, "AcDbSymbolTableRecord");
        layer.tag(100, "AcDbLayerTableRecord");
        layer.tag(2, LAYER);
        layer.tag(70, 0);
        layer.tag(62, 7);
        layer.tag(6, "Continuous");
        layer.tag(290, 1);
        let count = document.layer_count() + 1;
        changes.push(document.tag_change(table.index(), 70, &count.to_string())?);
        let at = document.record_start(end.index());
        added_layer = Some(crate::dxf_document::AddedSymbol {
            handle: layer_id,
            at,
        });
        changes.push(Change {
            start: at,
            end: at,
            text: layer.0,
        });
    }
    if !root_entries.contains_key("ACAD_IMAGE_VARS") {
        let id = allocate()?;
        root_tags.tag(3, "ACAD_IMAGE_VARS");
        root_tags.id(root_pointer, id);
        objects.tag(0, "RASTERVARIABLES");
        objects.id(5, id);
        objects.tag(102, "{ACAD_REACTORS");
        objects.id(330, root_id);
        objects.tag(102, "}");
        objects.id(330, root_id);
        objects.tag(100, "AcDbRasterVariables");
        objects.tag(90, 0);
        objects.tag(70, 0);
        objects.tag(71, 1);
        objects.tag(72, 3);
    }
    let existing_dict = root_entries
        .get("ACAD_IMAGE_DICT")
        .and_then(|id| document.by_handle(&format!("{id:X}")).ok().flatten());
    if root_entries.contains_key("ACAD_IMAGE_DICT") && existing_dict.is_none() {
        return Err("Afbeeldingsdictionary ontbreekt".into());
    }
    if existing_dict.is_some_and(|r| r.kind() != "DICTIONARY") {
        return Err("Afbeeldingsdictionary heeft een onjuist recordtype".into());
    }
    let dictionary = if let Some(r) = existing_dict {
        r.handle().unwrap()
    } else {
        allocate()?
    };
    let mut names: HashSet<_> = existing_dict
        .map(dict_entries)
        .unwrap_or_default()
        .into_keys()
        .collect();
    let mut entries = Tags::default();
    for (image, (pixels, b)) in images.iter().zip(prepared) {
        let mut entities = Tags::default();
        crate::input::check(cancel)?;
        let definition = allocate()?;
        let entity = allocate()?;
        let reactor = allocate()?;
        let mut name = image.definition_name.clone();
        let mut suffix = 2;
        while names.contains(&name) {
            name = format!("{} ({suffix})", image.definition_name);
            suffix += 1;
        }
        if name.len() > 1024 * 1024 {
            return Err("Afbeeldingsnaam is te lang".into());
        }
        names.insert(name.clone());
        entries.tag(3, name);
        entries.id(
            if existing_dict.is_some_and(|r| r.first(280) == Some("1")) {
                360
            } else {
                350
            },
            definition,
        );
        objects.tag(0, "IMAGEDEF");
        objects.id(5, definition);
        objects.tag(102, "{ACAD_REACTORS");
        objects.id(330, dictionary);
        objects.id(330, reactor);
        objects.tag(102, "}");
        objects.id(330, dictionary);
        objects.tag(100, "AcDbRasterImageDef");
        objects.tag(90, 0);
        objects.tag(1, image.path.to_str().unwrap());
        objects.tag(10, pixels[0]);
        objects.tag(20, pixels[1]);
        objects.tag(11, 0.01);
        objects.tag(21, 0.01);
        objects.tag(280, 1);
        objects.tag(281, 0);
        objects.record("IMAGEDEF_REACTOR", reactor, entity);
        objects.tag(100, "AcDbRasterImageDefReactor");
        objects.tag(90, 2);
        objects.id(330, entity);
        let space = if let Some(name) = &image.layout {
            let wanted = crate::python_values::uppercase(name);
            let layout = document
                .records()
                .find(|r| {
                    r.kind() == "LAYOUT"
                        && r.tags()
                            .filter(|(c, _)| *c == 1)
                            .last()
                            .is_some_and(|(_, n)| crate::python_values::uppercase(n) == wanted)
                })
                .and_then(RecordView::handle)
                .ok_or_else(|| format!("Sjabloonlayout ontbreekt: {name}"))?;
            document
                .records()
                .find(|r| {
                    r.kind() == "BLOCK_RECORD"
                        && r.first(340)
                            .and_then(|v| u64::from_str_radix(v.trim(), 16).ok())
                            == Some(layout)
                })
                .and_then(RecordView::handle)
                .ok_or("Layout mist een blokrecord")?
        } else {
            model
        };
        let block_record = document
            .by_handle(&format!("{space:X}"))?
            .ok_or("Afbeeldingsruimte mist een blokrecord")?;
        let block_name = block_record
            .first(2)
            .ok_or("Afbeeldingsruimte mist een bloknaam")?;
        let at = if space == model || block_name.eq_ignore_ascii_case("*Paper_Space") {
            document.section_end("ENTITIES")?
        } else {
            // Only the active paper layout belongs in ENTITIES. Inactive
            // layouts serialize their entities inside their BLOCK definition.
            let mut found = false;
            let mut end = None;
            for record in document.records() {
                if record.kind() == "BLOCK" && record.first(2) == Some(block_name) {
                    found = true;
                } else if found && record.kind() == "ENDBLK" {
                    end = Some(document.record_start(record.index()));
                    break;
                }
            }
            end.ok_or("Papierlayout mist een afgesloten blokdefinitie")?
        };
        entities.record("IMAGE", entity, space);
        entities.tag(100, "AcDbEntity");
        if space != model {
            entities.tag(67, 1);
        }
        entities.tag(8, LAYER);
        entities.tag(100, "AcDbRasterImage");
        entities.tag(90, 0);
        for (c, v) in [
            (10, b.min_x),
            (20, b.min_y),
            (30, 0.),
            (11, (b.max_x - b.min_x) / pixels[0] as f64),
            (21, 0.),
            (31, 0.),
            (12, 0.),
            (22, (b.max_y - b.min_y) / pixels[1] as f64),
            (32, 0.),
        ] {
            entities.tag(c, v);
        }
        entities.tag(13, pixels[0]);
        entities.tag(23, pixels[1]);
        entities.id(340, definition);
        entities.tag(70, 11);
        entities.tag(280, 0);
        entities.tag(281, 50);
        entities.tag(282, 50);
        entities.tag(283, 0);
        entities.id(360, reactor);
        entities.tag(71, 1);
        entities.tag(91, 2);
        entities.tag(14, -0.5);
        entities.tag(24, -0.5);
        entities.tag(14, pixels[0] as f64 - 0.5);
        entities.tag(24, pixels[1] as f64 - 0.5);
        entities.tag(290, 0);
        entity_sections.entry(at).or_default().push_str(&entities.0);
    }
    if let Some(r) = existing_dict {
        let at = document.before_xdata(r.index());
        changes.push(Change {
            start: at,
            end: at,
            text: entries.0,
        });
    } else {
        root_tags.tag(3, "ACAD_IMAGE_DICT");
        root_tags.id(root_pointer, dictionary);
        objects.record("DICTIONARY", dictionary, root_id);
        objects.tag(100, "AcDbDictionary");
        objects.tag(280, 0);
        objects.tag(281, 1);
        objects.0.push_str(&entries.0);
    }
    if !root_tags.0.is_empty() {
        let at = document.before_xdata(root_index);
        changes.push(Change {
            start: at,
            end: at,
            text: root_tags.0,
        });
    }
    for (at, text) in entity_sections {
        changes.push(Change {
            start: at,
            end: at,
            text,
        });
    }
    let at = document.section_end("OBJECTS")?;
    changes.push(Change {
        start: at,
        end: at,
        text: objects.0,
    });
    seed.text = format!("{next:X}");
    changes.push(seed);
    crate::input::check(cancel)?;
    document.apply(changes)?;
    if let Some(symbol) = added_layer {
        document
            .added_layers
            .insert(crate::dxf_symbols::key(LAYER), symbol);
    }
    document.image_batch_added = true;
    Ok(())
}
