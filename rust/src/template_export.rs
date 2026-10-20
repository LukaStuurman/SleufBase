//! Native template assembly/publication from prepared PNGs or full TIFF/map
//! sources. Never substitutes previews or silently omits failed assets.
//! One owned bundle contains every new image.
use crate::{
    bgt_surface,
    dxf_document::Document,
    export_control::{CheckedWriter, Control, Stage},
    input, profile, profile_drawing, profile_leaders, profile_reference, profile_surface,
    template_geometry::{self, Entity, Layer, Shape},
    template_images::{self, Image},
    template_metadata::Date,
    template_setup,
    template_transaction::{Published, Transaction},
    Bounds, Dataset, Rule,
};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub template: PathBuf,
    pub slots: Vec<Option<Slot>>,
    #[serde(default)]
    pub datasets: Vec<Dataset>,
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub roads: Vec<Vec<profile::XY>>,
    #[serde(default)]
    pub terrain_boundaries: Vec<Vec<profile::XY>>,
    #[serde(default)]
    pub surfaces: Vec<bgt_surface::Feature>,
    #[serde(default)]
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub geometry: Vec<Entity>,
    #[serde(default)]
    pub settings: Settings,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    pub bounds: Bounds,
    pub label: String,
    pub title: String,
    pub address: Option<String>,
    pub comments: String,
    #[serde(default)]
    pub tiff_png: PathBuf,
    #[serde(default)]
    pub tiff_source: Option<crate::template_raster::Source>,
    #[serde(default)]
    pub virtual_source: Option<crate::virtual_raster::Source>,
    #[serde(default)]
    pub map_png: PathBuf,
    #[serde(default)]
    pub map_source: Option<crate::map_export::Source>,
    pub profile: Option<Profile>,
    #[serde(default)]
    pub photos: Vec<crate::template_photo::Photo>,
}
#[derive(serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub project: Option<String>,
    pub drawn_by: String,
    /// Year, month, day. None captures the Windows local date once per export.
    pub export_date: Option<[u16; 3]>,
    pub label_gap: f64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            project: None,
            drawn_by: String::new(),
            export_date: None,
            label_gap: 18.,
        }
    }
}
#[derive(serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Profile {
    pub dataset: usize,
    pub metadata: serde_json::Value,
    pub forced_start: Option<profile::XY>,
    pub reverse: bool,
    pub marker_diameter: f64,
    pub manual_scale: Option<i64>,
    pub show_reference: bool,
    pub use_custom_maaiveld: bool,
    pub auto_fill_bgt: bool,
    pub avoid_collisions: bool,
    pub clip_markers: bool,
    pub diameter_prefix: String,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            dataset: 0,
            metadata: serde_json::Value::Null,
            forced_start: None,
            reverse: false,
            marker_diameter: 0.02,
            manual_scale: None,
            show_reference: true,
            use_custom_maaiveld: true,
            auto_fill_bgt: true,
            avoid_collisions: true,
            clip_markers: false,
            diameter_prefix: "Ø".into(),
        }
    }
}
#[derive(Debug)]
pub struct Exported {
    pub published: Published,
    pub filled_slots: usize,
    pub profiles: usize,
    pub added_pages: usize,
}
/// Resolve manifest-relative input paths without changing process working dir.
/// Project preferences/credentials are not read by this prepared-input path.
pub fn read_request(path: &Path, control: &Control) -> Result<Request, String> {
    let data = input::read(
        path,
        64 * 1024 * 1024,
        "Sjabloonexportinvoer",
        control.flag(),
    )?;
    let mut request: Request = serde_json::from_slice(&data)
        .map_err(|e| format!("Ongeldige sjabloonexportinvoer: {e}"))?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let resolve = |p: &mut PathBuf| {
        if !p.as_os_str().is_empty() && p.is_relative() {
            *p = parent.join(&*p);
        }
    };
    resolve(&mut request.template);
    for slot in request.slots.iter_mut().flatten() {
        resolve(&mut slot.tiff_png);
        if let Some(source) = &mut slot.tiff_source {
            resolve(&mut source.path);
        }
        resolve(&mut slot.map_png);
        for photo in &mut slot.photos {resolve(&mut photo.path);}
        if let Some(source) = &mut slot.map_source {
            source.resolve_paths(parent);
        }
    }
    control.check()?;
    Ok(request)
}
fn copy_asset(
    source: &Path,
    destination: &Path,
    total: &mut u64,
    control: &Control,
) -> Result<(), String> {
    let mut source =
        File::open(source).map_err(|e| format!("Voorbereide afbeelding ontbreekt: {e}"))?;
    if source.metadata().map_err(|e| e.to_string())?.len() > 256 * 1024 * 1024 {
        return Err("Voorbereide afbeelding is te groot".into());
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|e| e.to_string())?;
    let mut buffer = [0u8; 64 * 1024];
    let mut copied = 0u64;
    loop {
        control.check()?;
        let count = match source.read(&mut buffer) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            r => r.map_err(|e| e.to_string())?,
        };
        if count == 0 {
            break;
        }
        copied += count as u64;
        *total += count as u64;
        if copied > 256 * 1024 * 1024 || *total > 2 * 1024 * 1024 * 1024 {
            return Err("Sjabloonafbeeldingen overschrijden het exportbudget".into());
        }
        output
            .write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
    }
    control.check()?;
    if copied == 0 {
        return Err("Voorbereide afbeelding is leeg".into());
    }
    // Verify the actual PNG, not just its dimensions/signature. Decode one
    // bounded image at a time so corrupt pixel streams cannot be published.
    drop(output);
    let mut reader = image::ImageReader::open(destination).map_err(|e| e.to_string())?;
    reader.set_format(image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|e| format!("Ongeldige voorbereide PNG: {e}"))?;
    control.check()
}
struct ProfileContext<'a> {
    request: &'a Request,
    templates: &'a profile_drawing::Templates,
    surfaces: &'a bgt_surface::Index<'a>,
}
fn build_profile<'a>(
    request: &'a Request,
    options: &Profile,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Option<profile::Profile<'a>>, String> {
    let dataset = request
        .datasets
        .get(options.dataset)
        .ok_or("Profiel verwijst naar een ontbrekende dataset")?;
    if !options.marker_diameter.is_finite() {
        return Err("Ongeldige profielmarkerdiameter".into());
    }
    profile::build_with_cancel(
        dataset,
        &request.rules,
        profile::Options {
            roads: &request.roads,
            terrain_boundaries: &request.terrain_boundaries,
            forced_start: options.forced_start,
            reverse: options.reverse,
            marker_scale: options.marker_diameter.max(0.005),
        },
        cancel,
    )
}
fn draw_profile(
    doc: &mut Document,
    context: &ProfileContext<'_>,
    slot: &Slot,
    target: &crate::template_slots::Slot,
    options: &Profile,
    profile: &profile::Profile<'_>,
    control: &Control,
) -> Result<bool, String> {
    let ProfileContext {
        request,
        templates,
        surfaces,
    } = context;
    let cancel = control.flag();
    let dataset = request
        .datasets
        .get(options.dataset)
        .ok_or("Profiel verwijst naar een ontbrekende dataset")?;
    let marker_scale = options.marker_diameter.max(0.005);
    template_prepare_profile(doc, profile, cancel)?;
    let reference = if options.show_reference {
        Some(profile_reference::resolve(profile, &options.metadata)?)
    } else {
        None
    };
    let Some(mut drawing) = profile_drawing::frame_and_bands(
        profile,
        dataset,
        [target.row_box, target.tiff_box],
        templates,
        profile_drawing::Options {
            label: &slot.label,
            manual_scale: options.manual_scale,
            marker_scale,
            reference_chainage: reference.as_ref().map_or(0., |r| r.chainage),
            distance_description: reference.as_ref().map_or("", |r| r.description.as_str()),
        },
        cancel,
    )?
    else {
        return Ok(false);
    };
    let automatic = if options.auto_fill_bgt {
        surfaces.texts(profile, cancel)?
    } else {
        serde_json::json!({})
    };
    let fills =
        profile_surface::resolve(&options.metadata, &automatic, options.use_custom_maaiveld)?;
    let mut body = profile_surface::build(
        profile,
        dataset,
        &drawing,
        target.tiff_box,
        &fills,
        profile_surface::Options {
            clip_markers: options.clip_markers,
            marker_in_leader: false,
        },
        cancel,
    )?;
    if let Some(reference) = &reference {
        profile_reference::draw(
            profile,
            drawing.layout,
            target.tiff_box.min_x,
            reference,
            cancel,
        )?
        .append(&mut body);
    }
    let (mut layers, obstacles) = profile_surface::compose(&mut drawing, body)?;
    if drawing
        .entities
        .iter()
        .any(|e| matches!(&e.shape,Shape::Block{name,..} if name==profile_surface::MARKER))
    {
        profile_surface::prepare_marker(doc, cancel)?;
    }
    let prefix = crate::python_values::trim(&options.diameter_prefix);
    let leaders = profile_leaders::compose(
        profile,
        dataset,
        &mut drawing,
        &obstacles,
        profile_leaders::ProfileOptions {
            marker_scale,
            diameter_prefix: if prefix.is_empty() { "Ø" } else { prefix },
            avoid_collisions: options.avoid_collisions,
        },
        cancel,
    )?;
    if !leaders.entries.is_empty() {
        profile_leaders::prepare(doc, cancel)?;
        layers.push(Layer {
            name: profile_leaders::LAYER.into(),
            color: 7,
            true_color: None,
            lineweight: Some(0),
        });
    }
    template_geometry::add(doc, &layers, &drawing.entities, cancel)?;
    Ok(true)
}
fn template_prepare_profile(
    doc: &mut Document,
    profile: &profile::Profile<'_>,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    crate::template_prepare::profile(
        doc,
        profile
            .points
            .iter()
            .map(|p| (p.layer_name.as_ref(), p.color)),
        cancel,
    )
}
/// Images are built in one graph batch; restore the source per-slot draw order
/// afterwards with one reindex, rather than reloading a large DXF per image.
fn order_slot_entities(
    doc: &mut Document,
    first: u64,
    ranges: &[(u64, u64, usize)],
    image_order: &[usize],
    control: &Control,
) -> Result<(), String> {
    use crate::template_copy as copy;
    use std::collections::BTreeMap;
    let mut indexed = doc.indexed_copy(control.flag())?;
    let model = indexed
        .records()
        .find(|r| {
            r.kind() == "BLOCK_RECORD"
                && r.first(2)
                    .is_some_and(|n| n.eq_ignore_ascii_case("*Model_Space"))
        })
        .and_then(|r| r.handle())
        .ok_or("Sjabloon mist modelruimte")?;
    let mut groups: BTreeMap<usize, Vec<_>> = BTreeMap::new();
    let mut images = 0;
    let mut active = None;
    let mut first_index = None;
    let mut last_index = 0;
    let mut count = 0;
    for record in indexed
        .records()
        .filter(|r| r.section() == Some("ENTITIES"))
    {
        control.check()?;
        let child = matches!(record.kind(), "ATTRIB" | "VERTEX" | "SEQEND");
        if !child {
            active = None;
            if let Some(handle) = record.handle().filter(|&h| h >= first) {
                if copy::owner(record) != Some(model) {
                    continue;
                }
                let group = if record.kind() == "IMAGE" {
                    let order = *image_order
                        .get(images)
                        .ok_or("Onverwachte nieuwe sjabloonafbeelding")?;
                    images += 1;
                    order
                } else {
                    ranges
                        .partition_point(|&(start, _, _)| start <= handle)
                        .checked_sub(1)
                        .and_then(|i| ranges.get(i))
                        .filter(|&&(_, end, _)| handle < end)
                        .map(|&(_, _, order)| order)
                        .ok_or("Onverwacht nieuw sjabloonobject")?
                };
                active = Some(group);
            }
        }
        if let Some(group) = active {
            groups.entry(group).or_default().push(record);
            first_index.get_or_insert(record.index());
            last_index = record.index();
            count += 1;
        }
    }
    if images != image_order.len() {
        return Err("Niet alle sjabloonafbeeldingen zijn gekoppeld".into());
    }
    if let Some(first_index) = first_index {
        if last_index - first_index + 1 != count {
            return Err("Nieuwe sjabloonobjecten zijn niet aaneengesloten".into());
        }
        let mut text = String::new();
        for group in groups.values() {
            for &record in group {
                control.check()?;
                text.push_str(&copy::encode(&copy::tags(record)));
            }
        }
        let change = crate::dxf_document::Change {
            start: indexed.record_start(first_index),
            end: indexed.record_start(last_index + 1),
            text,
        };
        drop(groups);
        indexed.apply(vec![change])?;
    }
    *doc = indexed;
    Ok(())
}
/// Prepared asset assembly is separate from raster/server-data preparation.
/// No previous drawing or bundle changes until publication succeeds.
pub fn export(output: &Path, request: &Request, control: &Control) -> Result<Exported, String> {
    export_with_progress(output, request, control, |_| {})
}
/// Callbacks run on the worker; errors, cancellation and panics drop only this
/// attempt's owned bundle. A successful publication is the last fallible step.
pub fn export_with_progress(
    output: &Path,
    request: &Request,
    control: &Control,
    mut progress: impl FnMut(&str),
) -> Result<Exported, String> {
    control.check()?;
    if request.slots.is_empty() || !request.slots.iter().any(Option::is_some) {
        return Err("Voeg minimaal één gevuld proefsleufvak toe".into());
    }
    if request.slots.len() > 5000 {
        return Err("Te veel sjabloonvakken voor het afbeeldingsbudget".into());
    }
    for slot in request.slots.iter().flatten() {
        crate::template_photo::validate(&slot.photos)?;
        let available=usize::from(slot.tiff_source.is_some())
            +usize::from(slot.virtual_source.is_some())
            +usize::from(!slot.tiff_png.as_os_str().is_empty());
        if available!=1 {
            return Err("Geef per vak één TIFF-bron of één voorbereide TIFF-PNG op".into());
        }
        if slot.map_source.is_some() == !slot.map_png.as_os_str().is_empty() {
            return Err("Geef per vak één kaartbron of één voorbereide kaart-PNG op".into());
        }
    }
    if let Ok(target) = std::fs::canonicalize(output) {
        let sources = std::iter::once(request.template.as_path()).chain(
            request.slots.iter().flatten().flat_map(|s| {
                let mut paths = vec![
                    s.tiff_source
                        .as_ref()
                        .map_or(s.tiff_png.as_path(), |t| t.path.as_path()),
                    s.map_png.as_path(),
                ];
                paths.extend(s.photos.iter().map(|p|p.path.as_path()));
                if let Some(source) = &s.map_source {
                    paths.extend(source.input_paths());
                }
                paths
            }),
        );
        for source in sources {
            control.check()?;
            if std::fs::canonicalize(source).is_ok_and(|p| p == target) {
                return Err("Kies een uitvoerbestand dat verschilt van het sjabloon en de invoerafbeeldingen".into());
            }
        }
    }
    let tx = Transaction::new(output, control)?;
    progress("Sjabloon voorbereiden");
    control.check()?;
    let mut doc = Document::load(&request.template, control.flag())?;
    let bounds = request
        .slots
        .iter()
        .map(|s| s.as_ref().map(|s| s.bounds))
        .collect::<Vec<_>>();
    let date = request
        .settings
        .export_date
        .map(|[y, m, d]| Date::new(y, m, d))
        .transpose()?;
    let prepared = template_setup::prepare_with_progress(
        &mut doc,
        &bounds,
        &template_setup::Options {
            project: request.settings.project.clone(),
            drawn_by: request.settings.drawn_by.clone(),
            export_date: date,
            label_gap: request.settings.label_gap,
            profile_layers: None,
        },
        control.flag(),
        &mut progress,
    )?;
    control.set_stage(Stage::Planning);
    template_geometry::add(&mut doc, &request.layers, &request.geometry, control.flag())?;
    let templates = if request.slots.iter().flatten().any(|s| s.profile.is_some()) {
        Some(profile_drawing::Templates::read(&doc, control.flag())?)
    } else {
        None
    };
    let surfaces = bgt_surface::Index::new(&request.surfaces, control.flag())?;
    let first_slot_handle = doc.seed()?.0;
    let mut profile_ranges = Vec::new();
    let mut image_order = Vec::new();
    let mut images = Vec::new();
    let mut profiles = 0;
    let mut filled_slots = 0;
    let mut total = 0;
    let mut map_renderer = crate::map_export::Renderer::default();
    for (index, (source, target)) in request.slots.iter().zip(&prepared.slots).enumerate() {
        progress(&format!("Proefsleufvak {} invullen", index + 1));
        control.check()?;
        let Some(slot) = source else {
            doc.replace_text(&target.label_handle, "")?;
            if let Some(h) = &target.address_handle {
                doc.replace_text(h, "Adres:")?;
            }
            if let Some(h) = &target.comments_handle {
                doc.replace_text(h, "Opmerkingen:")?;
            }
            continue;
        };
        doc.replace_text(&target.label_handle, &slot.title)?;
        if let Some(h) = &target.address_handle {
            doc.replace_text(
                h,
                slot.address
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Adres:"),
            )?;
        }
        if let Some(h) = &target.comments_handle {
            doc.replace_text(h, &slot.comments)?;
        }
        let measured = slot
            .profile
            .as_ref()
            .map(|p| build_profile(request, p, control.flag()))
            .transpose()?
            .flatten();
        for (role, path, bounds, inset, preserve_aspect) in [
            ("tiff", &slot.tiff_png, target.tiff_box, 0.04, true),
            ("kaart", &slot.map_png, target.map_box, 0., false),
        ] {
            // Index, not visible label, guarantees uniqueness for duplicate PS names.
            let destination = tx.asset_path(&format!("slot{:05}_{role}.png", index + 1))?;
            if let Some(source) = slot.virtual_source.as_ref().filter(|_| role == "tiff") {
                progress(&format!("Virtuele proefsleufraster {} opbouwen", index+1));
                let image=crate::virtual_raster::render(source,control.flag())?;
                crate::template_raster::save_image(&image,&destination,control.flag())?;
                let bytes=std::fs::metadata(&destination).map_err(|e|e.to_string())?.len();
                total=total.checked_add(bytes).ok_or("Rasterbudget overschreden")?;
                if bytes>256*1024*1024||total>2*1024*1024*1024 {
                    return Err("Sjabloonafbeeldingen overschrijden het exportbudget".into());
                }
            } else if let Some(source) = slot.tiff_source.as_ref().filter(|_| role == "tiff") {
                let axis = measured
                    .as_ref()
                    .map(|p| [[p.start.x, p.start.y], [p.end.x, p.end.y]]);
                let raster = crate::template_raster::from_tiff(
                    source,
                    &request.roads,
                    &request.terrain_boundaries,
                    axis,
                    control.flag(),
                )?;
                crate::template_raster::save(&raster, &destination, control.flag())?;
                let bytes = std::fs::metadata(&destination)
                    .map_err(|e| e.to_string())?
                    .len();
                total += bytes;
                if bytes > 256 * 1024 * 1024 || total > 2 * 1024 * 1024 * 1024 {
                    return Err("Sjabloonafbeeldingen overschrijden het exportbudget".into());
                }
            } else if let Some(source) = slot.map_source.as_ref().filter(|_| role == "kaart") {
                let reference = if let (Some(options), Some(profile)) = (&slot.profile, &measured) {
                    if options.show_reference {
                        Some(
                            profile_reference::resolve(profile, &options.metadata)?.display(
                                &options.metadata,
                                [
                                    (slot.bounds.min_x + slot.bounds.max_x) / 2.,
                                    (slot.bounds.min_y + slot.bounds.max_y) / 2.,
                                ],
                            )?,
                        )
                    } else {
                        None
                    }
                } else {
                    None
                };
                progress(&format!("Kaartblad {} opbouwen", index + 1));
                control.check()?;
                let page = map_renderer.render(
                    source,
                    slot.bounds,
                    slot.tiff_source.as_ref(),
                    reference,
                    control.flag(),
                )?;
                crate::template_raster::save_image(&page.image, &destination, control.flag())?;
                let bytes = std::fs::metadata(&destination)
                    .map_err(|e| e.to_string())?
                    .len();
                total += bytes;
                if bytes > 256 * 1024 * 1024 || total > 2 * 1024 * 1024 * 1024 {
                    return Err("Sjabloonafbeeldingen overschrijden het exportbudget".into());
                }
            } else {
                copy_asset(path, &destination, &mut total, control)?;
            }
            images.push(Image {
                path: destination,
                bounds,
                definition_name: format!("{}_{role}", slot.label),
                inset,
                preserve_aspect,
                layout: None,
            });
            image_order.push(index * 2);
        }
        // Additional digital site photos are normalized into separate owned
        // PNG assets. They sit as inset overlays, never replace the full TIFF
        // or map and use the same atomic export transaction.
        if !slot.photos.is_empty() {
            progress(&format!("Werkfoto's voor vak {} verwerken",index+1));
            let boxes=crate::template_photo::boxes(target.tiff_box,target.map_box,&slot.photos)?;
            for (photo_index,(photo,bounds)) in slot.photos.iter().zip(boxes).enumerate() {
                control.check()?;
                let filename=format!("slot{:05}_photo{:02}.png",index+1,photo_index+1);
                let destination=tx.asset_path(&filename)?;
                let bytes=crate::template_photo::prepare(photo,&destination,control)?;
                total=total.checked_add(bytes).ok_or("Foto-uitvoerbudget overschreden")?;
                if total>2*1024*1024*1024 {return Err("Sjabloonafbeeldingen overschrijden het exportbudget".into());}
                images.push(Image{
                    path:destination,bounds,
                    definition_name:format!("{}_photo{:02}",slot.label,photo_index+1),
                    inset:0.015,preserve_aspect:true,layout:None,
                });
                image_order.push(index*2);
            }
        }
        if let (Some(options), Some(measured)) = (&slot.profile, &measured) {
            let start = doc.seed()?.0;
            profiles += usize::from(draw_profile(
                &mut doc,
                &ProfileContext {
                    templates: templates.as_ref().unwrap(),
                    request,
                    surfaces: &surfaces,
                },
                slot,
                target,
                options,
                measured,
                control,
            )?);
            profile_ranges.push((start, doc.seed()?.0, index * 2 + 1));
        }
        filled_slots += 1;
    }
    progress("Afbeeldingen koppelen");
    control.check()?;
    template_images::add_images(&mut doc, &images, control.flag())?;
    if profiles > 0 {
        order_slot_entities(
            &mut doc,
            first_slot_handle,
            &profile_ranges,
            &image_order,
            control,
        )?;
    }
    progress("DXF schrijven");
    control.check()?;
    control.set_stage(Stage::Writing);
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(tx.staged_drawing())
        .map_err(|e| e.to_string())?;
    let mut writer = CheckedWriter { out: file, control };
    doc.write_to(&mut writer, control.flag())?;
    writer.flush().map_err(|e| e.to_string())?;
    drop(writer);
    progress("Export definitief opslaan");
    control.check()?;
    let published = tx.publish(control)?;
    Ok(Exported {
        published,
        filled_slots,
        profiles,
        added_pages: prepared.added_pages,
    })
}
