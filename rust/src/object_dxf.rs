//! Streaming R2018 object export with Python-referenced default rules and layout.
//! Template/profile drawings remain a separate workflow.
use crate::{
    bgt::{BgtKind, BgtLayer},
    cadastral::CadastralLayer,
    cross_section::Marker,
    disciplines::{matching_rule, Rule},
    model::{Bounds, Dataset, Feature, Geometry, Vertex},
    object_layout as layout,
    virtual_trench::VirtualTrench,
};
pub use layout::job_label;
use std::{
    collections::BTreeMap,
    io::{self, BufWriter, Write},
    path::Path,
};
mod handles {
    include!("object_dxf_handles.rs");
}
const BASE: &str = include_str!("object_dxf_base.txt");
const LABEL_LAYER: &str = "PROEFSLEUVEN_LABEL";
const LABEL_STYLE: &str = "PROEFSLEUVEN_LIBMONO";
const APP_ID: &str = "KTM_DXF_EXPORT";
const MAX_POINTS: usize = 10_000_000;

#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    pub point_half_length: f64,
    pub label_gap: f64,
    pub max_bytes: u64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            point_half_length: 0.5,
            label_gap: 6.,
            max_bytes: 256 * 1024 * 1024,
        }
    }
}
impl Options {
    fn checked(self) -> Result<Self, String> {
        if !self.point_half_length.is_finite() || !self.label_gap.is_finite() {
            return Err("Ongeldige DXF-lijnlengte of labelafstand".into());
        }
        Ok(Self {
            point_half_length: self.point_half_length.max(0.01),
            label_gap: self.label_gap.max(0.),
            max_bytes: self.max_bytes.min(256 * 1024 * 1024),
        })
    }
}
fn text(value: &str, limit: usize, description: &str) -> Result<(), String> {
    if value.len() > limit || value.chars().any(char::is_control) {
        return Err(format!(
            "{description} is te lang of bevat ongeldige besturingstekens"
        ));
    }
    Ok(())
}
fn layer_name(name: &str) -> Result<(), String> {
    text(name, 255, "DXF-laagnaam")?;
    if name.is_empty() || name.chars().any(|c| "<>/\\\":;?*|=".contains(c)) {
        return Err("Ongeldige DXF-laagnaam".into());
    }
    Ok(())
}
fn finite(p: Vertex) -> Result<(), String> {
    if !p.x.is_finite() || !p.y.is_finite() || p.z.is_some_and(|v| !v.is_finite()) {
        return Err("DXF bevat niet-eindige coördinaten".into());
    }
    Ok(())
}
#[derive(Clone, Copy)]
struct Extents {
    min: [f64; 3],
    max: [f64; 3],
}
impl Extents {
    fn add(bounds: &mut Option<Self>, p: Vertex) -> Result<(), String> {
        finite(p)?;
        let xyz = [p.x, p.y, p.z.unwrap_or(0.)];
        if let Some(b) = bounds {
            for (i, v) in xyz.into_iter().enumerate() {
                b.min[i] = b.min[i].min(v);
                b.max[i] = b.max[i].max(v);
            }
        } else {
            *bounds = Some(Self { min: xyz, max: xyz });
        }
        Ok(())
    }
    fn xy(self) -> Bounds {
        Bounds {
            min_x: self.min[0],
            min_y: self.min[1],
            max_x: self.max[0],
            max_y: self.max[1],
        }
    }
}
fn add_point(used: &mut usize, bounds: &mut Option<Extents>, p: Vertex) -> Result<(), String> {
    *used = used
        .checked_add(1)
        .filter(|v| *v <= MAX_POINTS)
        .ok_or("DXF overschrijdt het puntbudget")?;
    Extents::add(bounds, p)
}
struct Layer {
    name: String,
    color: u16,
    true_color: Option<u32>,
}
struct Job<'a> {
    dataset: &'a Dataset,
    delta: [f64; 2],
    bounds: Bounds,
    label: String,
    center: [f64; 2],
}
struct Plan<'a> {
    jobs: Vec<Job<'a>>,
    layers: BTreeMap<String, Layer>,
    bounds: Extents,
    markers: Vec<Vec<Marker>>,
    max_bytes: u64,
}
fn style<'a>(feature: &Feature, rules: &'a [Rule]) -> (&'a str, u16) {
    matching_rule(&feature.source_name, rules).map_or(("0", 256), |r| {
        (r.target_layer.trim(), r.color_aci.clamp(1, 256))
    })
}
fn ensure(layers: &mut BTreeMap<String, Layer>, name: &str, color: u16) -> Result<(), String> {
    layer_name(name)?;
    if name == "0" {
        return Ok(());
    }
    let color = if color == 256 { 7 } else { color.clamp(1, 255) };
    // Exact repeated spellings do not allocate a normalized key per object.
    if let Some(layer) = layers.get_mut(name) {
        layer.color = color;
        return Ok(());
    }
    let folded = name.to_lowercase();
    let existing = layers
        .keys()
        .find(|key| key.to_lowercase() == folded)
        .cloned();
    let layer = layers
        .entry(existing.unwrap_or_else(|| name.into()))
        .or_insert_with(|| Layer {
            name: name.into(),
            color: 7,
            true_color: None,
        });
    layer.color = color;
    Ok(())
}
#[derive(Clone, Copy)]
struct Metadata<'a> {
    dataset: &'a Dataset,
    feature: &'a Feature,
    layer: &'a str,
}
impl Metadata<'_> {
    fn coding(&self) -> &str {
        if self.feature.source_name.is_empty() {
            "0"
        } else {
            &self.feature.source_name
        }
    }
    fn validate(self) -> Result<(), String> {
        for (prefix, value) in [
            (5, self.dataset.title.as_str()),
            (8, self.feature.object_name.as_str()),
            (8, self.coding()),
            (7, self.layer),
        ] {
            text(value, 255 - prefix, "DXF-XDATA")?;
        }
        Ok(())
    }
}
fn boundary(trench: &VirtualTrench) -> impl Iterator<Item = Vertex> + '_ {
    let points = &trench.boundary_3d;
    let end = if points.len() > 1 && points.first() == points.last() {
        points.len() - 1
    } else {
        points.len()
    };
    points[..end].iter().copied().chain(
        points
            .is_empty()
            .then(|| trench.polygon())
            .into_iter()
            .flatten()
            .map(|(x, y)| Vertex { x, y, z: None }),
    )
}
impl<'a> Plan<'a> {
    fn new(
        datasets: impl Iterator<Item = &'a Dataset>,
        trenches: &[VirtualTrench],
        cad: Option<&CadastralLayer>,
        bgt: &[BgtLayer],
        rules: &[Rule],
        options: Options,
    ) -> Result<Self, String> {
        let mut jobs = Vec::new();
        let mut layers = BTreeMap::new();
        let mut bounds = None;
        let mut used = 0usize;
        for dataset in datasets.filter(|d| !d.features.is_empty()) {
            if jobs.is_empty() {
                ensure(&mut layers, LABEL_LAYER, 7)?;
                layers.get_mut(LABEL_LAYER).unwrap().true_color = Some(0xffffff);
            }
            let delta = layout::point_delta(dataset, options.point_half_length);
            let mut own = None;
            // Python emits all points before all polylines, even if the source
            // JSON interleaves them. Direction depends only on point order.
            for points_only in [true, false] {
                for feature in &dataset.features {
                    if matches!(feature.geometry, Geometry::Point(_)) != points_only {
                        continue;
                    }
                    let (name, color) = style(feature, rules);
                    ensure(&mut layers, name, color)?;
                    Metadata {
                        dataset,
                        feature,
                        layer: name,
                    }
                    .validate()?;
                    let mut add = |p: Vertex| -> Result<(), String> {
                        add_point(&mut used, &mut bounds, p)?;
                        Extents::add(&mut own, p)
                    };
                    match &feature.geometry {
                        Geometry::Point(p) => {
                            for end in layout::ends(*p, delta) {
                                add(end)?;
                            }
                        }
                        Geometry::Polyline(points) => {
                            if points.len() < 2 {
                                return Err("Objectpolyline heeft minder dan twee punten".into());
                            }
                            for p in points {
                                add(*p)?;
                            }
                        }
                    }
                }
            }
            let own = own.ok_or("Geen DXF-objectgeometrie")?;
            jobs.push(Job {
                dataset,
                delta,
                bounds: own.xy(),
                label: job_label(dataset, jobs.len() + 1),
                center: [0.; 2],
            });
        }
        if !jobs.is_empty() {
            let job_bounds: Vec<_> = jobs.iter().map(|j| j.bounds).collect();
            let mut placed = Vec::with_capacity(jobs.len());
            for (i, job) in jobs.iter_mut().enumerate() {
                text(&job.label, 2049, "DXF-joblabel")?;
                job.center =
                    layout::label_center(i, &job.label, &job_bounds, &placed, options.label_gap)?;
                placed.push(layout::label_bounds(&job.label, job.center));
            }
        }
        let mut markers = Vec::with_capacity(trenches.len());
        for trench in trenches {
            trench.validate()?;
            text(&trench.label, 2049, "DXF-proefsleuflabel")?;
            for name in [
                "VIRTUAL_TRENCH_3D",
                "VIRTUAL_TRENCH_AXIS",
                "VIRTUAL_TRENCH_MARKER",
                "VIRTUAL_TRENCH_LABEL",
            ] {
                ensure(
                    &mut layers,
                    name,
                    if name == "VIRTUAL_TRENCH_AXIS" { 30 } else { 7 },
                )?;
            }
            for p in boundary(trench) {
                add_point(&mut used, &mut bounds, p)?;
            }
            if boundary(trench).count() < 3 {
                return Err("MarXact-grens heeft minder dan drie hoekpunten".into());
            }
            add_point(&mut used, &mut bounds, trench.start)?;
            add_point(&mut used, &mut bounds, trench.end)?;
            let ready = crate::cross_section::markers(trench);
            for marker in &ready {
                add_point(&mut used, &mut bounds, marker.start)?;
                add_point(&mut used, &mut bounds, marker.end)?;
            }
            markers.push(ready);
        }
        if let Some(cad) = cad.filter(|c| c.visible) {
            for linework in &cad.linework {
                ensure(&mut layers, &linework.layer_name, 7)?;
                for path in &linework.paths {
                    for &(x, y) in path {
                        add_point(&mut used, &mut bounds, Vertex { x, y, z: None })?;
                    }
                }
            }
            for label in &cad.labels {
                ensure(&mut layers, &label.layer_name, 7)?;
                text(&label.text, 2049, "DXF-kaartlabel")?;
                if !label.rotation.is_finite() {
                    return Err("Ongeldige DXF-teksthoek".into());
                }
                add_point(
                    &mut used,
                    &mut bounds,
                    Vertex {
                        x: label.position.0,
                        y: label.position.1,
                        z: None,
                    },
                )?;
            }
        }
        for collection in bgt.iter().filter(|c| c.visible) {
            let (layer, color) = bgt_style(collection);
            ensure(&mut layers, layer, color)?;
            for path in &collection.paths {
                for &(x, y) in path {
                    add_point(&mut used, &mut bounds, Vertex { x, y, z: None })?;
                }
            }
        }
        if used > MAX_POINTS {
            return Err("DXF overschrijdt het puntbudget".into());
        }
        Ok(Self {
            jobs,
            layers,
            bounds: bounds.ok_or("Geen geometrie om naar DXF te exporteren")?,
            markers,
            max_bytes: options.max_bytes,
        })
    }
}
fn bgt_style(layer: &BgtLayer) -> (&'static str, u16) {
    match layer.kind {
        BgtKind::Wegdeel => ("BGT_WEGDEEL", 8),
        BgtKind::OnbegroeidTerreindeel => ("BGT_TERREIN", 3),
    }
}

