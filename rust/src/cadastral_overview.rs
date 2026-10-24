//! Complete standalone cadastral overview export from native raster sources.
//! Server fetching is upstream; IMAGE ownership, CAD primitives and atomic
//! publication reuse the template exporter rather than a second DXF writer.
use crate::{
    cadastral::{CadLabel, CadLinework},
    dxf_document::{Change, Document},
    export_control::Control,
    geo::GeoTransform,
    template_geometry::{self as geometry, Entity, Paint, Shape},
    template_transaction::{Published, Transaction},
    Bounds,
};
use image::RgbaImage;
use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
#[path = "cadastral_overview_assets.rs"]
mod assets;
#[path = "cadastral_overview_mask.rs"]
mod mask;
const BASE: &str = include_str!("cadastral_overview_base.txt");
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrenchMode {
    #[default]
    Polygon,
    Centerline,
    None,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub trench_mode: TrenchMode,
    pub include_tiff_images: bool,
    pub label_gap: f64,
    pub centerline_color: [u8; 3],
    pub label_color: [u8; 3],
    pub protected_sources: Vec<PathBuf>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            trench_mode: TrenchMode::Polygon,
            include_tiff_images: false,
            label_gap: 6.,
            centerline_color: [0; 3],
            label_color: [0; 3],
            protected_sources: Vec::new(),
        }
    }
}
impl Settings {
    pub fn from_settings(
        settings: &crate::settings_compat::MigratedSettings,
    ) -> Result<Self, String> {
        settings.validate()?;
        let p = &settings.export_preferences;
        let color = |value: &str| -> [u8; 3] {
            std::array::from_fn(|i| {
                u8::from_str_radix(&value[1 + i * 2..3 + i * 2], 16)
                    .expect("validated RGB preference")
            })
        };
        Ok(Self {
            trench_mode: match p.dxf_trench_export_mode.as_str() {
                "centerline" => TrenchMode::Centerline,
                "none" => TrenchMode::None,
                _ => TrenchMode::Polygon,
            },
            include_tiff_images: p.include_tiffs_in_dxf_export,
            label_gap: settings.dxf_label_gap,
            centerline_color: color(&p.dxf_centerline_color),
            label_color: color(&p.dxf_label_color),
            protected_sources: Vec::new(),
        })
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub path: PathBuf,
    pub transform: GeoTransform,
    pub bounds: Bounds,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub virtual_trench: Option<crate::virtual_trench::VirtualTrench>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub linework: Vec<CadLinework>,
    #[serde(default)]
    pub labels: Vec<CadLabel>,
    #[serde(default)]
    pub settings: Settings,
}
#[derive(Debug)]
pub struct ResultInfo {
    pub published: Published,
    pub layers: usize,
    pub images: usize,
    pub export_bounds: Bounds,
}
pub fn resolve_paths(request: &mut Request, parent: &Path) {
    for layer in &mut request.layers {
        if layer.path.is_relative() {
            layer.path = parent.join(&layer.path);
        }
        if let Some(t) = &mut layer.virtual_trench {
            if let Some(p) = &mut t.source_path {
                if p.is_relative() {
                    *p = parent.join(&*p);
                }
            }
        }
    }
    for p in &mut request.settings.protected_sources {
        if p.is_relative() {
            *p = parent.join(&*p);
        }
    }
}
pub fn read_request(path: &Path, control: &Control) -> Result<Request, String> {
    let data = crate::input::read(
        path,
        64 * 1024 * 1024,
        "Kadastrale exportinvoer",
        control.flag(),
    )?;
    let mut request: Request = serde_json::from_slice(&data)
        .map_err(|e| format!("Ongeldige kadastrale exportinvoer: {e}"))?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    resolve_paths(&mut request, parent);
    request.settings.protected_sources.push(path.to_owned());
    Ok(request)
}
fn rgb(c: [u8; 3]) -> u32 {
    u32::from(c[0]) << 16 | u32::from(c[1]) << 8 | u32::from(c[2])
}
fn valid_bounds(b: Bounds) -> bool {
    [b.min_x, b.min_y, b.max_x, b.max_y]
        .into_iter()
        .all(|v| v.is_finite() && v.abs() < 1e12)
        && b.min_x <= b.max_x
        && b.min_y <= b.max_y
}
fn points_bounds(points: &[[f64; 2]]) -> Result<Bounds, String> {
    let mut b = Bounds {
        min_x: f64::INFINITY,
        min_y: f64::INFINITY,
        max_x: f64::NEG_INFINITY,
        max_y: f64::NEG_INFINITY,
    };
    for p in points {
        b.min_x = b.min_x.min(p[0]);
        b.max_x = b.max_x.max(p[0]);
        b.min_y = b.min_y.min(p[1]);
        b.max_y = b.max_y.max(p[1]);
    }
    if !valid_bounds(b) {
        return Err("Ongeldige proefsleufgeometrie".into());
    }
    Ok(b)
}
pub(crate) fn protect(output: &Path, request: &Request) -> Result<(), String> {
    let target = fs::canonicalize(output).ok();
    let mut paths: Vec<&Path> = request.layers.iter().map(|l| l.path.as_path()).collect();
    paths.extend(
        request
            .settings
            .protected_sources
            .iter()
            .map(PathBuf::as_path),
    );
    paths.extend(request.layers.iter().filter_map(|l| {
        l.virtual_trench
            .as_ref()
            .and_then(|t| t.source_path.as_deref())
    }));
    for source in paths.into_iter().filter(|p| !p.as_os_str().is_empty()) {
        if source == output
            || target
                .as_ref()
                .is_some_and(|p| fs::canonicalize(source).is_ok_and(|s| s == *p))
        {
            return Err(
                "Het DXF-pad verwijst naar een geladen bron; kies een ander uitvoerbestand".into(),
            );
        }
    }
    if let Ok(m) = fs::symlink_metadata(output) {
        if !m.is_file() || m.file_type().is_symlink() {
            return Err("DXF-uitvoer is geen gewoon bestand".into());
        }
    }
    Ok(())
}
fn basename(path: &Path, fallback: usize) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    // Original filename-only expression; the object-export job-title naming
    // contract differs for underscores and is intentionally not substituted.
    let chars: Vec<_> = stem.chars().collect();
    let digits = |start: usize| -> Option<String> {
        let mut result = String::new();
        for &c in &chars[start..] {
            let Some(d) = crate::object_layout::decimal(c) else {
                break;
            };
            result.push(char::from(b'0' + d as u8));
        }
        let n = result.trim_start_matches('0');
        if result.is_empty() {
            None
        } else {
            Some(if n.is_empty() { "0".into() } else { n.into() })
        }
    };
    for i in 0..chars.len().saturating_sub(1) {
        if chars[i].eq_ignore_ascii_case(&'p')
            && matches!(chars[i + 1], 's' | 'S' | 'ſ')
            && (i == 0 || !chars[i - 1].is_ascii_alphanumeric())
        {
            let mut j = i + 2;
            while j < chars.len()
                && (chars[j].is_whitespace() || matches!(chars[j], '.' | '_' | '-'))
            {
                j += 1;
            }
            if let Some(n) = digits(j) {
                return format!("PS{n}");
            }
        }
    }
    for i in 0..chars.len() {
        if let Some(n) = digits(i) {
            return format!("PS{n}");
        }
    }
    let compact =
        crate::python_values::uppercase(&stem.split_whitespace().collect::<Vec<_>>().join(" "));
    if compact.is_empty() {
        format!("PS{fallback}")
    } else {
        compact.chars().take(32).collect()
    }
}
fn label(layer: &Layer, index: usize) -> String {
    let explicit = crate::python_values::uppercase(crate::python_values::trim(
        layer
            .metadata
            .get("template_proefsleuf_label")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    ));
    let pattern = regex::Regex::new(r"^PS\d+[A-Z]*$").unwrap();
    if pattern.is_match(&explicit) {
        explicit
    } else {
        basename(&layer.path, index)
    }
}
struct Prepared {
    image: RgbaImage,
    tx: GeoTransform,
    bounds: Bounds,
}
fn prepare(layer: &Layer, cancel: &AtomicBool) -> Result<Prepared, String> {
    if let Some(trench) = &layer.virtual_trench {
        let layout = crate::virtual_raster::layout(trench, 2.5)?;
        let image = crate::virtual_raster::render(
            &crate::virtual_raster::Source {
                trench: trench.clone(),
                quality: 2.5,
                reverse: false,
            },
            cancel,
        )?;
        return Ok(Prepared {
            image,
            tx: layout.transform(),
            bounds: layer.bounds,
        });
    }
    let decoded = crate::raster::decode_full_with_cancel(&layer.path, cancel)?;
    let image = RgbaImage::from_raw(decoded.width, decoded.height, decoded.rgba)
        .ok_or("Onvolledig TIFF-raster")?;
    layer.transform.bounds(image.width(), image.height())?;
    Ok(Prepared {
        image,
        tx: layer.transform,
        bounds: layer.bounds,
    })
}
fn polyline(layer: &str, vertices: Vec<[f64; 2]>, paint: Paint) -> Entity {
    Entity {
        layer: layer.into(),
        paint,
        shape: Shape::Polyline {
            vertices,
            closed: false,
        },
        layout: None,
    }
}
fn text(
    layer: &str,
    value: String,
    p: [f64; 2],
    height: f64,
    rotation: f64,
    style: &str,
    paint: Paint,
) -> Entity {
    Entity {
        layer: layer.into(),
        paint,
        shape: Shape::Text {
            text: value,
            insert: p,
            height,
            rotation,
            style: style.into(),
            halign: 1,
            valign: 2,
        },
        layout: None,
    }
}
fn header(doc: &mut Document, b: Bounds, settings: &Settings) -> Result<(), String> {
    let mut changes = Vec::new();
    for (name, values) in [
        ("$EXTMIN", [b.min_x, b.min_y, 0.]),
        ("$EXTMAX", [b.max_x, b.max_y, 0.]),
    ] {
        for (code, value) in [10, 20, 30].into_iter().zip(values) {
            changes.push(doc.header_scalar(name, code, &value.to_string())?);
        }
    }
    for r in doc.records().filter(|r| r.kind() == "LAYER") {
        let color = match r.first(2) {
            Some("PROEFSLEUVEN_HARTLIJN") => Some(settings.centerline_color),
            Some("PROEFSLEUVEN_LABEL") => Some(settings.label_color),
            _ => None,
        };
        if let Some(color) = color {
            changes.push(doc.tag_change(r.index(), 420, &rgb(color).to_string())?);
        }
    }
    doc.apply(changes)
}
fn hatch_text(id: u64, owner: u64, polygon: &[[f64; 2]]) -> String {
    let mut out=format!("0\nHATCH\n5\n{id:X}\n330\n{owner:X}\n100\nAcDbEntity\n8\nPROEFSLEUVEN_VLAK\n62\n7\n420\n16777215\n100\nAcDbHatch\n10\n0\n20\n0\n30\n0\n210\n0\n220\n0\n230\n1\n2\nSOLID\n70\n1\n71\n0\n91\n1\n92\n3\n72\n0\n73\n1\n93\n{}\n",polygon.len());
    for p in polygon {
        writeln!(out, "10\n{}\n20\n{}", p[0], p[1]).unwrap();
    }
    out.push_str("97\n0\n75\n0\n76\n1\n98\n0\n");
    out
}

