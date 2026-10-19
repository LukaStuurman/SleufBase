//! Active v0.3.10 profile label search. Optional dynamic-donor patches are not
//! enabled by the production Python package and must not replace this contract.
use crate::{
    dxf_document::Document,
    template_geometry::{self, Entity, Paint, Shape},
};
use std::sync::atomic::AtomicBool;
pub const BLOCK: &str = "SAL-VERWIJZING_LEIDING_BOVENKANT_INFO_DIEPTE-SOD";
pub const LAYER: &str = "X-XX-AL-VERWIJZING-SD";
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
