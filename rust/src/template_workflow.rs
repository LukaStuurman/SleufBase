//! Assemble the native dialog's project sources on the export worker. Large
//! datasets stay borrowed; all map slots share one prepared CAD path vector.
use crate::{
    export_control::Control,
    export_worker::Inputs,
    map_export::{Background, Source, Stroke},
    template_export::{self, Profile, Request, Slot},
    template_geometry::{Entity, Layer, Paint, Shape},
    template_order::State,
    Bounds, Rule,
};
use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};
fn line(layer: &str, points: Vec<[f64; 2]>, color: Option<u32>) -> Entity {
    Entity {
        layer: layer.into(),
        layout: None,
        paint: Paint {
            true_color: color,
            ..Default::default()
        },
        shape: Shape::Polyline {
            vertices: points,
            closed: false,
        },
    }
}
pub fn comments(bounds: Bounds, comments: &[crate::project::Comment], extra: &str) -> String {
    let padding =
        ((bounds.max_x - bounds.min_x).max(bounds.max_y - bounds.min_y) * 0.08).clamp(0.5, 2.);
    let mut seen = std::collections::HashSet::new();
    let mut lines = Vec::new();
    for text in comments
        .iter()
        .filter(|c| {
            c.x >= bounds.min_x - padding
                && c.x <= bounds.max_x + padding
                && c.y >= bounds.min_y - padding
                && c.y <= bounds.max_y + padding
        })
        .map(|c| c.text.as_str())
        .chain(std::iter::once(extra))
    {
        for line in text.split(|c| {
            matches!(
                c,
                '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}'
                    ..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
            )
        }) {
            let line = crate::python_values::trim(line);
            if !line.is_empty() && seen.insert(crate::python_values::casefold(line)) {
                lines.push(line);
            }
        }
    }
    if lines.is_empty() {
        "Opmerkingen:".into()
    } else {
        format!("Opmerkingen:\\P{}", lines.join("\\P"))
    }
}
pub fn request(
    state: &State,
    inputs: &Inputs,
    rules: &[Rule],
    control: &Control,
) -> Result<Request, String> {
    let mut client = None;
    request_with_bgt(state, inputs, rules, control, |bounds| {
        if client.is_none() {
            client = Some(crate::bgt_tile_client::Client::new(Default::default())?);
        }
        Ok(client
            .as_ref()
            .unwrap()
            .fetch_surface_features(bounds, control.flag())?
            .into_iter()
            .map(|f| f.surface)
            .collect())
    })
}
/// Also supports an existing BGT provider/cache; returned geometries must have
/// completed native repair/union/clipping before being handed to this stage.
pub fn request_with_bgt(
    state: &State,
    inputs: &Inputs,
    rules: &[Rule],
    control: &Control,
    mut fetch: impl FnMut(Bounds) -> Result<Vec<crate::bgt_surface::Feature>, String>,
) -> Result<Request, String> {
    control.check()?;
    state.validate()?;
    let labels = state.labels()?;
    if !labels.iter().any(Option::is_some) {
        return Err("Kies minimaal één proefsleuf voor de sjabloonexport".into());
    }
    let rasters = inputs
        .rasters
        .iter()
        .map(|r| (&r.path, r))
        .collect::<HashMap<_, _>>();
    let mut overlays = Vec::new();
    let mut geometry = Vec::new();
    let mut layers = vec![Layer {
        name: "PROEFSLEUVEN_KADASTER".into(),
        color: 8,
        true_color: None,
        lineweight: None,
    }];
    let mut points = 0usize;
    for overlay in &inputs.overlays {
        if !overlay.visible {
            continue;
        }
        for feature in overlay.rendered_features() {
            control.check()?;
            if !feature.visible {
                continue;
            }
            for path in feature.paths() {
                if path.len() < 2 {
                    continue;
                }
                points += path.len();
                if points > 2_000_000 {
                    return Err("Te veel CAD-punten voor de sjabloonexport".into());
                }
                let path = path.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>();
                let rgb = feature.rgb;
                geometry.push(line(
                    "PROEFSLEUVEN_KADASTER",
                    path.clone(),
                    Some((u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2])),
                ));
                overlays.push(Stroke {
                    points: path,
                    color: rgb,
                    selected: false,
                    highlighted: false,
                });
            }
        }
    }
    if let Some(cad) = inputs.cadastral.as_ref().filter(|c| c.visible) {
        let mut names = std::collections::HashSet::new();
        for group in &cad.linework {
            if names.insert(group.layer_name.clone()) {
                layers.push(Layer {
                    name: group.layer_name.clone(),
                    color: 8,
                    true_color: None,
                    lineweight: None,
                });
            }
            for path in &group.paths {
                points += path.len();
                if points > 2_000_000 {
                    return Err("Te veel Kadaster-punten voor de sjabloonexport".into());
                }
                geometry.push(line(
                    &group.layer_name,
                    path.iter().map(|&(x, y)| [x, y]).collect(),
                    None,
                ));
            }
        }
        for label in &cad.labels {
            if names.insert(label.layer_name.clone()) {
                layers.push(Layer {
                    name: label.layer_name.clone(),
                    color: 8,
                    true_color: None,
                    lineweight: None,
                });
            }
            geometry.push(Entity {
                layer: label.layer_name.clone(),
                layout: None,
                paint: Paint {
                    color: Some(8),
                    ..Default::default()
                },
                shape: Shape::Text {
                    text: label.text.clone(),
                    insert: [label.position.0, label.position.1],
                    height: 1.,
                    rotation: (-label.rotation).rem_euclid(360.),
                    style: "KADASTRALE_LABELS".into(),
                    halign: 1,
                    valign: 2,
                },
            });
        }
    }
    let overlays = Arc::new(overlays);
    let mut roads = Vec::new();
    let mut terrain_boundaries = Vec::new();
    for b in &inputs.bgt {
        if b.visible {
            let target = if b.kind == crate::bgt::BgtKind::Wegdeel {
                &mut roads
            } else {
                &mut terrain_boundaries
            };
            target.extend(
                b.paths
                    .iter()
                    .map(|p| p.iter().map(|&(x, y)| [x, y]).collect::<Vec<_>>()),
            );
        }
    }
    let mut slots = Vec::new();
    let mut surface_cache = HashMap::<[u64; 4], Arc<Vec<crate::bgt_surface::Feature>>>::new();
    let mut surface_points = 0usize;
    let mut surface_count = 0usize;
    let mut datasets = HashMap::new();
    for (i, d) in inputs.datasets.iter().enumerate() {
        datasets.insert(d.job_id, i);
    }
    for (row, label) in state.rows.iter().zip(labels) {
        control.check()?;
        let Some(path) = &row.raster else {
            slots.push(None);
            continue;
        };
        if !row.included {
            continue;
        }
        let raster = rasters
            .get(path)
            .ok_or_else(|| format!("GeoTIFF is niet meer geladen: {}", path.display()))?;
        if raster.epsg.is_some_and(|e| e != 28992) {
            return Err(format!("{} gebruikt geen RD (EPSG:28992)", path.display()));
        }
        let dataset = if !row.without_profile {
            if let Some(path) = &row.dataset {
                Some(
                    inputs
                        .datasets
                        .iter()
                        .position(|d| d.source_path == *path)
                        .ok_or("De gekozen profieljob is niet meer geladen")?,
                )
            } else {
                raster
                    .kickthemap_job_id
                    .and_then(|id| datasets.get(&id).copied())
            }
        } else {
            None
        };
        let label = label.unwrap();
        let b = raster.bounds;
        let mut boundary = Vec::with_capacity(5);
        // Use the actual affine TIFF corners, rather than its axis-aligned box.
        for (x, y) in [
            (0., 0.),
            (f64::from(raster.width), 0.),
            (f64::from(raster.width), f64::from(raster.height)),
            (0., f64::from(raster.height)),
            (0., 0.),
        ] {
            let p = raster.transform.pixel_to_world(x, y);
            boundary.push([p.0, p.1]);
        }
        geometry.push(line("PROEFSLEUVEN_GRENZEN", boundary, None));
        let source = crate::template_raster::Source {
            path: raster.path.clone(),
            transform: raster.transform,
            reverse_orientation: state.reverse,
            axis: None,
        };
        let surfaces = if state.profiles && state.auto_bgt && dataset.is_some() {
            control.set_detail(&format!("BGT-ondergrond voor {label} ophalen"));
            let margin = 2.;
            let bounds = Bounds {
                min_x: b.min_x - margin,
                min_y: b.min_y - margin,
                max_x: b.max_x + margin,
                max_y: b.max_y + margin,
            };
            let key = [bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y].map(f64::to_bits);
            if let std::collections::hash_map::Entry::Vacant(entry) = surface_cache.entry(key) {
                let features = fetch(bounds)?;
                surface_count += features.len();
                for feature in &features {
                    surface_points +=
                        crate::bgt_geometry::vertex_count(&feature.geometry, control.flag())?;
                    if surface_points > 2_000_000 || surface_count > 100_000 {
                        return Err(
                            "BGT-ondergrond overschrijdt het gezamenlijke exportbudget".into()
                        );
                    }
                }
                entry.insert(Arc::new(features));
            }
            Some(surface_cache[&key].clone())
        } else {
            None
        };
        slots.push(Some(Slot {
            bounds: b,
            title: format!("Proefsleuf {}", label.strip_prefix("PS").unwrap_or(&label)),
            label,
            address: row.address.clone(),
            comments: comments(b, &inputs.comments, &row.comments),
            tiff_png: Default::default(),
            tiff_source: Some(source),
            virtual_source: None,
            photos: Vec::new(),
            map_png: Default::default(),
            map_source: Some(Source {
                background: if state.aerial {
                    Background::Pdok
                } else {
                    Background::Solid {
                        color: [245, 245, 245, 255],
                    }
                },
                overlays: overlays.clone(),
                ..Default::default()
            }),
            profile: dataset.map(|dataset| Profile {
                dataset,
                draw: state.profiles,
                metadata: row.metadata.clone(),
                reverse: state.reverse,
                marker_diameter: state.marker_diameter,
                manual_scale: row.scale,
                show_reference: state.show_reference,
                avoid_collisions: state.avoid_collisions,
                auto_fill_bgt: state.auto_bgt,
                ..Default::default()
            }),
            report_dataset: dataset.or_else(|| {
                raster
                    .kickthemap_job_id
                    .and_then(|id| datasets.get(&id).copied())
            }),
            report_label: None,
            surfaces_constructed: surfaces.is_some(),
            surfaces,
        }));
    }
    layers.push(Layer {
        name: "PROEFSLEUVEN_GRENZEN".into(),
        color: 7,
        true_color: None,
        lineweight: None,
    });
    Ok(Request {
        template: state.template.clone().unwrap_or_default(),
        slots,
        datasets: Vec::new(),
        rules: rules.to_vec(),
        roads,
        terrain_boundaries,
        surfaces: Vec::new(),
        layers,
        geometry,
        settings: template_export::Settings {
            paired: state.paired,
            project: Some(crate::python_values::trim(&state.project).into()),
            drawn_by: state.drawn_by.clone(),
            label_gap: state.label_gap,
            ..Default::default()
        },
    })
}
pub fn export(
    path: &Path,
    state: &State,
    inputs: &Inputs,
    rules: &[Rule],
    control: &Control,
) -> Result<template_export::Exported, String> {
    export_with_excel(path, state, inputs, rules, false, control)
}
pub fn export_with_excel(
    path: &Path,
    state: &State,
    inputs: &Inputs,
    rules: &[Rule],
    discipline_excel: bool,
    control: &Control,
) -> Result<template_export::Exported, String> {
    control.check()?;
    // Replacement is valid for earlier exports, never for loaded project data.
    // Do this before fetching BGT or preparing any new assets.
    let target = std::fs::canonicalize(path).ok();
    let mut sources = inputs
        .protected_paths
        .iter()
        .map(PathBuf::as_path)
        .collect::<Vec<_>>();
    sources.extend(inputs.datasets.iter().map(|d| d.source_path.as_path()));
    sources.extend(inputs.overlays.iter().map(|d| d.path.as_path()));
    for r in &inputs.rasters {
        sources.push(r.path.as_path());
        sources.extend(r.kickthemap_job_sidecar.as_deref());
    }
    sources.extend(
        inputs
            .trenches
            .iter()
            .filter_map(|t| t.source_path.as_deref()),
    );
    sources.extend(state.template.as_deref());
    for &source in &sources {
        control.check()?;
        if source == path
            || target
                .as_ref()
                .is_some_and(|p| std::fs::canonicalize(source).is_ok_and(|s| s == *p))
        {
            return Err(
                "Kies een uitvoerbestand dat verschilt van alle geladen projectbronnen".into(),
            );
        }
    }
    let mut request = request(state, inputs, rules, control)?;
    request.settings.discipline_excel = discipline_excel;
    request.settings.protected_sources = sources.into_iter().map(Path::to_owned).collect();
    let mut bundled = None;
    if request.template.as_os_str().is_empty() {
        let mut file = tempfile::Builder::new()
            .prefix("sleufbase-template-")
            .suffix(".dxf")
            .tempfile()
            .map_err(|e| e.to_string())?;
        file.write_all(include_bytes!("../../assets/cadastral_template.dxf"))
            .map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;
        request.template = file.path().to_owned();
        bundled = Some(file);
    }
    let result =
        template_export::export_with_datasets(path, &request, &inputs.datasets, control, |s| {
            control.set_detail(s)
        });
    drop(bundled);
    result
}