/// Defaults used by desktop template planners, matching their original setting.
pub fn template_defaults() -> Settings {
    Settings {
        trench_mode: TrenchMode::Centerline,
        ..Settings::default()
    }
}
fn overview_layers(settings: &Settings) -> Vec<geometry::Layer> {
    let rows = [
        ("BGT_VECTOR_OMTREK", 8, Some(rgb([155; 3])), Some(9)),
        ("BGT_WEGDEEL", 8, Some(rgb([145; 3])), Some(9)),
        ("KAD_PERCEEL", 8, Some(rgb([150; 3])), Some(13)),
        ("KAD_GRENS", 8, Some(rgb([110; 3])), Some(18)),
        ("KAD_BEBOUWING", 8, Some(rgb([175; 3])), Some(18)),
        ("KAD_STRAATNAAM", 8, Some(rgb([115; 3])), None),
        ("KAD_HUISNUMMER", 8, Some(rgb([95; 3])), None),
        ("PROEFSLEUVEN_VLAK", 7, Some(0xffffff), None),
        ("PROEFSLEUVEN_OMTREK", 8, Some(rgb([120; 3])), Some(25)),
        (
            "PROEFSLEUVEN_HARTLIJN",
            7,
            Some(rgb(settings.centerline_color)),
            Some(25),
        ),
        ("PROEFSLEUVEN_TIFF", 7, None, None),
        (
            "PROEFSLEUVEN_LABEL",
            7,
            Some(rgb(settings.label_color)),
            None,
        ),
    ];
    rows.into_iter()
        // ezdxf.layers.add supplies its own color/lineweight defaults over
        // those dxfattribs in the original helper. Match written resources
        // even when the source template contains no overview layers.
        .map(|(name, _color, true_color, _lineweight)| geometry::Layer {
            name: name.into(),
            color: 7,
            true_color,
            lineweight: Some(-3),
        })
        .collect()
}
pub fn export_bounds(request: &Request) -> Result<Bounds, String> {
    checked_bounds(request, &crate::input::NEVER_CANCEL)
}
fn checked_bounds(request: &Request, cancel: &AtomicBool) -> Result<Bounds, String> {
    if request.layers.is_empty() {
        return Err("Laad eerst minimaal een GeoTIFF-bestand".into());
    }
    if request.layers.len() > 10_000
        || request.linework.len() > 100_000
        || request.labels.len() > 150_000
        || !request.settings.label_gap.is_finite()
    {
        return Err("Kadastrale exportinvoer overschrijdt grenzen".into());
    }
    let mut metadata_bytes = 0usize;
    let mut source_bytes = 0u64;
    let mut points = 0usize;
    for group in &request.linework {
        crate::input::check(cancel)?;
        crate::object_dxf::layer_name(&group.layer_name)?;
        for path in &group.paths {
            points = points.saturating_add(path.len());
            if points > 2_000_000 {
                return Err("Te veel CAD-punten voor het kadastrale overzicht".into());
            }
            for (i, p) in path.iter().enumerate() {
                if i % 16384 == 0 {
                    crate::input::check(cancel)?;
                }
                if !p.0.is_finite() || !p.1.is_finite() {
                    return Err("Niet-eindige Kadastercoördinaten".into());
                }
            }
        }
    }
    let mut combined = None;
    for layer in &request.layers {
        crate::input::check(cancel)?;
        if !valid_bounds(layer.bounds) {
            return Err("Ongeldige GeoTIFF-grenzen".into());
        }
        if [
            layer.transform.a,
            layer.transform.b,
            layer.transform.c,
            layer.transform.d,
            layer.transform.e,
            layer.transform.f,
        ]
        .iter()
        .any(|v| !v.is_finite())
        {
            return Err("Ongeldige affine GeoTIFF-transformatie".into());
        }
        metadata_bytes = metadata_bytes
            .saturating_add(crate::raster_points::validate_metadata(&layer.metadata)?);
        if let Some(trench) = &layer.virtual_trench {
            trench.validate()?;
            let mut bytes = trench.point_metadata_bytes()?;
            // A virtual layer commonly carries the very same layer metadata
            // in both source representations. Count it once, while retaining
            // separate endpoint/object metadata and genuinely different maps.
            if layer.metadata == trench.metadata {
                bytes =
                    bytes.saturating_sub(crate::raster_points::validate_metadata(&layer.metadata)?);
            }
            metadata_bytes = metadata_bytes.saturating_add(bytes);
        }
        if metadata_bytes > 16 * 1024 * 1024 {
            return Err("Te veel laag- en puntmetadata voor het kadastrale overzicht".into());
        }
        if request.settings.include_tiff_images && layer.virtual_trench.is_none() {
            let size = fs::metadata(&layer.path)
                .map_err(|e| format!("TIFF niet bereikbaar: {e}"))?
                .len();
            source_bytes = source_bytes
                .checked_add(size)
                .ok_or("Te veel TIFF-brondata")?;
            if size == 0 || source_bytes > assets::MAX_BYTES {
                return Err("TIFF-bronnen overschrijden het exportbudget".into());
            }
        }
        let b = layer.bounds;
        combined = Some(combined.map_or(b, |a: Bounds| Bounds {
            min_x: a.min_x.min(b.min_x),
            min_y: a.min_y.min(b.min_y),
            max_x: a.max_x.max(b.max_x),
            max_y: a.max_y.max(b.max_y),
        }));
    }
    let b = combined.unwrap();
    let pad = ((b.max_x - b.min_x).max(b.max_y - b.min_y) * 0.35).clamp(60., 180.);
    Ok(Bounds {
        min_x: b.min_x - pad,
        min_y: b.min_y - pad,
        max_x: b.max_x + pad,
        max_y: b.max_y + pad,
    })
}
struct AffineImage {
    png: PathBuf,
    raster: PathBuf,
    transform: GeoTransform,
    height: u32,
}
/// Geometry is already appended; collect these images into the exporter's one
/// image batch, then finalize their full affine transforms before publication.
pub struct Plan {
    pub images: Vec<crate::template_images::Image>,
    assignments: Vec<AffineImage>,
    first_handle: u64,
}
impl Plan {
    pub fn image_count(&self) -> usize {
        self.assignments.len()
    }
    pub fn asset_bytes(&self) -> Result<u64, String> {
        self.assignments.iter().try_fold(0u64, |sum, a| {
            let n = fs::metadata(&a.raster).map_err(|e| e.to_string())?.len()
                + fs::metadata(&a.png).map_err(|e| e.to_string())?.len();
            sum.checked_add(n)
                .ok_or_else(|| "Te veel overzichtsrasterdata".into())
        })
    }
    pub fn finalize(&self, document: &Document, control: &Control) -> Result<Document, String> {
        let mut doc = document.indexed_copy(control.flag())?;
        let mut changes = Vec::new();
        let mut image_handles = Vec::new();
        for asset in &self.assignments {
            control.check()?;
            let definition = doc
                .records()
                .find(|r| r.kind() == "IMAGEDEF" && r.first(1) == asset.png.to_str())
                .ok_or("Overzichtsafbeeldingsdefinitie ontbreekt")?;
            let id = definition
                .first(5)
                .ok_or("Afbeeldingsdefinitie mist handle")?;
            let entity = doc
                .records()
                .find(|r| {
                    r.kind() == "IMAGE" && r.first(340).is_some_and(|h| h.eq_ignore_ascii_case(id))
                })
                .ok_or("Overzichtsafbeelding ontbreekt")?;
            image_handles.push(entity.handle().ok_or("Overzichtsafbeelding mist handle")?);
            let t = asset.transform;
            let p = t.pixel_to_world(0., asset.height as f64);
            for (code, value) in [
                (10, p.0),
                (20, p.1),
                (11, t.a),
                (21, t.d),
                (12, -t.b),
                (22, -t.e),
            ] {
                changes.push(doc.tag_change(entity.index(), code, &value.to_string())?);
            }
            changes.push(doc.tag_change(entity.index(), 8, "PROEFSLEUVEN_TIFF")?);
            changes.push(doc.tag_change(
                definition.index(),
                1,
                asset.raster.to_str().ok_or("Rasterpad is geen Unicode")?,
            )?);
        }
        doc.apply(changes)?;
        if !image_handles.is_empty() {
            doc = doc.indexed_copy(control.flag())?;
            let model = doc
                .records()
                .find(|r| r.kind() == "BLOCK_RECORD" && r.first(2) == Some("*Model_Space"))
                .and_then(|r| r.handle())
                .ok_or("Modelruimte ontbreekt")?;
            // The original overview paints TIFFs first, then Kadaster, trench
            // geometry and labels. Keep this order even when all overview and
            // paper/slot images were allocated together in the shared batch.
            let insertion = doc
                .records()
                .find(|r| {
                    r.section() == Some("ENTITIES")
                        && crate::template_copy::owner(*r) == Some(model)
                        && r.handle()
                            .is_some_and(|h| h >= self.first_handle && !image_handles.contains(&h))
                })
                .map(|r| doc.record_start(r.index()));
            if let Some(at) = insertion {
                let mut changes = Vec::new();
                let mut text = String::new();
                for handle in &image_handles {
                    let r = doc
                        .by_handle(&format!("{handle:X}"))?
                        .ok_or("Overzichtsafbeelding ontbreekt")?;
                    text.push_str(&crate::template_copy::encode(&crate::template_copy::tags(
                        r,
                    )));
                    changes.push(doc.record_change(r.index(), String::new()));
                }
                changes.push(Change {
                    start: at,
                    end: at,
                    text,
                });
                doc.apply(changes)?;
            }
        }
        for asset in &self.assignments {
            fs::remove_file(&asset.png).map_err(|e| e.to_string())?;
        }
        Ok(doc)
    }
}
pub fn populate(
    doc: &mut Document,
    request: &Request,
    tx: &Transaction,
    control: &Control,
    mut progress: impl FnMut(&str),
) -> Result<Plan, String> {
    control.check()?;
    checked_bounds(request, control.flag())?;
    let mut budget = assets::Budget::new();
    let mut shapes = Vec::new();
    let mut bounds = Vec::new();
    let mut images = Vec::new();
    let mut image_paths = Vec::new();
    let mut copies = Vec::new();
    for (index, layer) in request.layers.iter().enumerate() {
        control.check()?;
        if request.settings.trench_mode == TrenchMode::None && !request.settings.include_tiff_images
        {
            continue;
        }
        progress(&format!("Proefsleuf {} voorbereiden", index + 1));
        // Virtual axes/boundaries are already measured source geometry. Do not
        // render or allocate a raster merely to retrieve those coordinates.
        let prepared = if request.settings.include_tiff_images || layer.virtual_trench.is_none() {
            Some(prepare(layer, control.flag())?)
        } else {
            None
        };
        if request.settings.trench_mode != TrenchMode::None {
            let shape = if let Some(t) = &layer.virtual_trench {
                if request.settings.trench_mode == TrenchMode::Centerline {
                    vec![[t.start.x, t.start.y], [t.end.x, t.end.y]]
                } else {
                    let mut boundary = t.measured_boundary();
                    if boundary.first() == boundary.last() {
                        boundary.pop();
                    }
                    boundary.into_iter().map(|(x, y)| [x, y]).collect()
                }
            } else {
                let p = prepared.as_ref().ok_or("Overzichtsraster ontbreekt")?;
                if request.settings.trench_mode == TrenchMode::Centerline {
                    mask::centerline(&p.image, p.tx, p.bounds, &request.linework, control.flag())?
                } else {
                    mask::polygon(&p.image, p.tx, p.bounds, control.flag())?
                }
            };
            bounds.push(points_bounds(&shape)?);
            shapes.push(shape);
        }
        if request.settings.include_tiff_images {
            let p = prepared.as_ref().ok_or("Overzichtsraster ontbreekt")?;
            let png = tx.asset_path(&format!("overview{:05}.png", index + 1))?;
            let encoded = crate::template_raster::save_image_bounded(
                &p.image,
                &png,
                control.flag(),
                budget.remaining(),
            )?;
            budget.consume(encoded)?;
            let suffix = layer
                .path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("tiff");
            let name = format!(
                "{}__slot{:03}.{suffix}",
                basename(&layer.path, index + 1),
                index + 1
            );
            let raster = tx.asset_path(&name)?;
            if layer.virtual_trench.is_some() {
                budget.virtual_tiff(&p.image, &raster, control)?;
            } else {
                budget.copy(&layer.path, &raster, control)?;
            }
            images.push(crate::template_images::Image {
                path: png.clone(),
                bounds: Bounds {
                    min_x: 0.,
                    min_y: 0.,
                    max_x: p.image.width() as f64,
                    max_y: p.image.height() as f64,
                },
                definition_name: name,
                inset: 0.,
                preserve_aspect: false,
                layout: None,
            });
            image_paths.push((png, raster));
            copies.push((p.tx, p.image.height()));
        }
    }
    let mut entities = Vec::new();
    for group in &request.linework {
        crate::object_dxf::layer_name(&group.layer_name)?;
        let mut paint = Paint {
            color: Some(8),
            lineweight: Some(13),
            ..Default::default()
        };
        match group.layer_name.as_str() {
            "BGT_WEGDEEL" => {
                paint.true_color = Some(rgb([145; 3]));
                paint.lineweight = Some(9);
            }
            "BGT_VECTOR_OMTREK" => {
                paint.true_color = Some(rgb([155; 3]));
                paint.lineweight = Some(9);
            }
            _ => {}
        }
        for path in &group.paths {
            if path.len() >= 2 {
                entities.push(polyline(
                    &group.layer_name,
                    path.iter().map(|p| [p.0, p.1]).collect(),
                    paint.clone(),
                ));
            }
        }
    }
    for label in &request.labels {
        entities.push(text(
            &label.layer_name,
            label.text.clone(),
            [label.position.0, label.position.1],
            1.,
            (-label.rotation).rem_euclid(360.),
            "KADASTRALE_LABELS",
            Paint {
                color: Some(8),
                ..Default::default()
            },
        ));
    }
    let before_trenches = entities.len();
    let mut hatch_specs = Vec::new();
    for shape in &shapes {
        if request.settings.trench_mode == TrenchMode::Centerline {
            entities.push(polyline(
                "PROEFSLEUVEN_HARTLIJN",
                shape.clone(),
                Paint {
                    color: Some(7),
                    true_color: Some(rgb(request.settings.centerline_color)),
                    ..Default::default()
                },
            ));
        } else {
            if !request.settings.include_tiff_images {
                hatch_specs.push((entities.len(), shape));
            }
            let mut vertices = shape.clone();
            vertices.push(shape[0]);
            entities.push(polyline(
                "PROEFSLEUVEN_OMTREK",
                vertices,
                Paint {
                    color: Some(7),
                    true_color: Some(rgb([120; 3])),
                    lineweight: Some(25),
                },
            ));
        }
    }
    let mut placed = Vec::new();
    for (i, layer) in request.layers.iter().enumerate().take(shapes.len()) {
        let value = label(layer, i + 1);
        let center = crate::object_layout::label_center(
            i,
            &value,
            &bounds,
            &placed,
            request.settings.label_gap.max(0.),
        )?;
        placed.push(crate::object_layout::label_bounds(&value, center));
        entities.push(text(
            "PROEFSLEUVEN_LABEL",
            value,
            center,
            6.,
            0.,
            "PROEFSLEUVEN_LIBMONO",
            Paint {
                color: Some(7),
                true_color: Some(rgb(request.settings.label_color)),
                ..Default::default()
            },
        ));
    }
    let mut extra_layers = overview_layers(&request.settings);
    for e in &entities {
        if !doc.has_layer(&e.layer)
            && !extra_layers
                .iter()
                .any(|l: &geometry::Layer| l.name == e.layer)
        {
            extra_layers.push(geometry::Layer {
                name: e.layer.clone(),
                color: 8,
                true_color: None,
                lineweight: None,
            });
        }
    }
    let first_handle = doc.seed()?.0;
    let handles = geometry::add(doc, &extra_layers, &entities, control.flag())?;
    *doc = doc.indexed_copy(control.flag())?;
    let mut changes = Vec::new();
    if request.settings.trench_mode == TrenchMode::Centerline {
        for h in handles.iter().skip(before_trenches).take(shapes.len()) {
            let r = doc.by_handle(h)?.ok_or("Hartlijn ontbreekt")?;
            let mut tags = crate::template_copy::tags(r);
            crate::template_copy::scope_set(&mut tags, "AcDbPolyline", 43, "0.6".into())?;
            changes.push(doc.record_change(r.index(), crate::template_copy::encode(&tags)));
        }
    }
    if !hatch_specs.is_empty() {
        let model = doc
            .records()
            .find(|r| r.kind() == "BLOCK_RECORD" && r.first(2) == Some("*Model_Space"))
            .and_then(|r| r.handle())
            .ok_or("Modelruimte ontbreekt")?;
        let (mut next, mut seed) = doc.seed()?;
        for (index, polygon) in hatch_specs {
            let h = &handles[index];
            let r = doc.by_handle(h)?.ok_or("Proefsleufomtrek ontbreekt")?;
            let at = doc.record_start(r.index());
            changes.push(Change {
                start: at,
                end: at,
                text: hatch_text(next, model, polygon),
            });
            next = next.checked_add(1).ok_or("Geen vrije DXF-handles")?;
        }
        seed.text = format!("{next:X}");
        changes.push(seed);
    }
    doc.apply(changes)?;
    Ok(Plan {
        images,
        first_handle,
        assignments: image_paths
            .into_iter()
            .zip(copies)
            .map(|((png, raster), (transform, height))| AffineImage {
                png,
                raster,
                transform,
                height,
            })
            .collect(),
    })
}

