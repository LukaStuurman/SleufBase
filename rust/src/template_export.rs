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
    sync::Arc,
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub template: PathBuf,
    /// Full RD overview, outside the per-slot normal/reverse containers.
    #[serde(default)]
    pub overview: Option<crate::cadastral_overview::Request>,
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
    /// Discipline counting is independent of whether a profile is drawn.
    #[serde(default)]
    pub report_dataset: Option<usize>,
    #[serde(default)]
    pub report_label: Option<String>,
    /// A locally clipped, independently prepared BGT batch for this slot.
    #[serde(default)]
    pub surfaces: Option<Arc<Vec<bgt_surface::Feature>>>,
    #[serde(default)]
    pub surfaces_constructed: bool,
    #[serde(default)]
    pub photos: Vec<crate::template_photo::Photo>,
}
#[derive(serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub logos: crate::template_branding::Logos,
    pub project: Option<String>,
    pub drawn_by: String,
    /// Year, month, day. None captures the Windows local date once per export.
    pub export_date: Option<[u16; 3]>,
    pub label_gap: f64,
    /// Standalone manifests opt in; desktop workflows follow the user's setting.
    pub discipline_excel: bool,
    /// Keep both freshly rendered directions behind AutoCAD Versie.
    pub paired: bool,
    /// Local CAD font folders, later folders override duplicate filenames.
    pub font_directories: Vec<PathBuf>,
    /// Additional live project paths are never eligible for workbook replacement.
    #[serde(skip)]
    pub protected_sources: Vec<PathBuf>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            logos: Default::default(),
            project: None,
            drawn_by: String::new(),
            export_date: None,
            label_gap: 18.,
            discipline_excel: false,
            paired: false,
            font_directories: Vec::new(),
            protected_sources: Vec::new(),
        }
    }
}
#[derive(Clone,serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Profile {
    pub dataset: usize,
    /// Full virtual profile source, independent of the selected TIFF pane.
    pub virtual_trench: Option<crate::virtual_trench::VirtualTrench>,
    pub draw: bool,
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
            virtual_trench: None,
            draw: true,
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
    pub companion: Option<PathBuf>,
    pub warnings: Vec<String>,
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
    request.settings.protected_sources.push(path.to_owned());
    request
        .settings
        .protected_sources
        .extend(request.datasets.iter().map(|d| parent.join(&d.source_path)));
    let resolve = |p: &mut PathBuf| {
        if !p.as_os_str().is_empty() && p.is_relative() {
            *p = parent.join(&*p);
        }
    };
    resolve(&mut request.template);
    if let Some(overview)=&mut request.overview {
        crate::cadastral_overview::resolve_paths(overview,parent);
    }
    for logo in [&mut request.settings.logos.techbase, &mut request.settings.logos.client].into_iter().flatten() {
        resolve(logo);
    }
    for dir in &mut request.settings.font_directories {
        resolve(dir);
    }
    for slot in request.slots.iter_mut().flatten() {
        if let Some(source)=&mut slot.virtual_source {if let Some(path)=&mut source.trench.source_path {resolve(path);}}
        if let Some(trench)=slot.profile.as_mut().and_then(|p|p.virtual_trench.as_mut()) {if let Some(path)=&mut trench.source_path {resolve(path);}}
        resolve(&mut slot.tiff_png);
        if let Some(source) = &mut slot.tiff_source {
            resolve(&mut source.path);
        }
        resolve(&mut slot.map_png);
        for photo in &mut slot.photos {
            resolve(&mut photo.path);
        }
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
    datasets: &'a [Dataset],
    templates: &'a profile_drawing::Templates,
    surfaces: &'a bgt_surface::Index<'a>,
}
fn build_profile<'a>(
    request: &'a Request,
    datasets: &[Dataset],
    options: &Profile,
    reverse_override: Option<bool>,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Option<profile::Profile<'a>>, String> {
    let dataset = datasets
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
            reverse: reverse_override.unwrap_or(options.reverse),
            marker_scale: options.marker_diameter.max(0.005),
        },
        cancel,
    )
}
/// Deck bands are declared in the chosen profile direction and use the base
/// profile's marker-expanded surface. Keep this dataset local to one direction;
/// the original project/report dataset remains the measured points only.
fn virtual_profile_dataset(
    request:&Request,
    datasets:&[Dataset],
    options:&Profile,
    fallback:Option<&crate::virtual_trench::VirtualTrench>,
    reverse_override:Option<bool>,
    cancel:&std::sync::atomic::AtomicBool,
)->Result<Option<(Dataset,Profile)>,String>{
    let Some(trench)=options.virtual_trench.as_ref().or(fallback) else{return Ok(None)};
    trench.validate()?;
    let original=datasets.get(options.dataset).ok_or("Profiel verwijst naar een ontbrekende dataset")?;
    let reverse=reverse_override.unwrap_or(options.reverse);
    let endpoint=if reverse{trench.end}else{trench.start};
    let mut dataset=trench.to_base_dataset_with_cancel(original.job_id,true,cancel)?;
    // Avoid cloning the complete virtual source for every direction.
    let mut effective=Profile {
        dataset:options.dataset, virtual_trench:None, draw:options.draw,
        metadata:options.metadata.clone(),forced_start:options.forced_start,
        reverse:options.reverse,marker_diameter:options.marker_diameter,
        manual_scale:options.manual_scale,show_reference:options.show_reference,
        use_custom_maaiveld:options.use_custom_maaiveld,auto_fill_bgt:options.auto_fill_bgt,
        avoid_collisions:options.avoid_collisions,clip_markers:options.clip_markers,
        diameter_prefix:options.diameter_prefix.clone(),
    };
    effective.dataset=0;
    effective.reverse=reverse;
    effective.forced_start=Some([endpoint.x,endpoint.y]);
    if let Some(base)=build_profile(request,std::slice::from_ref(&dataset),&effective,None,cancel)?{
        trench.append_dekband_for_profile(&mut dataset,&base,cancel)?;
    }
    Ok(Some((dataset,effective)))
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
        datasets,
        templates,
        surfaces,
    } = context;
    let cancel = control.flag();
    let dataset = datasets
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
type SlotParents = std::collections::BTreeMap<usize, Vec<u64>>;
fn order_slot_entities(
    doc: &Document,
    first: u64,
    ranges: &[(u64, u64, usize)],
    image_order: &[usize],
    reorder: bool,
    control: &Control,
) -> Result<(Document, SlotParents), String> {
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
    let mut parents = SlotParents::new();
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
                // Georeferenced overview TIFFs remain in RD modelspace;
                // they are not contents of either per-slot profile state.
                if record.kind()=="IMAGE" && record.first(8)==Some("PROEFSLEUVEN_TIFF") {
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
                parents.entry(group).or_default().push(handle);
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
    if let Some(first_index) = first_index.filter(|_| reorder) {
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
    Ok((indexed, parents))
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
    progress: impl FnMut(&str),
) -> Result<Exported, String> {
    export_with_datasets(output, request, &request.datasets, control, progress)
}
/// Worker entry point: borrow the moved project datasets without cloning their
/// vertices into an intermediate request. The manifest API uses the same body.
pub fn export_with_datasets(
    output: &Path,
    request: &Request,
    datasets: &[Dataset],
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
    let directions = if request.settings.paired { 2 } else { 1 };
    let mut estimated_images = request.overview.as_ref().filter(|o|o.settings.include_tiff_images).map_or(0,|o|o.layers.len());
    for slot in request.slots.iter().flatten() {
        crate::template_photo::validate(&slot.photos)?;
        estimated_images = estimated_images
            .checked_add((2 + slot.photos.len()) * directions)
            .ok_or("Te veel sjabloonrasters")?;
        if estimated_images > 10_000 {
            return Err("Te veel sjabloonrasters voor beide richtingen".into());
        }
        if request.settings.paired
            && ((slot.tiff_source.is_none() && slot.virtual_source.is_none())
                || slot.map_source.is_none())
        {
            return Err(
                "Voor Normaal/Reverse zijn een originele TIFF/virtuele bron en een kaartbron nodig"
                    .into(),
            );
        }

        let available = usize::from(slot.tiff_source.is_some())
            + usize::from(slot.virtual_source.is_some())
            + usize::from(!slot.tiff_png.as_os_str().is_empty());
        if available != 1 {
            return Err("Geef per vak één TIFF-bron of één voorbereide TIFF-PNG op".into());
        }
        if slot.map_source.is_some() == !slot.map_png.as_os_str().is_empty() {
            return Err("Geef per vak één kaartbron of één voorbereide kaart-PNG op".into());
        }
    }
    if let Ok(target) = std::fs::canonicalize(output) {
        let sources = crate::template_report::input_paths(request, datasets);
        for source in sources {
            control.check()?;
            if std::fs::canonicalize(source).is_ok_and(|p| p == target) {
                return Err("Kies een uitvoerbestand dat verschilt van het sjabloon en de invoerafbeeldingen".into());
            }
        }
    }
    let mut font_catalog =
        if request.settings.paired || !request.settings.font_directories.is_empty() {
            Some(crate::dxf_fonts::Catalog::system_with_dirs(
                &request.settings.font_directories,
                control.flag(),
            )?)
        } else {
            None
        };
    let font_paths = font_catalog
        .as_ref()
        .map(|c| c.source_paths().map(Path::to_owned).collect::<Vec<_>>())
        .unwrap_or_default();
    if let Ok(target) = std::fs::canonicalize(output) {
        for font in &font_paths {
            control.check()?;
            if std::fs::canonicalize(font).is_ok_and(|p| p == target) {
                return Err("Het uitvoerpad verwijst naar een sjabloonlettertype".into());
            }
        }
    }
    let tx = Transaction::new(output, control)?;
    if let Some(overview)=&request.overview {crate::cadastral_overview::protect(output,overview)?;}
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
    let logos_per_page=usize::from(request.settings.logos.techbase.is_some())+usize::from(request.settings.logos.client.is_some());
    if estimated_images.saturating_add(prepared.pages.len().saturating_mul(logos_per_page))>10_000 {
        return Err("Sjabloonrasters en logo's overschrijden samen het afbeeldingsbudget".into());
    }
    control.set_stage(Stage::Planning);
    crate::template_branding::restore_contact(&mut doc, control.flag())?;
    template_geometry::add(&mut doc, &request.layers, &request.geometry, control.flag())?;
    let mut overview_plan=request.overview.as_ref().map(|overview| {
        progress("Kadastraal overzicht opbouwen");
        crate::cadastral_overview::populate(&mut doc,overview,&tx,control,&mut progress)
    }).transpose()?;
    let templates = if request
        .slots
        .iter()
        .flatten()
        .any(|s| s.profile.as_ref().is_some_and(|p| p.draw))
    {
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
    let mut total: u64 = 0;
    if let Some(plan)=&overview_plan {
        total=plan.asset_bytes()?;
        if total>2*1024*1024*1024{return Err("Overzichtsrasters overschrijden het exportbudget".into());}
    }
    let mut logo_assets: [Option<PathBuf>; 2] = [None, None];
    for (index, source) in [&request.settings.logos.techbase, &request.settings.logos.client].into_iter().enumerate() {
        if let Some(source) = source {
            if index == 1 && request.settings.logos.techbase.as_ref() == Some(source) {
                logo_assets[1] = logo_assets[0].clone();
                continue;
            }
            let destination = tx.asset_path(if index == 0 { "techbase-logo.png" } else { "client-logo.png" })?;
            let bytes = crate::template_branding::prepare_logo(source, &destination, control)?;
            total = total.checked_add(bytes).ok_or("Te veel logo-afbeeldingsdata")?;
            if total > 2 * 1024 * 1024 * 1024 { return Err("Afbeeldingen overschrijden het exportbudget".into()); }
            logo_assets[index] = Some(destination);
        }
    }
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
        let mut local_surfaces = None;
        let mut cached_map: Option<(Option<[f64; 2]>, PathBuf)> = None;
        let mut cached_photos = Vec::<PathBuf>::new();
        for direction in 0..directions {
            let reverse_override = request.settings.paired.then_some(direction == 1);
            let suffix = if request.settings.paired {
                if direction == 0 {
                    "_normaal"
                } else {
                    "_reverse"
                }
            } else {
                ""
            };
            let group = (index * directions + direction) * 2;
            let local_profile=slot.profile.as_ref()
                .map(|p|virtual_profile_dataset(request,datasets,p,slot.virtual_source.as_ref().map(|s|&s.trench),reverse_override,control.flag()))
                .transpose()?.flatten();
            let direction_datasets=local_profile.as_ref()
                .map_or(datasets,|(dataset,_)|std::slice::from_ref(dataset));
            let direction_options=local_profile.as_ref().map(|(_,p)|p).or(slot.profile.as_ref());
            let measured = direction_options
                .map(|p| build_profile(request, direction_datasets, p, reverse_override, control.flag()))
                .transpose()?
                .flatten();
            for (role, path, bounds, inset, preserve_aspect) in [
                ("tiff", &slot.tiff_png, target.tiff_box, 0.04, true),
                ("kaart", &slot.map_png, target.map_box, 0., false),
            ] {
                // Index, not visible label, guarantees uniqueness for duplicate PS names.
                let destination =
                    tx.asset_path(&format!("slot{:05}_{role}{suffix}.png", index + 1))?;
                if let Some(source) = slot.virtual_source.as_ref().filter(|_| role == "tiff") {
                    progress(&format!("Virtuele proefsleufraster {} opbouwen", index + 1));
                    let prepared = crate::virtual_raster::prepare_for_template_direction(
                        source,
                        reverse_override.unwrap_or(source.reverse),
                        control.flag(),
                    )?;
                    crate::template_raster::save(&prepared, &destination, control.flag())?;
                    let bytes = std::fs::metadata(&destination)
                        .map_err(|e| e.to_string())?
                        .len();
                    total = total
                        .checked_add(bytes)
                        .ok_or("Rasterbudget overschreden")?;
                    if bytes > 256 * 1024 * 1024 || total > 2 * 1024 * 1024 * 1024 {
                        return Err("Sjabloonafbeeldingen overschrijden het exportbudget".into());
                    }
                } else if let Some(source) = slot.tiff_source.as_ref().filter(|_| role == "tiff") {
                    let axis = measured
                        .as_ref()
                        .map(|p| [[p.start.x, p.start.y], [p.end.x, p.end.y]])
                        .or_else(|| {
                            source.axis.map(|[a, b]| {
                                if reverse_override == Some(true) {
                                    [b, a]
                                } else {
                                    [a, b]
                                }
                            })
                        });
                    let mut oriented = source.clone();
                    oriented.reverse_orientation =
                        reverse_override.unwrap_or(source.reverse_orientation);
                    let raster = crate::template_raster::from_tiff(
                        &oriented,
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
                    let reference = if let (Some(options), Some(profile)) =
                        (&slot.profile, &measured)
                    {
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
                    let effective_reference = source.reference.or(reference);
                    if let Some((_, previous)) = cached_map
                        .as_ref()
                        .filter(|(r, _)| *r == effective_reference)
                    {
                        copy_asset(previous, &destination, &mut total, control)?;
                    } else {
                        progress(&format!("Kaartblad {} opbouwen", index + 1));
                        control.check()?;
                        let page = map_renderer.render_with_raster(
                            source,
                            slot.bounds,
                            slot.tiff_source
                                .as_ref()
                                .map(crate::map_export::Raster::Tiff)
                                .or_else(|| {
                                    slot.virtual_source
                                        .as_ref()
                                        .map(crate::map_export::Raster::Virtual)
                                }),
                            reference,
                            control.flag(),
                        )?;
                        crate::template_raster::save_image(
                            &page.image,
                            &destination,
                            control.flag(),
                        )?;
                        let bytes = std::fs::metadata(&destination)
                            .map_err(|e| e.to_string())?
                            .len();
                        total += bytes;
                        if bytes > 256 * 1024 * 1024 || total > 2 * 1024 * 1024 * 1024 {
                            return Err(
                                "Sjabloonafbeeldingen overschrijden het exportbudget".into()
                            );
                        }
                        cached_map = Some((effective_reference, destination.clone()));
                    }
                } else {
                    copy_asset(path, &destination, &mut total, control)?;
                }
                images.push(Image {
                    path: destination,
                    bounds,
                    definition_name: format!("{}_{role}{suffix}", slot.label),
                    inset,
                    preserve_aspect,
                    layout: None,
                });
                image_order.push(group);
            }
            // Additional digital site photos are normalized into separate owned
            // PNG assets. They sit as inset overlays, never replace the full TIFF
            // or map and use the same atomic export transaction.
            if !slot.photos.is_empty() {
                progress(&format!("Werkfoto's voor vak {} verwerken", index + 1));
                let boxes =
                    crate::template_photo::boxes(target.tiff_box, target.map_box, &slot.photos)?;
                for (photo_index, (photo, bounds)) in slot.photos.iter().zip(boxes).enumerate() {
                    control.check()?;
                    let filename = format!(
                        "slot{:05}_photo{:02}{suffix}.png",
                        index + 1,
                        photo_index + 1
                    );
                    let destination = tx.asset_path(&filename)?;
                    let bytes = if let Some(previous) = cached_photos.get(photo_index) {
                        // Stream the already normalized PNG, without decoding the
                        // same high-resolution site photo again for Reverse.
                        copy_asset(previous, &destination, &mut total, control)?;
                        0
                    } else {
                        let bytes = crate::template_photo::prepare(photo, &destination, control)?;
                        cached_photos.push(destination.clone());
                        bytes
                    };
                    total = total
                        .checked_add(bytes)
                        .ok_or("Foto-uitvoerbudget overschreden")?;
                    if total > 2 * 1024 * 1024 * 1024 {
                        return Err("Sjabloonafbeeldingen overschrijden het exportbudget".into());
                    }
                    images.push(Image {
                        path: destination,
                        bounds,
                        definition_name: format!(
                            "{}_photo{:02}{suffix}",
                            slot.label,
                            photo_index + 1
                        ),
                        inset: 0.015,
                        preserve_aspect: true,
                        layout: None,
                    });
                    image_order.push(group);
                }
            }
            if let (Some(options), Some(measured)) =
                (direction_options.filter(|p| p.draw), &measured)
            {
                if local_surfaces.is_none() {
                    local_surfaces = slot
                        .surfaces
                        .as_ref()
                        .map(|s| {
                            if slot.surfaces_constructed {
                                bgt_surface::Index::from_normalized_features(
                                    s.iter(),
                                    control.flag(),
                                )
                            } else {
                                bgt_surface::Index::new(s, control.flag())
                            }
                        })
                        .transpose()?;
                }
                let start = doc.seed()?.0;
                profiles += usize::from(draw_profile(
                    &mut doc,
                    &ProfileContext {
                        templates: templates.as_ref().unwrap(),
                        datasets:direction_datasets,
                        surfaces: local_surfaces.as_ref().unwrap_or(&surfaces),
                    },
                    slot,
                    target,
                    options,
                    measured,
                    control,
                )?);
                profile_ranges.push((start, doc.seed()?.0, group + 1));
            }
        }
        filled_slots += 1;
    }
    progress("Afbeeldingen koppelen");
    control.check()?;
    let logo_layouts = prepared.pages.iter().map(|page| page.layout.clone()).collect::<Vec<_>>();
    images.extend(crate::template_branding::images(&doc, &logo_layouts,
        logo_assets[0].as_deref(), logo_assets[1].as_deref(), control.flag())?);
    if let Some(plan)=&mut overview_plan {images.append(&mut plan.images);}
    template_images::add_images(&mut doc, &images, control.flag())?;
    if let Some(plan)=&overview_plan {doc=plan.finalize(&doc,control)?;}
    if profiles > 0 || request.settings.paired {
        let (indexed, mut parents) = order_slot_entities(
            &doc,
            first_slot_handle,
            &profile_ranges,
            &image_order,
            !request.settings.paired,
            control,
        )?;
        doc = if request.settings.paired {
            let mut pairs = Vec::new();
            for (index, slot) in request.slots.iter().enumerate() {
                if let Some(slot) = slot {
                    let normal = index * 4;
                    let reverse = normal + 2;
                    let mut content = |group| {
                        let mut ids = parents.remove(&group).unwrap_or_default();
                        ids.extend(parents.remove(&(group + 1)).unwrap_or_default());
                        ids
                    };
                    pairs.push(crate::template_variants::Pair {
                        label: slot.label.clone(),
                        slot_index: (index + 1) as i64,
                        normal: content(normal),
                        reverse: content(reverse),
                        selector: None,
                    });
                }
            }
            progress("Normaal/Reverse-keuze koppelen");
            control.check()?;
            crate::template_variants::assemble_automatic_indexed(
                indexed,
                &pairs,
                &mut |name| font_catalog.as_mut().unwrap().resolve(name, control.flag()),
                control.flag(),
            )?
            .0
        } else {
            indexed
        };
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
    if let Some(overview)=&request.overview {crate::cadastral_overview::protect(output,overview)?;}
    let published = tx.publish(control)?;
    // Match the desktop contract: the DXF is already durable. A workbook failure
    // must be reported separately, never turn this into "DXF not saved". The
    // publication boundary has passed; this small final step is not cancellable.
    let report = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut warnings = Vec::new();
        let mut companion = None;
        if request.settings.discipline_excel {
            progress("Excel-disciplineoverzicht opslaan");
            match crate::template_report::write(output, &published, request, datasets, &font_paths)
            {
                Ok((path, messages)) => {
                    companion = Some(path);
                    warnings = messages;
                }
                Err(error) => warnings.push(format!(
                    "DXF opgeslagen; discipline-Excel kon niet worden gemaakt: {error}"
                )),
            }
        }
        (companion, warnings)
    }));
    let (companion, warnings) = report.unwrap_or_else(|_| {
        (
            None,
            vec!["DXF opgeslagen; discipline-Excel is onverwacht gestopt".into()],
        )
    });
    Ok(Exported {
        published,
        filled_slots,
        profiles,
        added_pages: prepared.added_pages,
        companion,
        warnings,
    })
}

#[cfg(test)]
mod virtual_profile_tests {
    use super::*;
    #[test]
    fn each_paired_direction_injects_original_bands_after_base_profile_without_mutating_reports(){
        let fixtures:serde_json::Value=serde_json::from_str(include_str!("../tests/fixtures/template-export/reference.json")).unwrap();
        let value=fixtures.as_array().unwrap().iter().find(|v|v["name"]=="native-tiff").unwrap()["request"].clone();
        let mut request:Request=serde_json::from_value(value).unwrap();
        request.rules=crate::RULES.to_vec();
        let oracle:serde_json::Value=serde_json::from_str(include_str!("../tests/fixtures/dekband/reference.json")).unwrap();
        let cancel=std::sync::atomic::AtomicBool::new(false);
        for measured in [false,true]{
            for selected_reverse in [false,true]{
                for override_reverse in [None,Some(false),Some(true)]{
                    let reverse=override_reverse.unwrap_or(selected_reverse);
                    let case=&oracle["cases"][usize::from(reverse)];
                    let mut trench:crate::virtual_trench::VirtualTrench=serde_json::from_value(case["source"].clone()).unwrap();
                    if measured {trench.boundary_3d=vec![trench.start,trench.end,crate::Vertex{x:trench.end.x+1.,..trench.end}];}
                    let dataset=trench.to_base_dataset_with_cancel(0,true,&cancel).unwrap();
                    let before=serde_json::to_vec(&dataset).unwrap();
                    let options=Profile{dataset:0,virtual_trench:Some(trench.clone()),reverse:selected_reverse,marker_diameter:0.1,..Default::default()};
                    let (local,effective)=virtual_profile_dataset(&request,std::slice::from_ref(&dataset),&options,None,override_reverse,&cancel).unwrap().unwrap();
                    assert_eq!(serde_json::to_vec(&dataset).unwrap(),before);
                    assert_eq!(dataset.features.len(),2,"report dataset must contain measured points only");
                    let expected=case["expected"].as_array().unwrap();
                    let lines=&local.features[2..];assert_eq!(lines.len(),expected.len());
                    for (line,row) in lines.iter().zip(expected){
                        assert_eq!(line.object_name,row["object_name"].as_str().unwrap());
                        let crate::Geometry::Polyline(points)=&line.geometry else{panic!("missing deck band")};
                        for (p,e) in points.iter().zip(row["geometry"]["Polyline"].as_array().unwrap()){
                            for (key,value) in [("x",p.x),("y",p.y),("z",p.z.unwrap())]{assert!((value-e[key].as_f64().unwrap()).abs()<1e-9,"{key}: {value} != {}",e[key]);}
                        }
                    }
                    let profile=build_profile(&request,std::slice::from_ref(&local),&effective,override_reverse,&cancel).unwrap().unwrap();
                    let start=if reverse{trench.end}else{trench.start};
                    assert_eq!((profile.start.x,profile.start.y,profile.start.z),(start.x,start.y,start.z));
                    assert!(profile.points.iter().all(|p|p.feature_index<local.features.len()));
                }
            }
        }
    }
}