struct Limit<W> {
    out: W,
    written: u64,
    max: u64,
}
impl<W: Write> Write for Limit<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.max.saturating_sub(self.written) {
            return Err(io::Error::other("DXF overschrijdt de uitvoerlimiet"));
        }
        let n = self.out.write(bytes)?;
        self.written += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}
struct Drawing<W> {
    out: W,
    next: u32,
    entities: usize,
}
impl<W: Write> Drawing<W> {
    fn tag(&mut self, code: i32, value: impl std::fmt::Display) -> Result<(), String> {
        writeln!(self.out, "{code}\n{value}").map_err(|e| e.to_string())
    }
    fn point(&mut self, code: i32, p: Vertex) -> Result<(), String> {
        finite(p)?;
        self.tag(code, p.x)?;
        self.tag(code + 10, p.y)?;
        self.tag(code + 20, p.z.unwrap_or(0.))
    }
    fn handle(&mut self) -> Result<u32, String> {
        let h = self.next;
        self.next = self.next.checked_add(1).ok_or("Te veel DXF-handles")?;
        Ok(h)
    }
    fn entity(
        &mut self,
        kind: &str,
        owner: impl std::fmt::Display,
        layer: &str,
        color: u16,
    ) -> Result<u32, String> {
        self.entities += 1;
        if self.entities > 2_000_000 {
            return Err("DXF overschrijdt het entiteitenbudget".into());
        }
        let handle = self.handle()?;
        self.tag(0, kind)?;
        self.tag(5, format_args!("{handle:X}"))?;
        self.tag(330, owner)?;
        self.tag(100, "AcDbEntity")?;
        self.tag(8, layer)?;
        self.tag(62, color)?;
        Ok(handle)
    }
    fn line(&mut self, layer: &str, color: u16, a: Vertex, b: Vertex) -> Result<(), String> {
        self.entity("LINE", handles::MODEL_SPACE, layer, color)?;
        self.tag(100, "AcDbLine")?;
        self.point(10, a)?;
        self.point(11, b)
    }
    fn polyline(
        &mut self,
        layer: &str,
        color: u16,
        points: impl Iterator<Item = Vertex>,
        closed: bool,
        data: Option<Metadata<'_>>,
    ) -> Result<(), String> {
        let owner = self.entity("POLYLINE", handles::MODEL_SPACE, layer, color)?;
        self.tag(100, "AcDb3dPolyline")?;
        self.point(
            10,
            Vertex {
                x: 0.,
                y: 0.,
                z: None,
            },
        )?;
        self.tag(70, if closed { 9 } else { 8 })?;
        if let Some(data) = data {
            self.xdata(data)?;
        }
        for p in points {
            self.entity("VERTEX", format_args!("{owner:X}"), layer, color)?;
            self.tag(100, "AcDbVertex")?;
            self.tag(100, "AcDb3dPolylineVertex")?;
            self.point(10, p)?;
            self.tag(70, 32)?;
        }
        self.entity("SEQEND", format_args!("{owner:X}"), layer, color)?;
        Ok(())
    }
    fn xdata(&mut self, value: Metadata<'_>) -> Result<(), String> {
        self.tag(1001, APP_ID)?;
        self.tag(1000, format_args!("Job: {}", value.dataset.title))?;
        self.tag(1000, format_args!("JobId: {}", value.dataset.job_id))?;
        self.tag(1000, format_args!("Object: {}", value.feature.object_name))?;
        self.tag(1000, format_args!("Coding: {}", value.coding()))?;
        self.tag(1000, format_args!("Layer: {}", value.layer))?;
        Ok(())
    }
    fn label(
        &mut self,
        layer: &str,
        value: &str,
        center: [f64; 2],
        height: f64,
        rotation: f64,
        job: bool,
    ) -> Result<(), String> {
        self.entity("TEXT", handles::MODEL_SPACE, layer, 7)?;
        if job {
            self.tag(420, 0xffffffu32)?;
        }
        self.tag(100, "AcDbText")?;
        let p = Vertex {
            x: center[0],
            y: center[1],
            z: None,
        };
        self.point(10, p)?;
        self.tag(40, height)?;
        self.tag(1, value)?;
        self.tag(50, rotation)?;
        self.tag(7, if job { LABEL_STYLE } else { "Standard" })?;
        self.tag(72, i32::from(job))?;
        if job {
            self.point(11, p)?;
        }
        self.tag(100, "AcDbText")?;
        self.tag(73, if job { 2 } else { 0 })
    }
    fn fragment(&mut self, fragment: &str, plan: &Plan) -> Result<(), String> {
        for line in fragment.lines() {
            match line {
                "__SB_EXTMIN_X__" => writeln!(self.out, "{}", plan.bounds.min[0]),
                "__SB_EXTMIN_Y__" => writeln!(self.out, "{}", plan.bounds.min[1]),
                "__SB_EXTMIN_Z__" => writeln!(self.out, "{}", plan.bounds.min[2]),
                "__SB_EXTMAX_X__" => writeln!(self.out, "{}", plan.bounds.max[0]),
                "__SB_EXTMAX_Y__" => writeln!(self.out, "{}", plan.bounds.max[1]),
                "__SB_EXTMAX_Z__" => writeln!(self.out, "{}", plan.bounds.max[2]),
                "__SB_LAYER_COUNT__" => writeln!(self.out, "{}", plan.layers.len() + 1),
                _ => writeln!(self.out, "{line}"),
            }
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
/// Encode the same complete document to any sink. The public file workflow
/// below stages it atomically; this entry also supports byte-identical benchmarks.
pub fn encode(
    out: impl Write,
    datasets: &[Dataset],
    trenches: &[VirtualTrench],
    cad: Option<&CadastralLayer>,
    bgt: &[BgtLayer],
    rules: &[Rule],
    options: Options,
) -> Result<(), String> {
    encode_iter(out, datasets.iter(), trenches, cad, bgt, rules, options)
}
/// Borrow several dataset collections without copying their objects.
pub fn encode_iter<'a>(
    out: impl Write,
    datasets: impl Iterator<Item = &'a Dataset>,
    trenches: &[VirtualTrench],
    cad: Option<&CadastralLayer>,
    bgt: &[BgtLayer],
    rules: &[Rule],
    options: Options,
) -> Result<(), String> {
    let options = options.checked()?;
    let plan = Plan::new(datasets, trenches, cad, bgt, rules, options)?;
    encode_plan(out, &plan, trenches, cad, bgt, rules, BASE)
}
fn envelope_slots(base: &str) -> (&str, &str, &str) {
    fn split<'a>(source: &'a str, marker: &str) -> (&'a str, &'a str) {
        let (before, after) = source.split_once(marker).expect("reviewed envelope slot");
        let after = after
            .strip_prefix("\r\n")
            .or_else(|| after.strip_prefix('\n'))
            .expect("envelope slot occupies a line");
        (before, after)
    }
    let (head, rest) = split(base, "__SB_LAYERS__");
    let (middle, tail) = split(rest, "__SB_ENTITIES__");
    (head, middle, tail)
}
fn encode_plan(
    out: impl Write,
    plan: &Plan<'_>,
    trenches: &[VirtualTrench],
    cad: Option<&CadastralLayer>,
    bgt: &[BgtLayer],
    rules: &[Rule],
    base: &str,
) -> Result<(), String> {
    let limit = Limit {
        out,
        written: 0,
        max: plan.max_bytes,
    };
    let mut draw = Drawing {
        out: BufWriter::with_capacity(64 * 1024, limit),
        next: 0x1000,
        entities: 0,
    };
    let (head, middle, tail) = envelope_slots(base);
    draw.fragment(head, plan)?;
    for layer in plan.layers.values() {
        let handle = draw.handle()?;
        draw.tag(0, "LAYER")?;
        draw.tag(5, format_args!("{handle:X}"))?;
        draw.tag(330, handles::LAYER_TABLE)?;
        draw.tag(100, "AcDbSymbolTableRecord")?;
        draw.tag(100, "AcDbLayerTableRecord")?;
        draw.tag(2, &layer.name)?;
        draw.tag(70, 0)?;
        draw.tag(62, layer.color)?;
        draw.tag(6, "Continuous")?;
        draw.tag(
            290,
            i32::from(!layer.name.eq_ignore_ascii_case("Defpoints")),
        )?;
        if let Some(color) = layer.true_color {
            draw.tag(420, color)?;
        }
    }
    draw.fragment(middle, plan)?;
    for job in &plan.jobs {
        for point_only in [true, false] {
            for feature in &job.dataset.features {
                if matches!(feature.geometry, Geometry::Point(_)) != point_only {
                    continue;
                }
                let (layer, color) = style(feature, rules);
                let values = Metadata {
                    dataset: job.dataset,
                    feature,
                    layer,
                };
                match &feature.geometry {
                    Geometry::Point(p) => {
                        let [a, b] = layout::ends(*p, job.delta);
                        draw.line(layer, color, a, b)?;
                        draw.xdata(values)?;
                    }
                    Geometry::Polyline(points) => {
                        draw.polyline(layer, color, points.iter().copied(), false, Some(values))?
                    }
                }
            }
        }
    }
    for job in &plan.jobs {
        draw.label(LABEL_LAYER, &job.label, job.center, 6., 0., true)?;
    }
    for (index, trench) in trenches.iter().enumerate() {
        draw.polyline("VIRTUAL_TRENCH_3D", 7, boundary(trench), true, None)?;
        draw.line("VIRTUAL_TRENCH_AXIS", 30, trench.start, trench.end)?;
        for marker in &plan.markers[index] {
            let feature = &trench.objects[marker.object_index];
            let color =
                matching_rule(&feature.source_name, rules).map_or(7, |r| r.color_aci.clamp(1, 256));
            draw.line("VIRTUAL_TRENCH_MARKER", color, marker.start, marker.end)?;
        }
        if !trench.label.is_empty() {
            draw.label(
                "VIRTUAL_TRENCH_LABEL",
                &trench.label,
                [trench.start.x, trench.start.y],
                0.3,
                0.,
                false,
            )?;
        }
    }
    if let Some(cad) = cad.filter(|c| c.visible) {
        for collection in &cad.linework {
            for path in &collection.paths {
                for pair in path.windows(2) {
                    draw.line(
                        &collection.layer_name,
                        7,
                        Vertex {
                            x: pair[0].0,
                            y: pair[0].1,
                            z: None,
                        },
                        Vertex {
                            x: pair[1].0,
                            y: pair[1].1,
                            z: None,
                        },
                    )?;
                }
            }
        }
        for label in &cad.labels {
            if !label.text.is_empty() {
                draw.label(
                    &label.layer_name,
                    &label.text,
                    [label.position.0, label.position.1],
                    0.6,
                    label.rotation,
                    false,
                )?;
            }
        }
    }
    for collection in bgt.iter().filter(|c| c.visible) {
        let (layer, color) = bgt_style(collection);
        for path in &collection.paths {
            for pair in path.windows(2) {
                draw.line(
                    layer,
                    color,
                    Vertex {
                        x: pair[0].0,
                        y: pair[0].1,
                        z: None,
                    },
                    Vertex {
                        x: pair[1].0,
                        y: pair[1].1,
                        z: None,
                    },
                )?;
            }
        }
    }
    if draw.entities == 0 {
        return Err("Geen DXF-entiteiten om te exporteren".into());
    }
    draw.fragment(tail, plan)?;
    draw.out.flush().map_err(|e| e.to_string())
}
pub fn write(
    path: &Path,
    datasets: &[Dataset],
    rules: &[Rule],
    options: Options,
) -> Result<(), String> {
    write_with_layers(path, datasets, &[], None, &[], rules, options)
}
pub fn write_with_layers(
    path: &Path,
    datasets: &[Dataset],
    trenches: &[VirtualTrench],
    cad: Option<&CadastralLayer>,
    bgt: &[BgtLayer],
    rules: &[Rule],
    options: Options,
) -> Result<(), String> {
    write_with_layers_iter(path, datasets.iter(), trenches, cad, bgt, rules, options)
}
/// GUI/combined exports can borrow loaded jobs and separately prepared virtual jobs.
pub fn write_with_layers_iter<'a>(
    path: &Path,
    datasets: impl Iterator<Item = &'a Dataset>,
    trenches: &[VirtualTrench],
    cad: Option<&CadastralLayer>,
    bgt: &[BgtLayer],
    rules: &[Rule],
    options: Options,
) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "Bestand bestaat al: {}. Kies een nieuwe naam.",
            path.display()
        ));
    }
    crate::storage::atomic_write(path, false, |file| {
        encode_iter(file, datasets, trenches, cad, bgt, rules, options)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_windows_checkout_of_the_compiled_envelope_encodes_the_same_complete_document() {
        let data = Dataset {
            job_id: 1,
            title: "PS1".into(),
            source_path: "jobFeatures.json".into(),
            features: vec![Feature {
                object_name: "Water".into(),
                source_name: "water".into(),
                geometry: Geometry::Point(Vertex {
                    x: 1.,
                    y: 2.,
                    z: Some(3.),
                }),
                attributes: Default::default(),
            }],
        };
        let plan = Plan::new(
            std::iter::once(&data),
            &[],
            None,
            &[],
            crate::RULES,
            Default::default(),
        )
        .unwrap();
        let lf = BASE.replace("\r\n", "\n");
        let crlf = lf.replace('\n', "\r\n");
        let mut normal = Vec::new();
        let mut windows = Vec::new();
        encode_plan(&mut normal, &plan, &[], None, &[], crate::RULES, &lf).unwrap();
        encode_plan(&mut windows, &plan, &[], None, &[], crate::RULES, &crlf).unwrap();
        assert_eq!(normal, windows);
        let doc = crate::dxf_read::parse_dxf_bytes(&windows, Path::new("windows.dxf")).unwrap();
        assert_eq!(doc.features.len(), 1);
        assert_eq!(doc.features[0].entity_type, "LINE");
        assert!(std::str::from_utf8(&windows).unwrap().ends_with("0\nEOF\n"));
    }
}