pub fn export(output: &Path, request: &Request, control: &Control) -> Result<ResultInfo, String> {
    export_with_progress(output, request, control, |_| {})
}
pub fn export_with_progress(
    output: &Path,
    request: &Request,
    control: &Control,
    mut progress: impl FnMut(&str),
) -> Result<ResultInfo, String> {
    control.check()?;
    protect(output, request)?;
    let export_bounds = export_bounds(request)?;
    let tx = Transaction::new(output, control)?;
    let mut doc = Document::from_bytes(BASE.as_bytes().to_vec(), control.flag())?;
    header(&mut doc, export_bounds, &request.settings)?;
    let mut plan = populate(&mut doc, request, &tx, control, &mut progress)?;
    let image_count = plan.image_count();
    crate::template_images::add_images(&mut doc, &plan.images, control.flag())?;
    plan.images.clear();
    doc = plan.finalize(&doc, control)?;
    let internal_layer = doc
        .records()
        .find(|r| r.kind() == "LAYER" && r.first(2) == Some("PROEFSLEUF_TEMPLATE_IMAGES"))
        .map(|r| r.index());
    if let Some(layer) = internal_layer {
        let mut changes = vec![doc.record_change(layer, String::new())];
        if let Some(table) = doc
            .records()
            .find(|r| r.kind() == "TABLE" && r.first(2) == Some("LAYER"))
        {
            changes.push(doc.tag_change(
                table.index(),
                70,
                &(doc.layer_count() - 1).to_string(),
            )?);
        }
        doc.apply(changes)?;
    }
    progress("Kadastraal overzicht definitief opslaan");
    control.check()?;
    crate::storage::atomic_write(tx.staged_drawing(), false, |file| {
        doc.write_to(file, control.flag())
    })?;
    protect(output, request)?;
    let published = tx.publish(control)?;
    Ok(ResultInfo {
        published,
        layers: request.layers.len(),
        images: image_count,
        export_bounds,
    })
}
