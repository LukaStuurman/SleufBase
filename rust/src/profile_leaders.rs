//! Active v0.3.10 profile label search. Optional dynamic-donor patches are not
//! enabled by the production Python package and must not replace this contract.
use crate::{
    dxf_document::Document,
    template_geometry::{self, Entity, Paint, Shape},
};
use std::sync::atomic::AtomicBool;
pub const BLOCK: &str = "SAL-VERWIJZING_LEIDING_BOVENKANT_INFO_DIEPTE-SOD";
pub const LAYER: &str = "X-XX-AL-VERWIJZING-SD";
fn normalized_number(value: f64) -> Result<String, String> {
    finite(&[value])?;
    let rounded = value.round_ties_even();
    if (value - rounded).abs() <= 1e-6 {
        Ok(format!("{rounded:.0}"))
    } else {
        Ok(format!("{value:.3}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .replace('.', ","))
    }
}
/// Active original description: configured discipline, sleeve, bundle,
/// diameter/prefix and material. Numeric matching shares the frozen Python
/// Unicode-decimal matcher with profile marker sizing.
pub fn description(feature: &crate::Feature, base: &str, prefix: &str) -> Result<String, String> {
    use crate::python_values::{casefold, trim};
    if [
        base,
        prefix,
        &feature.attributes[0],
        &feature.attributes[1],
        &feature.attributes[2],
    ]
    .iter()
    .any(|v| v.len() > 1_048_576)
    {
        return Err("Te lange profielverwijzingsmetadata".into());
    }
    let raw = trim(&feature.attributes[2]).trim_matches(['\'', '"']);
    let base = trim(base);
    let mut result = if base.is_empty() {
        "ONBEKEND".into()
    } else {
        base.to_owned()
    };
    if casefold(raw).contains("mantelbuis") && !casefold(&result).contains("mantelbuis") {
        result.push_str(" mantelbuis");
    }
    if let Some(n) = crate::profile::first_number(raw, false) {
        result.push_str(" Bundel: ");
        result.push_str(&normalized_number(n)?);
        result.push('X');
    }
    if let Some(n) = crate::profile::diameter_mm(feature).filter(|n| *n > 0.) {
        let prefix = trim(prefix);
        result.push(' ');
        result.push_str(if prefix.is_empty() { "Ø" } else { prefix });
        result.push_str(&normalized_number(n)?);
    }
    let material = trim(&feature.attributes[0]);
    if !material.is_empty() {
        result.push(' ');
        result.push_str(material);
    }
    if result.len() > 1_048_576 {
        return Err("Te lange profielverwijzingstekst".into());
    }
    Ok(result)
}
/// Derive real leader inputs from native measurement/profile/band results.
/// In particular the leader scale is the global marker scale, not the measured
/// pipe diameter used for point circles; shifted bands retain their own X.
pub fn entries(
    profile: &crate::profile::Profile<'_>,
    data: &crate::Dataset,
    drawing: &crate::profile_drawing::Drawing,
    marker_scale: f64,
    prefix: &str,
    cancel: &AtomicBool,
) -> Result<Vec<Entry>, String> {
    crate::input::check(cancel)?;
    finite(&[marker_scale, drawing.layout.scale])?;
    if drawing.bands.len() > 25_000 || drawing.layout.scale <= 0. {
        return Err("Ongeldige profielverwijzingsbanden".into());
    }
    let marker_scale = marker_scale.max(0.005);
    let mut output: Vec<Entry> = Vec::new();
    let mut previous = None;
    let mut bytes = 0usize;
    let start_z = profile.start.z.unwrap_or(0.);
    finite(&[start_z])?;
    for band in &drawing.bands {
        crate::input::check(cancel)?;
        let point = profile
            .points
            .get(band.point_index)
            .ok_or("Profielband mist meetpunt")?;
        if point.feature_index != band.feature_index
            || point.point_z != band.point_z
            || point.chainage != band.chainage
        {
            return Err("Profielband hoort niet bij het actuele meetpunt".into());
        }
        if point.is_endpoint {
            continue;
        }
        let feature = data
            .features
            .get(point.feature_index)
            .ok_or("Profielverwijzing mist meetobject")?;
        let (description, depth_text) = if previous == Some(band.point_index) {
            let last = output.last().ok_or("Profielverwijzingscache ontbreekt")?;
            (last.description.clone(), last.depth_text.clone())
        } else {
            let depth = start_z - band.point_z;
            finite(&[depth])?;
            (
                description(feature, &point.description, prefix)?,
                format!("Diepte: {:.2}", if depth > 0. { depth } else { 0. }),
            )
        };
        previous = Some(band.point_index);
        bytes = bytes
            .checked_add(description.len())
            .and_then(|b| b.checked_add(depth_text.len()))
            .filter(|b| *b <= 128 * 1024 * 1024)
            .ok_or("Te veel profielverwijzingstekst")?;
        let y = drawing.layout.reference_y
            + (band.point_z - profile.reference_level) * drawing.layout.scale;
        let entry = Entry {
            anchor_x: band.anchor_x,
            leader_line_start_y: y,
            leader_block_start_y: y + marker_scale,
            leader_top_y: y + 10. * marker_scale,
            band_y: drawing.layout.top_band_y,
            band_connector_y: drawing.layout.top_band_y + 0.04,
            band_x: Some(band.placed_x),
            leader_scale: Some(marker_scale),
            leader_color: point.color,
            description,
            depth_text,
        };
        finite(&[
            entry.anchor_x,
            entry.leader_line_start_y,
            entry.leader_block_start_y,
            entry.leader_top_y,
            entry.band_y,
            entry.band_connector_y,
            band.placed_x,
        ])?;
        output.push(entry);
    }
    Ok(output)
}
pub struct ProfileOptions<'a> {
    pub marker_scale: f64,
    pub diameter_prefix: &'a str,
    pub avoid_collisions: bool,
}
pub struct Plan {
    pub entries: Vec<Entry>,
    pub placed: Vec<Placed>,
}
/// Complete an already composed native frame/body/reference drawing. All
/// derivation/search succeeds before append commits any geometry to the frame.
pub fn compose(
    profile: &crate::profile::Profile<'_>,
    data: &crate::Dataset,
    drawing: &mut crate::profile_drawing::Drawing,
    obstacles: &crate::profile_surface::Obstacles,
    options: ProfileOptions<'_>,
    cancel: &AtomicBool,
) -> Result<Plan, String> {
    let entries = entries(
        profile,
        data,
        drawing,
        options.marker_scale,
        options.diameter_prefix,
        cancel,
    )?;
    let placed = solve(
        &entries,
        options.marker_scale,
        Options {
            hard: &obstacles.hard,
            soft: &obstacles.soft,
            text: &obstacles.text,
            min_x: Some(drawing.layout.start_x),
            max_x: Some(drawing.layout.end_x),
            avoid_collisions: options.avoid_collisions,
        },
        cancel,
    )?;
    append(&mut drawing.entities, &entries, &placed, cancel)?;
    Ok(Plan { entries, placed })
}
type Box4 = [f64; 4];
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    pub anchor_x: f64,
    pub leader_line_start_y: f64,
    pub leader_block_start_y: f64,
    pub leader_top_y: f64,
    pub band_y: f64,
    pub band_connector_y: f64,
    pub band_x: Option<f64>,
    pub leader_scale: Option<f64>,
    pub leader_color: u16,
    pub description: String,
    pub depth_text: String,
}
pub struct Options<'a> {
    pub hard: &'a [Box4],
    pub soft: &'a [Box4],
    pub text: &'a [Box4],
    pub min_x: Option<f64>,
    pub max_x: Option<f64>,
    pub avoid_collisions: bool,
}
impl Default for Options<'_> {
    fn default() -> Self {
        Self {
            hard: &[],
            soft: &[],
            text: &[],
            min_x: None,
            max_x: None,
            avoid_collisions: true,
        }
    }
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Placed {
    pub entry_index: usize,
    pub text_insert: [f64; 2],
    pub marker_scale: f64,
}
fn finite(values: &[f64]) -> Result<(), String> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err("Niet-eindige profielverwijzingsindeling".into())
    }
}
fn expanded(s: Box4, c: f64) -> Box4 {
    [
        s[0].min(s[2]) - c,
        s[1].min(s[3]) - c,
        s[0].max(s[2]) + c,
        s[1].max(s[3]) + c,
    ]
}
fn overlap(a: Box4, b: Box4) -> f64 {
    if a[2] <= b[0] || b[2] <= a[0] || a[3] <= b[1] || b[3] <= a[1] {
        return 0.;
    }
    let w = a[2].min(b[2]) - a[0].max(b[0]);
    let h = a[3].min(b[3]) - a[1].max(b[1]);
    if w <= 0. || h <= 0. {
        0.
    } else {
        w * h
    }
}
fn penalty(a: Box4, boxes: &[Box4], weight: f64) -> f64 {
    boxes
        .iter()
        .fold(0., |sum, b| sum + overlap(a, *b) * weight)
}
fn lines_penalty(lines: [Box4; 2], boxes: &[Box4], weight: f64) -> f64 {
    lines
        .into_iter()
        .flat_map(|a| boxes.iter().map(move |b| overlap(a, *b) * weight))
        .fold(0., |sum, v| sum + v)
}
fn shifts(count: usize, low: usize, high: usize, step: f64) -> Vec<f64> {
    let mut v = vec![0.];
    for i in 1..=count.max(low).min(high) {
        let x = step * i as f64;
        v.extend([x, -x]);
    }
    v
}
pub fn connector(entry: &Entry) -> Box4 {
    let bx = entry.band_x.unwrap_or(entry.anchor_x);
    let end = if (bx - entry.anchor_x).abs() > 1e-6 {
        entry.band_connector_y
    } else {
        entry.band_y
    };
    [
        entry.anchor_x,
        entry.leader_line_start_y,
        entry.anchor_x,
        end,
    ]
}
/// Stable original candidate order and scoring, including the soft-obstacle
/// retry and minimum-penalty fallback. Store hard scores in a flat index instead
/// of hashing float pairs. With collision avoidance off (including >80 entries)
/// the original always accepts the first candidate: skip all unused scoring.
pub fn solve(
    entries: &[Entry],
    fallback_scale: f64,
    options: Options<'_>,
    cancel: &AtomicBool,
) -> Result<Vec<Placed>, String> {
    crate::input::check(cancel)?;
    finite(&[fallback_scale])?;
    if entries.len() > 25_000
        || options.hard.len() + options.soft.len() + options.text.len() > 200_000
    {
        return Err("Te veel profielverwijzingen of obstakels".into());
    }
    for x in options.min_x.into_iter().chain(options.max_x) {
        finite(&[x])?;
    }
    for (i, b) in options
        .hard
        .iter()
        .chain(options.soft)
        .chain(options.text)
        .enumerate()
    {
        if i % 1024 == 0 {
            crate::input::check(cancel)?;
        }
        finite(b)?;
    }
    let mut maximum = 0.005f64;
    for e in entries {
        crate::input::check(cancel)?;
        finite(&[
            e.anchor_x,
            e.leader_line_start_y,
            e.leader_block_start_y,
            e.leader_top_y,
            e.band_y,
            e.band_connector_y,
            e.band_x.unwrap_or(e.anchor_x),
            e.leader_scale.unwrap_or(fallback_scale),
        ])?;
        if e.leader_color > 256 || e.description.len() > 1_048_576 || e.depth_text.len() > 1_048_576
        {
            return Err("Ongeldige profielverwijzingstekst of kleur".into());
        }
        maximum = maximum.max(e.leader_scale.unwrap_or(fallback_scale).max(0.005));
    }
    let mut order = (0..entries.len()).collect::<Vec<_>>();
    order.sort_by(|a, b| {
        entries[*a]
            .anchor_x
            .partial_cmp(&entries[*b].anchor_x)
            .unwrap()
            .then_with(|| {
                entries[*a]
                    .leader_top_y
                    .partial_cmp(&entries[*b].leader_top_y)
                    .unwrap()
            })
    });
    let avoid = options.avoid_collisions && entries.len() <= 80;
    if !avoid {
        return order
            .into_iter()
            .map(|i| {
                crate::input::check(cancel)?;
                let e = &entries[i];
                let scale = e.leader_scale.unwrap_or(fallback_scale).max(0.005);
                let x = (e.anchor_x - 0.5 * scale) + 0.5 * scale;
                finite(&[x])?;
                Ok(Placed {
                    entry_index: i,
                    text_insert: [x, e.leader_top_y],
                    marker_scale: scale,
                })
            })
            .collect();
    }
    let clearance = (0.03f64 * 0.75).max(maximum * 0.75);
    finite(&[clearance])?;
    let mut hard = options
        .hard
        .iter()
        .map(|s| expanded(*s, clearance))
        .collect::<Vec<_>>();
    let soft = options
        .soft
        .iter()
        .map(|s| expanded(*s, clearance))
        .collect::<Vec<_>>();
    for b in hard.iter().chain(&soft) {
        finite(b)?;
    }
    let yshifts = shifts(entries.len() + 2, 8, 18, 0.09);
    let mut placed_boxes = Vec::new();
    let mut output = Vec::with_capacity(entries.len());
    for &index in &order {
        crate::input::check(cancel)?;
        let e = &entries[index];
        let scale = e.leader_scale.unwrap_or(fallback_scale).max(0.005);
        let gap = (0.05f64).max(scale * 2.5);
        let step = (0.05f64).max(gap);
        let px = (0.02f64).max(0.03 * 0.9).max(scale * 0.6);
        let py = (0.01f64).max(0.03 * 0.35);
        let base_x = e.anchor_x - 0.5 * scale;
        let depth_x = base_x + gap;
        let length = crate::profile_surface::width(&e.description, 0.03)
            .max(crate::profile_surface::width(&e.depth_text, 0.03));
        let xshifts = shifts(entries.len() * 2 + 2, 12, 28, step);
        let mut other = Vec::with_capacity(entries.len() * 2);
        for &j in &order {
            if j != index {
                let o = &entries[j];
                other.push(expanded(connector(o), clearance));
                let bx = o.band_x.unwrap_or(o.anchor_x);
                if (bx - o.anchor_x).abs() > 1e-6 {
                    other.push(expanded(
                        [o.anchor_x, o.band_connector_y, bx, o.band_y],
                        clearance,
                    ));
                }
            }
        }
        let mut hard_scores = vec![0.; xshifts.len() * yshifts.len()];
        let mut best: Option<(f64, f64, Box4, f64)> = None;
        let mut selected = None;
        'passes: for allow_soft in [false, true] {
            for (yi, ys) in yshifts.iter().enumerate() {
                crate::input::check(cancel)?;
                for (xi, xs) in xshifts.iter().enumerate() {
                    let x = base_x + xs;
                    let dx = depth_x + xs;
                    let y = e.leader_top_y + ys;
                    let box4 = [x.min(dx) - px, y - py, x.max(dx) + px, y + length + py];
                    let line_boxes = [
                        expanded(
                            [
                                e.anchor_x,
                                e.leader_line_start_y,
                                e.anchor_x,
                                e.band_connector_y,
                            ],
                            clearance,
                        ),
                        expanded(
                            [e.anchor_x, e.leader_block_start_y, x + 0.5 * scale, y],
                            clearance,
                        ),
                    ];
                    finite(&box4)?;
                    for b in line_boxes {
                        finite(&b)?;
                    }
                    let key = yi * xshifts.len() + xi;
                    let mut score = if allow_soft {
                        hard_scores[key]
                    } else {
                        let mut score = 0.;
                        if let Some(min) = options.min_x {
                            if box4[0] < min {
                                score += (min - box4[0]) * 5000.;
                            }
                        }
                        if let Some(max) = options.max_x {
                            if box4[2] > max {
                                score += (box4[2] - max) * 5000.;
                            }
                        }
                        score += penalty(box4, &placed_boxes, 25000.);
                        score += penalty(box4, options.text, 25000.);
                        score += penalty(box4, &hard, 20000.);
                        score += lines_penalty(line_boxes, options.text, 20000.);
                        score += lines_penalty(line_boxes, &hard, 18000.);
                        score += penalty(box4, &other, 22000.);
                        score += lines_penalty(line_boxes, &other, 22000.);
                        hard_scores[key] = score;
                        score
                    };
                    if !allow_soft {
                        score += penalty(box4, &soft, 8000.);
                        score += lines_penalty(line_boxes, &soft, 6000.);
                    }
                    score += xs.abs() * 20.;
                    score += ys.abs() * 40.;
                    if !allow_soft {
                        score -= 1.;
                    }
                    finite(&[score])?;
                    if best.as_ref().is_none_or(|b| score < b.3) {
                        best = Some((x, y, box4, score));
                    }
                    if score > 1e-9 {
                        continue;
                    }
                    selected = Some((x, y, box4));
                    break 'passes;
                }
            }
        }
        let (x, y, b) = selected
            .or_else(|| best.map(|(x, y, b, _)| (x, y, b)))
            .ok_or("Geen profielverwijzingskandidaat")?;
        let insert = [x + 0.5 * scale, y];
        finite(&insert)?;
        output.push(Placed {
            entry_index: index,
            text_insert: insert,
            marker_scale: scale,
        });
        placed_boxes.push(b);
        hard.push(expanded(connector(e), clearance));
        hard.push(expanded(
            [e.anchor_x, e.leader_block_start_y, insert[0], y],
            clearance,
        ));
    }
    Ok(output)
}
pub fn prepare(document: &mut Document, cancel: &AtomicBool) -> Result<(), String> {
    let source: template_geometry::AttributeBlockSource =
        serde_json::from_str(include_str!("data/profile-leader-annotation.json"))
            .map_err(|e| e.to_string())?;
    template_geometry::declare_attribute_block(document, BLOCK, source, cancel)
}
/// Append the active annotation INSERT, attached native LEADER and band line in
/// original order. Annotation indices refer to this complete entity batch.
pub fn append(
    output: &mut Vec<Entity>,
    entries: &[Entry],
    placed: &[Placed],
    cancel: &AtomicBool,
) -> Result<(), String> {
    if placed.len() > 25_000 || output.len().saturating_add(placed.len() * 3) > 100_000 {
        return Err("Te veel geplaatste profielverwijzingen".into());
    }
    let mut batch = Vec::with_capacity(placed.len() * 3);
    let mut text_bytes = 0usize;
    for p in placed {
        crate::input::check(cancel)?;
        let e = entries
            .get(p.entry_index)
            .ok_or("Profielverwijzingsindex ontbreekt")?;
        text_bytes = text_bytes
            .checked_add(e.description.len())
            .and_then(|n| n.checked_add(e.depth_text.len()))
            .filter(|n| *n <= 128 * 1024 * 1024)
            .ok_or("Te veel profielverwijzingstekst")?;
        finite(&[p.text_insert[0], p.text_insert[1], p.marker_scale])?;
        if p.marker_scale < 0.005 || e.leader_color > 256 {
            return Err("Ongeldige geplaatste profielverwijzing".into());
        }
        let annotation = output.len() + batch.len();
        batch.push(Entity {
            layer: LAYER.into(),
            layout: None,
            paint: Paint {
                color: Some(e.leader_color),
                ..Default::default()
            },
            shape: Shape::Block {
                name: BLOCK.into(),
                insert: p.text_insert,
                scale: p.marker_scale,
                attributes: std::collections::BTreeMap::from([
                    ("OMSCHRIJVING".into(), e.description.clone()),
                    ("HOOGTE".into(), e.depth_text.clone()),
                ]),
                attribute_placements: Default::default(),
            },
        });
        batch.push(Entity {
            layer: LAYER.into(),
            layout: None,
            paint: Paint {
                color: Some(8),
                lineweight: Some(0),
                ..Default::default()
            },
            shape: Shape::Leader {
                vertices: [[e.anchor_x, e.leader_block_start_y], p.text_insert],
                annotation,
                block_color: if e.leader_color == 0 {
                    7
                } else {
                    e.leader_color
                },
            },
        });
        let c = connector(e);
        batch.push(Entity {
            layer: crate::profile_drawing::LAYER.into(),
            layout: None,
            paint: Paint {
                color: Some(8),
                lineweight: Some(0),
                ..Default::default()
            },
            shape: Shape::Polyline {
                vertices: vec![[c[0], c[1]], [c[2], c[3]]],
                closed: false,
            },
        });
    }
    crate::input::check(cancel)?;
    output.extend(batch);
    Ok(())
}
