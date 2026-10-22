//! Durable raw RD start/reference points and their rotated map positions.
use crate::{geotiff::GeoTiffInfo, profile, python_values, Dataset, Geometry, Rule, Vertex};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashSet, io::Write, path::PathBuf, sync::atomic::AtomicBool};
pub const REFERENCE: &str = "template_reference_point";
pub const START: &str = "template_cross_section_start_point";
pub type Context = (
    crate::profile_context::GeometryPaths,
    crate::profile_context::GeometryPaths,
);
const MAX_METADATA: usize = 2 * 1024 * 1024;
struct Counter(usize);
impl Write for Counter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0 = self.0.saturating_add(b.len());
        if self.0 > MAX_METADATA {
            return Err(std::io::Error::other("Laagmetadata overschrijdt 2 MiB"));
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub fn validate_metadata(value: &Value) -> Result<usize, String> {
    if !value.is_null() && !value.is_object() {
        return Err("Laagmetadata moet een object zijn".into());
    }
    let mut count = Counter(0);
    serde_json::to_writer(&mut count, value).map_err(|e| e.to_string())?;
    for key in [REFERENCE, START] {
        if let Some(v) = value.get(key) {
            // Older malformed values are preserved, never displayed as invented points.
            if let Some(p) = point(v) {
                check(p)?;
            }
        }
    }
    if let Some(angle) = python_values::number(value.get("rotation_degrees")) {
        if !angle.is_finite() {
            return Err("Niet-eindige rasterrotatie".into());
        }
    }
    Ok(count.0)
}
fn check(p: [f64; 2]) -> Result<(), String> {
    if p.iter().any(|v| !v.is_finite() || v.abs() > 1e12) {
        Err("Ongeldige RD-puntcoördinaten".into())
    } else {
        Ok(())
    }
}
pub fn point(v: &Value) -> Option<[f64; 2]> {
    if !v.is_object() {
        return None;
    }
    let p = [
        python_values::number(v.get("x"))?,
        python_values::number(v.get("y"))?,
    ];
    check(p).ok()?;
    Some(p)
}
pub fn stored(raster: &GeoTiffInfo, key: &str) -> Option<[f64; 2]> {
    raster.metadata.get(key).and_then(point)
}
pub fn center(raster: &GeoTiffInfo) -> [f64; 2] {
    let p = raster.transform.pixel_to_world(
        f64::from(raster.width) * 0.5,
        f64::from(raster.height) * 0.5,
    );
    [p.0, p.1]
}
fn rotate(raster: &GeoTiffInfo, p: [f64; 2], inverse: bool) -> Result<[f64; 2], String> {
    check(p)?;
    let mut angle = python_values::number(raster.metadata.get("rotation_degrees")).unwrap_or(0.);
    if !angle.is_finite() {
        return Err("Ongeldige rasterrotatie".into());
    }
    if angle.abs() < 1e-9 {
        return Ok(p);
    }
    if inverse {
        angle = -angle;
    }
    let c = center(raster);
    check(c)?;
    let a = angle.to_radians();
    let x = p[0] - c[0];
    let y = p[1] - c[1];
    let result = [
        c[0] + x * a.cos() - y * a.sin(),
        c[1] + x * a.sin() + y * a.cos(),
    ];
    check(result)?;
    Ok(result)
}
pub fn display(raster: &GeoTiffInfo, p: [f64; 2]) -> Result<[f64; 2], String> {
    rotate(raster, p, false)
}
pub fn set(
    raster: &mut GeoTiffInfo,
    key: &str,
    p: [f64; 2],
    displayed: bool,
) -> Result<bool, String> {
    if raster.epsg.is_some_and(|v| v != 28992) {
        return Err("Kaartpunten vereisen RD (EPSG:28992)".into());
    }
    if !matches!(key, REFERENCE | START) {
        return Err("Onbekend kaartpunt".into());
    }
    if raster.metadata_size()? + 128 > MAX_METADATA {
        return Err("Geen ruimte voor nieuw kaartpunt in laagmetadata".into());
    }
    let p = if displayed {
        rotate(raster, p, true)?
    } else {
        check(p)?;
        p
    };
    let value = json!({"x":p[0],"y":p[1]});
    if raster.metadata.get(key) == Some(&value) {
        return Ok(false);
    }
    if raster.metadata.is_null() {
        raster.metadata = json!({});
    }
    raster
        .metadata
        .as_object_mut()
        .unwrap()
        .insert(key.into(), value);
    Ok(true)
}
impl GeoTiffInfo {
    fn metadata_size(&self) -> Result<usize, String> {
        validate_metadata(&self.metadata)
    }
}
pub fn clear_reference(rasters: &mut [GeoTiffInfo]) -> usize {
    rasters
        .iter_mut()
        .map(|r| {
            r.metadata
                .as_object_mut()
                .and_then(|m| m.remove(REFERENCE))
                .is_some()
        })
        .filter(|b| *b)
        .count()
}
/// Source-level edits take precedence over an older per-slot metadata snapshot.
pub fn effective(raster: &GeoTiffInfo, slot: &Value) -> Value {
    let mut result = slot.as_object().cloned().unwrap_or_default();
    if let Some(metadata) = raster.metadata.as_object() {
        result.extend(metadata.clone());
    }
    Value::Object(result)
}
pub fn start_in(metadata: &Value) -> Option<[f64; 2]> {
    metadata.get(START).and_then(point)
}
pub fn candidates(
    dataset: &Dataset,
    rules: &[Rule],
    cancel: &AtomicBool,
) -> Result<Vec<usize>, String> {
    let (mut all, mut unmatched, mut markers) = (Vec::new(), Vec::new(), Vec::new());
    for (i, f) in dataset.features.iter().enumerate() {
        if i % 256 == 0 {
            crate::input::check(cancel)?;
        }
        let Geometry::Point(p) = f.geometry else {
            continue;
        };
        check([p.x, p.y])?;
        if p.z.is_some_and(|z| !z.is_finite()) {
            return Err("Niet-eindige profielhoogte".into());
        }
        if p.z.is_none() {
            continue;
        }
        all.push(i);
        if crate::matching_rule(&f.source_name, rules).is_none_or(|r| r.target_layer == "0") {
            unmatched.push(i);
            let name = python_values::uppercase(python_values::trim(&f.source_name));
            if name.is_empty()
                || profile::numeric(&name)
                || matches!(name.as_str(), "POINT" | "PUNT")
            {
                markers.push(i);
            }
        }
    }
    let selected = if markers.len() >= 2 {
        markers
    } else if unmatched.len() >= 2 {
        unmatched
    } else {
        all
    };
    if selected.len() > 10_000 {
        return Err("Te veel beginpuntkandidaten voor één proefsleuf".into());
    }
    Ok(if selected.len() >= 2 {
        selected
    } else {
        Vec::new()
    })
}
fn vertex(dataset: &Dataset, i: usize) -> Vertex {
    match dataset.features[i].geometry {
        Geometry::Point(p) => p,
        _ => unreachable!(),
    }
}
pub fn automatic(
    dataset: &Dataset,
    rules: &[Rule],
    metadata: &Value,
    roads: &[Vec<[f64; 2]>],
    terrain: &[Vec<[f64; 2]>],
    cancel: &AtomicBool,
) -> Result<Option<[f64; 2]>, String> {
    let indices = candidates(dataset, rules, cancel)?;
    if indices.is_empty() {
        return Ok(None);
    }
    if let Some(target) = start_in(metadata) {
        if let Some((i, d)) = nearest(dataset, &indices, target) {
            if d <= 0.5 {
                let p = vertex(dataset, i);
                return Ok(Some([p.x, p.y]));
            }
        }
    }
    let mut start = indices[0];
    let mut best = f64::INFINITY;
    for &i in &indices {
        crate::input::check(cancel)?;
        let p = vertex(dataset, i);
        let d = if roads.is_empty() {
            0.
        } else {
            profile::path_projection([p.x, p.y], roads, false, cancel)?
                .map_or(f64::INFINITY, |v| v.1)
        };
        if d < best {
            best = d;
            start = i;
        }
    }
    let a = vertex(dataset, start);
    let mut end = None;
    let mut farthest = -1.;
    for &i in &indices {
        let p = vertex(dataset, i);
        if (p.x - a.x).abs() <= 1e-6 && (p.y - a.y).abs() <= 1e-6 {
            continue;
        }
        let d = (p.x - a.x).hypot(p.y - a.y);
        if d > farthest {
            farthest = d;
            end = Some(p);
        }
    }
    let Some(b) = end else {
        return Ok(Some([a.x, a.y]));
    };
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length = dx.hypot(dy);
    if length <= 1e-6 {
        return Ok(Some([a.x, a.y]));
    }
    let middle = [(a.x + b.x) * 0.5, (a.y + b.y) * 0.5];
    let road = profile::path_projection(middle, roads, true, cancel)?;
    let ground = profile::path_projection(middle, terrain, true, cancel)?;
    let preferred = match (road, ground) {
        (Some(r), Some(t)) => Some(if t.1 * 0.8 < r.1 { t.0 } else { r.0 }),
        (Some(r), None) => Some(r.0),
        (None, Some(t)) => Some(t.0),
        _ => None,
    };
    Ok(Some(
        if preferred
            .is_some_and(|p| ((p[0] - a.x) * dx + (p[1] - a.y) * dy) / length > length * 0.5)
        {
            [b.x, b.y]
        } else {
            [a.x, a.y]
        },
    ))
}
fn nearest(dataset: &Dataset, indices: &[usize], p: [f64; 2]) -> Option<(usize, f64)> {
    let mut best = None;
    for &i in indices {
        let v = vertex(dataset, i);
        let d = (v.x - p[0]).hypot(v.y - p[1]);
        if best.is_none_or(|(_, b)| d < b) {
            best = Some((i, d));
        }
    }
    best
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub raw: [f64; 2],
    pub display: [f64; 2],
    pub selected: bool,
}
pub fn rows(
    raster: &GeoTiffInfo,
    dataset: &Dataset,
    rules: &[Rule],
    roads: &[Vec<[f64; 2]>],
    terrain: &[Vec<[f64; 2]>],
    cancel: &AtomicBool,
) -> Result<Vec<Candidate>, String> {
    let indices = candidates(dataset, rules, cancel)?;
    let forced = start_in(&raster.metadata)
        .and_then(|p| nearest(dataset, &indices, p))
        .filter(|v| v.1 <= 0.5)
        .map(|v| vertex(dataset, v.0));
    let selected = if let Some(forced) = forced {
        Some(forced)
    } else {
        profile::build_with_cancel(
            dataset,
            rules,
            profile::Options {
                roads,
                terrain_boundaries: terrain,
                ..Default::default()
            },
            cancel,
        )?
        .and_then(|p| nearest(dataset, &indices, [p.start.x, p.start.y]))
        .map(|v| vertex(dataset, v.0))
    }
    .map(|p| [p.x, p.y]);
    indices
        .into_iter()
        .map(|i| {
            let p = vertex(dataset, i);
            let raw = [p.x, p.y];
            Ok(Candidate {
                raw,
                display: display(raster, raw)?,
                selected: selected
                    .is_some_and(|s| (s[0] - p.x).abs() <= 1e-6 && (s[1] - p.y).abs() <= 1e-6),
            })
        })
        .collect()
}
pub fn pick(
    rows: &[Candidate],
    world: [f64; 2],
    meters_per_pixel: f64,
    selection_tolerance: f64,
) -> Option<usize> {
    if rows.len() < 2
        || !meters_per_pixel.is_finite()
        || !selection_tolerance.is_finite()
        || check(world).is_err()
    {
        return None;
    }
    let tolerance = (meters_per_pixel * 14.).max(selection_tolerance).min(4.);
    let mut best = None;
    for (i, c) in rows.iter().enumerate() {
        let d = (c.display[0] - world[0]).hypot(c.display[1] - world[1]);
        if d <= tolerance && best.is_none_or(|(_, b)| d < b) {
            best = Some((i, d));
        }
    }
    best.map(|v| v.0)
}
#[derive(Debug)]
pub struct Update {
    pub index: usize,
    pub path: PathBuf,
    pub job_id: Option<u64>,
    pub point: [f64; 2],
}
pub fn local_dataset_path(raster: &GeoTiffInfo) -> Option<PathBuf> {
    let explicit = raster
        .metadata
        .get("kickthemap_job_features_path")
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty())
        .map(PathBuf::from);
    if let Some(path) = explicit {
        let path = if path.is_absolute() {
            path
        } else {
            raster
                .path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .join(path)
        };
        if path.is_file() {
            return Some(path);
        }
    }
    let stem = raster.path.file_stem()?.to_string_lossy();
    let path = raster
        .path
        .with_file_name(format!("{stem}_jobFeatures.json"));
    path.is_file().then_some(path)
}
pub fn context(layers: &[crate::bgt::BgtLayer], cancel: &AtomicBool) -> Result<Context, String> {
    let (mut roads, mut terrain) = (Vec::new(), Vec::new());
    let mut points = 0usize;
    for layer in layers.iter().filter(|l| l.visible) {
        let to = if layer.kind == crate::bgt::BgtKind::Wegdeel {
            &mut roads
        } else {
            &mut terrain
        };
        for path in &layer.paths {
            crate::input::check(cancel)?;
            points = points.saturating_add(path.len());
            if points > 2_000_000 || to.len() >= 10_000 {
                return Err("Te veel BGT-context voor beginpunten".into());
            }
            let path = path.iter().map(|&(x, y)| [x, y]).collect::<Vec<_>>();
            if path.iter().flatten().any(|v| !v.is_finite()) {
                return Err("Ongeldige BGT-coördinaten".into());
            }
            to.push(path);
        }
    }
    Ok((roads, terrain))
}
pub fn prepare(
    inputs: &crate::export_worker::Inputs,
    rules: &[Rule],
    control: &crate::export_control::Control,
) -> Result<(Vec<Update>, Vec<String>), String> {
    prepare_online(inputs, rules, control, false)
}
fn online_context(
    client: &mut crate::profile_context::Client,
    raster: &GeoTiffInfo,
    dataset: &Dataset,
    rules: &[Rule],
    cancel: &AtomicBool,
    warnings: &mut Vec<String>,
) -> Result<(crate::profile_context::Paths, crate::profile_context::Paths), String> {
    let indices = candidates(dataset, rules, cancel)?;
    if indices.is_empty()
        || start_in(&raster.metadata)
            .and_then(|p| nearest(dataset, &indices, p))
            .is_some_and(|v| v.1 <= 0.5)
    {
        return Ok((Default::default(), Default::default()));
    }
    let bounds = crate::profile_context::padded(raster.bounds)?;
    let mut load =
        |kind: crate::profile_context::Kind| -> Result<crate::profile_context::Paths, String> {
            let result = client.fetch(kind, bounds, cancel);
            crate::input::check(cancel)?;
            match result {
                Ok(paths) => Ok(paths),
                Err(error) => {
                    if warnings.len() < 5000 {
                        let name = match kind {
                            crate::profile_context::Kind::Roads => "Wegen",
                            crate::profile_context::Kind::Terrain => "Terrein",
                        };
                        warnings.push(format!("{}: {name} niet beschikbaar ({error}); beginpunt zonder deze bron bepaald",raster.path.file_name().unwrap_or_default().to_string_lossy()));
                    }
                    Ok(Default::default())
                }
            }
        };
    Ok((
        load(crate::profile_context::Kind::Roads)?,
        load(crate::profile_context::Kind::Terrain)?,
    ))
}
pub fn prepare_online(
    inputs: &crate::export_worker::Inputs,
    rules: &[Rule],
    control: &crate::export_control::Control,
    online: bool,
) -> Result<(Vec<Update>, Vec<String>), String> {
    if inputs.rasters.len() > 5000 {
        return Err("Te veel proefsleuven voor beginpunten".into());
    }
    let (roads, terrain) = if online {
        (Vec::new(), Vec::new())
    } else {
        context(&inputs.bgt, control.flag())?
    };
    let mut updates = Vec::new();
    let mut warnings = Vec::new();
    let mut provider = if online {
        Some(crate::profile_context::Client::new()?)
    } else {
        None
    };
    let mut local = None::<(PathBuf, Dataset)>;
    let mut jobs = std::collections::HashMap::<u64, Option<usize>>::new();
    for (i, d) in inputs.datasets.iter().enumerate() {
        jobs.entry(d.job_id)
            .and_modify(|v| *v = None)
            .or_insert(Some(i));
    }
    for (index, raster) in inputs.rasters.iter().enumerate() {
        control.check()?;
        let Some(job_id) = raster.kickthemap_job_id.filter(|v| *v > 0) else {
            continue;
        };
        control.set_detail(&format!(
            "Beginpunten laden ({}/{}): {}",
            index + 1,
            inputs.rasters.len(),
            raster
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ));
        let result = (|| {
            validate_metadata(&raster.metadata)?;
            if raster.epsg.is_some_and(|e| e != 28992) {
                return Err("Geen RD-coördinatenstelsel".to_owned());
            }
            let dataset = if let Some(position) = jobs.get(&job_id) {
                &inputs.datasets[position
                    .ok_or("Meerdere objectbronnen met dezelfde job-ID; kies een unieke bron")?]
            } else {
                let path = local_dataset_path(raster).ok_or(
                    "Geen gekoppelde objectdata beschikbaar; laad objecten via KickTheMap",
                )?;
                if crate::geotiff::job_sidecar(&path).is_some_and(|s| s.0 != job_id) {
                    return Err("Lokale objectdata hoort bij een andere job".into());
                }
                if local.as_ref().is_none_or(|(p, _)| *p != path) {
                    let bytes = crate::input::read(
                        &path,
                        64 * 1024 * 1024,
                        "Beginpuntobjecten",
                        control.flag(),
                    )?;
                    let body = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
                    let mut data = crate::parse::parse_job_features_with_cancel(
                        body,
                        job_id,
                        &raster.kickthemap_job_title,
                        control.flag(),
                    )?;
                    data.source_path = path.clone();
                    local = Some((path, data));
                }
                &local.as_ref().unwrap().1
            };
            let fetched = if let Some(provider) = &mut provider {
                Some(online_context(
                    provider,
                    raster,
                    dataset,
                    rules,
                    control.flag(),
                    &mut warnings,
                )?)
            } else {
                None
            };
            let point = automatic(
                dataset,
                rules,
                &raster.metadata,
                fetched
                    .as_ref()
                    .map_or(roads.as_slice(), |c| c.0.as_slice()),
                fetched
                    .as_ref()
                    .map_or(terrain.as_slice(), |c| c.1.as_slice()),
                control.flag(),
            )?
            .ok_or("Onvoldoende profielpunten met hoogte of geen beginpuntkandidaten")?;
            Ok::<_, String>(point)
        })();
        control.check()?;
        match result {
            Ok(point) => updates.push(Update {
                index,
                path: raster.path.clone(),
                job_id: Some(job_id),
                point,
            }),
            Err(e) => {
                if warnings.len() < 5000 {
                    warnings.push(format!(
                        "{}: {e}",
                        raster
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    ));
                }
            }
        }
    }
    Ok((updates, warnings))
}
pub fn prepare_candidates(
    inputs: &crate::export_worker::Inputs,
    path: &std::path::Path,
    rules: &[Rule],
    control: &crate::export_control::Control,
) -> Result<Vec<Candidate>, String> {
    prepare_candidates_online(inputs, path, rules, control, false).map(|v| v.0)
}
pub fn prepare_candidates_online(
    inputs: &crate::export_worker::Inputs,
    path: &std::path::Path,
    rules: &[Rule],
    control: &crate::export_control::Control,
    online: bool,
) -> Result<(Vec<Candidate>, Vec<String>), String> {
    let matching = inputs
        .rasters
        .iter()
        .filter(|r| r.path == path)
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err("Kies één unieke proefsleufbron".into());
    }
    let raster = matching[0];
    if raster.epsg.is_some_and(|v| v != 28992) {
        return Err("Beginpunten vereisen RD (EPSG:28992)".into());
    }
    let job = raster
        .kickthemap_job_id
        .ok_or("Koppel eerst KickTheMap-objectgegevens aan deze proefsleuf")?;
    let matching = inputs
        .datasets
        .iter()
        .filter(|d| d.job_id == job)
        .collect::<Vec<_>>();
    let loaded;
    let dataset = match matching.as_slice() {
        [d] => *d,
        [] => {
            let path = local_dataset_path(raster)
                .ok_or("Laad eerst de gekoppelde objectgegevens via KickTheMap")?;
            if crate::geotiff::job_sidecar(&path).is_some_and(|s| s.0 != job) {
                return Err("Lokale objectdata hoort bij een andere job".into());
            }
            let bytes =
                crate::input::read(&path, 64 * 1024 * 1024, "Beginpuntobjecten", control.flag())?;
            loaded = crate::parse::parse_job_features_with_cancel(
                std::str::from_utf8(&bytes).map_err(|e| e.to_string())?,
                job,
                &raster.kickthemap_job_title,
                control.flag(),
            )?;
            &loaded
        }
        _ => return Err("Meerdere objectbronnen hebben dezelfde job-ID".into()),
    };
    let (roads, terrain) = if online {
        (Vec::new(), Vec::new())
    } else {
        context(&inputs.bgt, control.flag())?
    };
    let mut warnings = Vec::new();
    let fetched = if online {
        Some(online_context(
            &mut crate::profile_context::Client::new()?,
            raster,
            dataset,
            rules,
            control.flag(),
            &mut warnings,
        )?)
    } else {
        None
    };
    let rows = rows(
        raster,
        dataset,
        rules,
        fetched
            .as_ref()
            .map_or(roads.as_slice(), |c| c.0.as_slice()),
        fetched
            .as_ref()
            .map_or(terrain.as_slice(), |c| c.1.as_slice()),
        control.flag(),
    )?;
    Ok((rows, warnings))
}
pub fn apply(rasters: &mut [GeoTiffInfo], updates: &[Update]) -> Result<usize, String> {
    let mut seen = HashSet::new();
    for update in updates {
        let r = rasters
            .get(update.index)
            .ok_or("Proefsleuflijst is gewijzigd")?;
        if !seen.insert(update.index)
            || r.path != update.path
            || r.kickthemap_job_id != update.job_id
        {
            return Err("Beginpunten horen bij andere projectbronnen".into());
        }
        check(update.point)?;
        if r.epsg.is_some_and(|v| v != 28992) {
            return Err("Beginpunten vereisen RD (EPSG:28992)".into());
        }
        if r.metadata_size()? + 128 > MAX_METADATA {
            return Err("Te grote laagmetadata voor beginpunten".into());
        }
    }
    let mut changed = 0;
    for update in updates {
        changed += usize::from(set(&mut rasters[update.index], START, update.point, false)?);
    }
    Ok(changed)
}
