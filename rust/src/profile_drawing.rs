//! Original profile frame, scale selection and paired chainage/height bands.
//! Terrain, markers, reference indicators and dynamic leaders are separate
//! drawing stages; this plan retains their anchors for subsequent composition.
use crate::{
    dxf_document::Document,
    profile::{self, Profile},
    template_geometry::{self, AttributeMetrics, Entity, Paint, Shape, TextPlacement},
    Bounds, Dataset, Geometry,
};
use std::{collections::BTreeMap, sync::atomic::AtomicBool};
pub const LAYER: &str = "X-XX-AL-PROFIELBALK-SD";
const LEFT: &str = "SAL-VERWIJZING_PROFIELRASTER_TITEL_LINKS-SOD";
const RIGHT: &str = "SAL-VERWIJZING_PROFIELRASTER_TITEL_RECHTS-SOD";
const REFERENCE: &str = "SAL-VERWIJZING_PROFIELRASTER_HOOGTEREFERENTIE_ONDER-SOD";
const BAND_LEFT: &str = "SAL-VERWIJZING_PROFIELRASTER_BAND_TITEL_LINKS-SOD";
const BAND_RIGHT: &str = "SAL-VERWIJZING_PROFIELRASTER_BAND_TITEL_RECHTS-SOD";
const VALUE: &str = "SAL-VERWIJZING_PROFIELRASTER_BAND_WAARDE-SOD";
pub struct Templates {
    band: Option<AttributeMetrics>,
}
impl Templates {
    pub fn read(document: &Document, cancel: &AtomicBool) -> Result<Self, String> {
        for name in [LEFT, RIGHT, REFERENCE, BAND_LEFT, BAND_RIGHT] {
            template_geometry::block_attribute_metrics(document, name, "", 0.02, cancel)?;
        }
        Ok(Self {
            band: template_geometry::block_attribute_metrics(
                document, VALUE, "WAARDE", 0.02, cancel,
            )?,
        })
    }
}
#[derive(Clone, Copy)]
pub struct Options<'a> {
    pub label: &'a str,
    pub manual_scale: Option<i64>,
    pub marker_scale: f64,
    pub reference_chainage: f64,
    pub distance_description: &'a str,
}
impl Default for Options<'_> {
    fn default() -> Self {
        Self {
            label: "PS1",
            manual_scale: None,
            marker_scale: 0.02,
            reference_chainage: 0.,
            distance_description: "",
        }
    }
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct Layout {
    pub start_x: f64,
    pub end_x: f64,
    pub reference_y: f64,
    pub title_y: f64,
    pub top_band_y: f64,
    pub bottom_band_y: f64,
    pub bottom_y: f64,
    pub denominator: u32,
    pub scale: f64,
}
#[derive(Debug, serde::Serialize)]
pub struct Band {
    pub point_index: usize,
    pub feature_index: usize,
    pub anchor_x: f64,
    pub placed_x: f64,
    pub chainage: f64,
    pub point_z: f64,
    pub pinned: bool,
    pub marker_scale: f64,
}
pub struct Drawing {
    pub layout: Layout,
    pub entities: Vec<Entity>,
    pub bands: Vec<Band>,
}
fn finite(values: &[f64]) -> Result<(), String> {
    if values.iter().any(|v| !v.is_finite()) {
        Err("Niet-eindige profielindeling".into())
    } else {
        Ok(())
    }
}
fn bounds(b: Bounds) -> Result<(), String> {
    finite(&[b.min_x, b.min_y, b.max_x, b.max_y])?;
    if b.min_x >= b.max_x || b.min_y >= b.max_y {
        Err("Ongeldige profielbegrenzing".into())
    } else {
        Ok(())
    }
}
pub fn names(label: &str) -> Result<(String, String), String> {
    if label.len() > 4096 || label.chars().any(char::is_control) {
        return Err("Ongeldig profielnummer".into());
    }
    let mut chars = label.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(n) = crate::object_layout::decimal(c) {
            let mut digits = String::from(char::from(b'0' + n as u8));
            while let Some(n) = chars.peek().and_then(|c| crate::object_layout::decimal(*c)) {
                digits.push(char::from(b'0' + n as u8));
                chars.next();
            }
            let normalized = digits.trim_start_matches('0');
            let normalized = if normalized.is_empty() {
                "0"
            } else {
                normalized
            };
            let suffix: String = chars
                .take_while(|c| c.is_ascii_alphabetic() || matches!(c, 'İ' | 'ı' | 'ſ' | 'K'))
                .collect();
            let suffix = crate::python_values::uppercase(&suffix);
            return Ok((
                format!("DP{normalized}{suffix}"),
                format!("PS {normalized}{suffix}"),
            ));
        }
    }
    Ok((label.replacen("PS", "DP", 1), label.into()))
}
pub fn layout(
    profile: &Profile<'_>,
    dataset: &Dataset,
    row: Bounds,
    tiff: Bounds,
    manual: Option<i64>,
    cancel: &AtomicBool,
) -> Result<Option<Layout>, String> {
    crate::input::check(cancel)?;
    bounds(row)?;
    bounds(tiff)?;
    finite(&[
        profile.axis_length,
        profile.reference_level,
        profile.start.z.unwrap_or(0.),
        profile.end.z.unwrap_or(0.),
    ])?;
    if profile.axis_length <= 1e-9 || profile.points.len() > 100_000 {
        return Err("Ongeldige profielas of te veel profielpunten".into());
    }
    let start_x = tiff.min_x - 6.7895;
    let reference_y = tiff.min_y - 0.4283;
    let available = tiff.min_x - 1.705 - start_x;
    if available <= 1e-6 {
        return Ok(None);
    }
    let mut max_z = profile
        .start
        .z
        .filter(|z| *z != 0.)
        .unwrap_or(profile.reference_level)
        .max(
            profile
                .end
                .z
                .filter(|z| *z != 0.)
                .unwrap_or(profile.reference_level),
        );
    let mut vertices = 0usize;
    for point in &profile.points {
        crate::input::check(cancel)?;
        finite(&[point.point_z, point.chainage])?;
        max_z = max_z.max(point.point_z);
        let feature = dataset
            .features
            .get(point.feature_index)
            .ok_or("Profiel verwijst naar ontbrekend bronobject")?;
        if let Geometry::Polyline(polyline) = &feature.geometry {
            vertices = vertices
                .checked_add(polyline.len())
                .filter(|n| *n <= 10_000_000)
                .ok_or("Te veel profielvertices")?;
            for (i, v) in polyline.iter().enumerate() {
                if i.is_multiple_of(1024) {
                    crate::input::check(cancel)?;
                }
                if let Some(z) = v.z {
                    finite(&[z])?;
                    max_z = max_z.max(z);
                }
            }
        }
    }
    let height = (max_z - profile.reference_level).max(0.);
    let available_height = row.max_y - 0.08 - reference_y;
    finite(&[height, available_height, available])?;
    let vertical = if height <= 1e-9 || available_height <= 1e-9 {
        1.
    } else {
        (available_height / height).clamp(0.01, 1.)
    };
    let horizontal = (available / profile.axis_length.max(1e-9)).min(1.);
    let denominator = manual.map_or_else(
        || ((20. / horizontal.min(vertical).max(1e-6)).max(20.) / 20.).ceil() as u32 * 20,
        |n| n.clamp(20, 5000) as u32,
    );
    let scale = 20. / f64::from(denominator);
    let result = Layout {
        start_x,
        end_x: start_x + profile.axis_length * scale,
        reference_y,
        title_y: tiff.min_y - 0.4683,
        top_band_y: tiff.min_y - 0.4883,
        bottom_band_y: tiff.min_y - 0.6883,
        bottom_y: tiff.min_y - 0.8883,
        denominator,
        scale,
    };
    finite(&[
        result.start_x,
        result.end_x,
        result.reference_y,
        result.title_y,
        result.top_band_y,
        result.bottom_band_y,
        result.bottom_y,
        result.scale,
    ])?;
    Ok(Some(result))
}
fn block(name: &str, point: [f64; 2], values: &[(&str, String)]) -> Entity {
    Entity {
        layer: LAYER.into(),
        paint: Paint::default(),
        layout: None,
        shape: Shape::Block {
            name: name.into(),
            insert: point,
            scale: 0.02,
            attributes: values
                .iter()
                .map(|(k, v)| ((*k).into(), v.clone()))
                .collect(),
            attribute_placements: BTreeMap::new(),
        },
    }
}
fn line(a: [f64; 2], b: [f64; 2], color: Option<u16>) -> Entity {
    Entity {
        layer: LAYER.into(),
        paint: Paint {
            color,
            true_color: None,
            lineweight: Some(0),
        },
        layout: None,
        shape: Shape::Polyline {
            vertices: vec![a, b],
            closed: false,
        },
    }
}
pub fn format_distance(value: f64) -> Result<String, String> {
    finite(&[value])?;
    let text = format!("{value:.2}");
    Ok(if text == "-0.00" { "0.00".into() } else { text })
}
pub fn distribute(
    bands: &mut [Band],
    width: f64,
    min_x: f64,
    max_x: f64,
    cancel: &AtomicBool,
) -> Result<(), String> {
    place_bands(bands, width, min_x, max_x, cancel).map(|_| ())
}
fn place_bands(
    bands: &mut [Band],
    width: f64,
    min_x: f64,
    max_x: f64,
    cancel: &AtomicBool,
) -> Result<Vec<usize>, String> {
    crate::input::check(cancel)?;
    finite(&[width, min_x, max_x])?;
    if width <= 0. || min_x > max_x || bands.len() > 25_000 {
        return Err("Ongeldige bandbegrenzing of te veel bandwaarden".into());
    }
    for b in bands.iter() {
        finite(&[b.anchor_x])?;
    }
    // Stable index ordering preserves equal-anchor order without moving source
    // features or duplicating their geometry.
    let mut order: Vec<_> = (0..bands.len()).collect();
    order.sort_by(|a, b| bands[*a].anchor_x.partial_cmp(&bands[*b].anchor_x).unwrap());
    let clamp = |index: usize, value: f64| {
        if bands[index].pinned {
            bands[index].anchor_x
        } else {
            value.clamp(
                min_x + width * 0.5,
                (max_x - width * 0.5).max(min_x + width * 0.5),
            )
        }
    };
    let mut positions: Vec<_> = order
        .iter()
        .map(|i| clamp(*i, bands[*i].anchor_x))
        .collect();
    let gap = width * 0.5 + width * 0.5 + (0.03_f64 * 0.48).max(0.007);
    finite(&[min_x + width * 0.5, max_x - width * 0.5, gap])?;
    for _ in 0..3 {
        crate::input::check(cancel)?;
        for i in 1..positions.len() {
            positions[i] = clamp(order[i], positions[i].max(positions[i - 1] + gap));
        }
        for i in (0..positions.len().saturating_sub(1)).rev() {
            positions[i] = clamp(order[i], positions[i].min(positions[i + 1] - gap));
        }
    }
    crate::input::check(cancel)?;
    finite(&positions)?;
    for (&i, position) in order.iter().zip(positions) {
        bands[i].placed_x = position;
    }
    Ok(order)
}
/// Prepare the frame and paired bands; the caller composes remaining stages and
/// appends the collected geometry only after all stages have succeeded.
pub fn frame_and_bands(
    profile: &Profile<'_>,
    dataset: &Dataset,
    boxes: [Bounds; 2],
    templates: &Templates,
    options: Options<'_>,
    cancel: &AtomicBool,
) -> Result<Option<Drawing>, String> {
    finite(&[options.marker_scale, options.reference_chainage])?;
    let Some(layout) = layout(
        profile,
        dataset,
        boxes[0],
        boxes[1],
        options.manual_scale,
        cancel,
    )?
    else {
        return Ok(None);
    };
    let (number, description) = names(options.label)?;
    if options.distance_description.len() > 4096
        || options.distance_description.chars().any(char::is_control)
    {
        return Err("Ongeldige referentieomschrijving".into());
    }
    let mut entities = vec![
        block(
            LEFT,
            [layout.start_x, layout.title_y],
            &[
                ("NUMMER", number),
                ("GROEP", "Proefsleuven".into()),
                ("OMSCHRIJVING", description),
                ("SCHAAL", format!("Schaal: 1:{}", layout.denominator)),
            ],
        ),
        block(RIGHT, [layout.end_x, layout.title_y], &[]),
        block(
            REFERENCE,
            [layout.start_x, layout.reference_y],
            &[(
                "REFERENTIE",
                format!("{:.2} N.A.P.", profile.reference_level),
            )],
        ),
    ];
    for (y, title, description) in [
        (layout.top_band_y, "Afstand", options.distance_description),
        (layout.bottom_band_y, "N.A.P.", ""),
    ] {
        entities.push(block(
            BAND_LEFT,
            [layout.start_x, y],
            &[
                ("TITEL", title.into()),
                ("OMSCHRIJVING", description.into()),
            ],
        ));
        entities.push(block(
            BAND_RIGHT,
            [layout.end_x, y],
            &[("TITEL", title.into()), ("OMSCHRIJVING", String::new())],
        ));
    }
    for y in [
        layout.reference_y,
        layout.top_band_y,
        layout.bottom_band_y,
        layout.bottom_y,
    ] {
        entities.push(line([layout.start_x, y], [layout.end_x, y], None));
    }
    let marker_scale = options.marker_scale.max(0.005);
    let mut bands = Vec::new();
    for (point_index, point) in profile.points.iter().enumerate() {
        crate::input::check(cancel)?;
        let feature = &dataset.features[point.feature_index];
        let is_line = matches!(feature.geometry, Geometry::Polyline(_));
        let scale = if point.is_endpoint || is_line {
            marker_scale
        } else {
            profile::marker_scale(feature, marker_scale)
        };
        let offsets = if point.is_endpoint || is_line {
            vec![0.]
        } else {
            profile::marker_offsets(feature, scale)
        };
        for offset in offsets {
            if bands.len() >= 25_000 {
                return Err("Te veel profielbandwaarden".into());
            }
            let anchor_x = layout.start_x + point.chainage * layout.scale + offset;
            let y = layout.reference_y + (point.point_z - profile.reference_level) * layout.scale;
            finite(&[anchor_x, y])?;
            bands.push(Band {
                point_index,
                feature_index: point.feature_index,
                anchor_x,
                placed_x: anchor_x,
                chainage: point.chainage,
                point_z: point.point_z,
                pinned: point.is_endpoint,
                marker_scale: scale,
            });
            entities.push(line([anchor_x, layout.reference_y], [anchor_x, y], None));
            for (band_y, value) in [
                (
                    layout.top_band_y,
                    point.chainage - options.reference_chainage,
                ),
                (layout.bottom_band_y, point.point_z),
            ] {
                finite(&[value])?;
                let text = if band_y == layout.top_band_y {
                    format_distance(value)?
                } else {
                    format!("{value:.2}")
                };
                entities.push(block(VALUE, [anchor_x, band_y], &[("WAARDE", text)]));
            }
        }
    }
    if let Some(metrics) = templates.band {
        let order = place_bands(
            &mut bands,
            metrics.height * 0.92,
            layout.start_x,
            layout.end_x,
            cancel,
        )?;
        for (i, band) in bands.iter().enumerate() {
            for (j, y) in [(1, layout.top_band_y), (2, layout.bottom_band_y)] {
                if let Shape::Block {
                    insert,
                    attribute_placements,
                    ..
                } = &mut entities[11 + i * 3 + j].shape
                {
                    *insert = [band.placed_x, y];
                    attribute_placements.insert(
                        "WAARDE".into(),
                        TextPlacement {
                            insert: [band.placed_x + metrics.offset[0], y + metrics.offset[1]],
                            height: metrics.height,
                            rotation: 90.,
                        },
                    );
                }
            }
        }
        // Python appends connectors in stable anchor order after placing labels.
        for i in order {
            let b = &bands[i];
            if (b.placed_x - b.anchor_x).abs() > 1e-6 {
                entities.push(line(
                    [b.anchor_x, layout.top_band_y + 0.04],
                    [b.placed_x, layout.top_band_y],
                    Some(8),
                ));
            }
        }
    }
    crate::input::check(cancel)?;
    Ok(Some(Drawing {
        layout,
        entities,
        bands,
    }))
}
