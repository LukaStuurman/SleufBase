//! Ordinary DXF MTEXT: retain source, prepare native vector contours and allow
//! parent block transforms. Inline color/background and MTEXT-in-ATTRIB remain
//! separate parity tasks.
use crate::{
    cad_geometry::{self as geo, Affine, V3},
    cad_text_path::{Path, MAX_CONTROLS},
    dxf_entities::{field, number, point},
    dxf_read::{DxfFeature, DxfOverlay},
    dxf_tags::Group,
    mtext::{Context, Paragraph},
    mtext_render::{self, Draw, EstimateRequest, Request, SimpleRequest},
};
use serde::{Deserialize, Serialize};
use std::sync::{atomic::AtomicBool, Arc};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub request: Request,
    pub style: String,
    #[serde(default)]
    pub contours: Path,
    /// Older previews stored only the complex request and cannot recover the
    /// original zero width, rotation presence and simple-route placement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<Entity>,
    /// Preserve the R2018 embedded-column object for subsequent native support.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<crate::cad::Tag>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entity {
    pub width: f64,
    pub rotation: f64,
    pub normal: V3,
    pub direction: Option<V3>,
}
impl Source {
    pub fn validate(&self) -> Result<(), String> {
        let r = &self.request;
        if r.content.len() > 1_048_576
            || self.style.len() > 32_768
            || !r.width.is_finite()
            || r.width <= 0.
            || !r.spacing.is_finite()
            || r.spacing <= 0.
            || !(1..=9).contains(&r.align)
            || r.columns.len() > 128
            || r.at.iter().any(|v| !v.is_finite())
            || r.matrix.iter().flatten().any(|v| !v.is_finite())
        {
            return Err("Ongeldige opgeslagen CAD-MTEXT".into());
        }
        r.initial.validate().map_err(|e| e.to_string())?;
        if self.extra.len() > 100_000
            || self
                .extra
                .iter()
                .any(|t| !(0..=1071).contains(&t.code) || t.value.len() > 1_048_576)
        {
            return Err("Ongeldige ingebouwde MTEXT-brongegevens".into());
        }
        if let Some(e) = &self.entity {
            if !e.width.is_finite() || e.width < 0. || !e.rotation.is_finite() {
                return Err("Ongeldige MTEXT-bronplaatsing".into());
            }
            Affine::ocs(e.normal)?;
            if let Some(d) = e.direction {
                unit(d)?;
            }
        }
        self.contours.validate()
    }
}
fn unit(v: V3) -> Result<V3, String> {
    geo::finite(v)?;
    let length = geo::length(v);
    if length == 0. || !length.is_finite() {
        return Err("Ongeldige MTEXT-tekstrichting".into());
    }
    Ok(v.map(|v| v / length))
}
fn extrusion(groups: &[Group<'_>]) -> V3 {
    let v = [
        number(groups, 210).unwrap_or(0.),
        number(groups, 220).unwrap_or(0.),
        number(groups, 230).unwrap_or(1.),
    ];
    if v == [0.; 3] {
        [0., 0., 1.]
    } else {
        v
    }
}
/// DXF insertion group 10 and explicit x-axis 11 are WCS, never OCS points.
fn matrix(groups: &[Group<'_>]) -> Result<[[f64; 4]; 3], String> {
    let insert = point(groups, 10)?;
    let ocs = Affine::ocs(extrusion(groups))?;
    let z = ocs.vector([0., 0., 1.]);
    let x = if field(groups, 11).is_some() {
        unit(point(groups, 11)?)?
    } else if field(groups, 50).is_some() {
        let angle = number(groups, 50).unwrap_or(0.).to_radians();
        let (s, c) = angle.sin_cos();
        unit(ocs.vector([c, s, 0.]))?
    } else {
        [1., 0., 0.]
    };
    let y = unit(geo::cross(z, x))?;
    Ok(std::array::from_fn(|i| [x[i], y[i], z[i], insert[i]]))
}
pub(crate) fn parse(groups: &[Group<'_>], feature: &mut DxfFeature) -> Result<(), String> {
    let all = groups;
    let groups = crate::dxf_entities::subclass(groups, "AcDbMText");
    let mut content = String::new();
    // Group 1 is the tail, even when unusual DXF input lists it before group 3.
    for tag in groups
        .iter()
        .filter(|g| g.code == 3)
        .chain(groups.iter().rev().find(|g| g.code == 1))
    {
        if content.len().saturating_add(tag.value.len()) > 1_048_576 {
            return Err("MTEXT bevat meer dan 1 MiB tekst".into());
        }
        content.push_str(&tag.value);
    }
    let declared_height = number(groups, 40).unwrap_or(2.5);
    if !declared_height.is_finite() {
        return Err("Ongeldige MTEXT-teksthoogte".into());
    }
    let height = if declared_height > 1e-9 {
        declared_height
    } else {
        2.5
    };
    let fixed_width = number(groups, 41).unwrap_or(0.);
    if fixed_width < 0. {
        return Err("Ongeldige MTEXT-kaderbreedte".into());
    }
    // A temporary positive width is replaced by real font estimation before
    // complex layout. The simple route retains zero as unrestricted wrapping.
    let width = if fixed_width > 0. { fixed_width } else { 1. };
    let mut initial = Context {
        cap_height: height,
        ..Context::default()
    };
    initial.aci = if (1..=255).contains(&feature.color_aci) {
        feature.color_aci as u16
    } else {
        7
    };
    let request = Request {
        content,
        initial,
        width,
        columns: Vec::new(),
        spacing: number(groups, 44)
            .filter(|s| (0.25..=4.).contains(s))
            .unwrap_or(1.),
        align: u8::try_from(crate::dxf_entities::integer(groups, 71, 1)?)
            .map_err(|_| "Ongeldig MTEXT-ankerpunt")?,
        at: [0., 0.],
        matrix: matrix(groups)?,
    };
    let source = Source {
        request,
        style: field(groups, 7).unwrap_or("Standard").to_owned(),
        contours: Path::default(),
        entity: Some(Entity {
            width: fixed_width,
            rotation: number(groups, 50).unwrap_or(0.),
            normal: extrusion(groups),
            direction: field(groups, 11).map(|_| point(groups, 11)).transpose()?,
        }),
        extra: all
            .iter()
            .position(|g| g.code == 101)
            .map_or_else(Vec::new, |at| {
                all[at..]
                    .iter()
                    .map(|g| crate::cad::Tag {
                        code: g.code,
                        value: g.value.to_string(),
                    })
                    .collect()
            }),
    };
    source.validate()?;
    feature.raw_text = source.request.content.clone();
    feature.cad.get_or_insert_with(Default::default).mtext = Some(Box::new(source));
    Ok(())
}
pub(crate) fn prepare_overlay(
    overlay: &mut DxfOverlay,
    styles: &std::collections::BTreeMap<String, crate::cad_text::Style>,
    catalog: Option<&mut crate::dxf_fonts::Catalog>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    if !overlay
        .source_features()
        .any(|f| f.cad.as_deref().is_some_and(|p| p.mtext.is_some()))
    {
        return Ok(());
    }
    let mut system;
    let catalog = if let Some(c) = catalog {
        c
    } else {
        let extra: Vec<_> = overlay
            .path
            .parent()
            .filter(|p| !p.as_os_str().is_empty() && p.is_dir())
            .map(|p| p.to_owned())
            .into_iter()
            .collect();
        system = crate::dxf_fonts::Catalog::system_with_dirs(&extra, cancel)?;
        &mut system
    };
    let mut renderer = mtext_render::Renderer::new(catalog, cancel);
    let mut points = 0usize;
    fn prepare_one(
        feature: &mut DxfFeature,
        styles: &std::collections::BTreeMap<String, crate::cad_text::Style>,
        renderer: &mut mtext_render::Renderer<'_>,
        total: &mut usize,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        crate::input::check(cancel)?;
        let Some(cad) = feature.cad.as_deref_mut() else {
            return Ok(());
        };
        let Some(source) = cad.mtext.as_deref_mut() else {
            return Ok(());
        };
        let default = crate::cad_text::Style {
            font: "txt".into(),
            ..Default::default()
        };
        let style = styles.get(&source.style.to_lowercase()).unwrap_or(&default);
        source.request.initial.font_face = renderer.style_face(&style.font)?;
        source.request.initial.width_factor = style.width;
        source.request.initial.oblique = style.oblique;
        source.request.initial.paragraph = Arc::new(Paragraph {
            align: match (source.request.align - 1) % 3 {
                1 => 3,
                2 => 2,
                _ => 1,
            },
            ..Default::default()
        });
        source.validate()?;
        let mut path = Path::default();
        // Keep ordinary MTEXT on the original simple route. Nonbreaking
        // spaces must use real NBSP cells: the old simple decoder can otherwise
        // display the literal code despite the frontend treating it as simple.
        let simple = source.entity.as_ref().filter(|_| {
            source.request.columns.is_empty()
                && style.width == 1.
                && style.oblique == 0.
                && !source.request.content.contains("\\~")
                && !source.request.content.replace("\\P", "").contains('\\')
        });
        if let Some(entity) = simple {
            let r = &source.request;
            let output = renderer.render_simple(SimpleRequest {
                content: r.content.clone(),
                face: r.initial.font_face.clone(),
                height: r.initial.cap_height,
                width: entity.width,
                spacing: r.spacing,
                align: r.align,
                insert: [r.matrix[0][3], r.matrix[1][3], r.matrix[2][3]],
                rotation: entity.rotation,
                normal: entity.normal,
                direction: entity.direction,
            })?;
            for line in output.lines {
                path.append(&line.contours, Affine::IDENTITY, MAX_CONTROLS)?;
            }
        } else {
            if let Some(entity) = source.entity.as_ref().filter(|e| e.width < 1e-6) {
                let extent = renderer.estimate(EstimateRequest {
                    content: source.request.content.clone(),
                    font: style.font.clone(),
                    height: source.request.initial.cap_height,
                    width: entity.width,
                    spacing: source.request.spacing,
                })?;
                // The original complex layouter rejects a zero-width paragraph.
                // Keep its source/empty ink rather than publishing guessed width.
                if extent.width == 0. {
                    source.contours = path;
                    feature.points.clear();
                    cad.path_breaks.clear();
                    return Ok(());
                }
                source.request.width = extent.width;
            }
            let output = renderer.render(source.request.clone())?;
            for draw in output.draws {
                crate::input::check(cancel)?;
                match draw {
                    Draw::Text { contours, .. } => {
                        path.append(&contours, Affine::IDENTITY, MAX_CONTROLS)?;
                    }
                    Draw::Line { start, end, .. } => {
                        path.push(0, &[start], MAX_CONTROLS)?;
                        path.push(1, &[end], MAX_CONTROLS)?;
                    }
                }
            }
        }
        let flat = path.flatten(
            Affine::IDENTITY,
            0.005,
            10_000_000usize.saturating_sub(*total),
            cancel,
        )?;
        *total = total
            .checked_add(flat.points.len())
            .filter(|n| *n <= 10_000_000)
            .ok_or("Te veel CAD-MTEXT-contourpunten")?;
        source.contours = path;
        feature.points = flat.points;
        cad.path_breaks = flat.breaks;
        Ok(())
    }
    for feature in overlay.features.iter_mut().chain(
        overlay
            .blocks
            .values_mut()
            .flat_map(|b| Arc::make_mut(b).features.iter_mut()),
    ) {
        prepare_one(feature, styles, &mut renderer, &mut points, cancel)?;
    }
    overlay.warnings.push(
        "MTEXT: vectorcontouren zichtbaar; inline kleuren, achtergrond, kolom-/embedded-bronnen en vlakvulling nog niet volledig ondersteund".into()
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn tags(text: &str) -> Vec<Group<'_>> {
        crate::dxf_tags::ascii(text).map(Result::unwrap).collect()
    }
    fn feature() -> DxfFeature {
        DxfFeature {
            entity_type: "MTEXT".into(),
            handle: "ABC".into(),
            layer: "0".into(),
            color_aci: 7,
            points: Vec::new(),
            raw_text: String::new(),
            block_name: String::new(),
            closed: None,
            cad: None,
        }
    }
    #[test]
    fn chunked_content_and_wcs_rotation_survive_serialization() {
        let tags = tags(
            "10\n100\n20\n200\n30\n12\n40\n2.5\n41\n50\n50\n90\n71\n5\n3\nA\\P\n1\nB\\Ctest;\n",
        );
        let mut f = feature();
        parse(&tags, &mut f).unwrap();
        let source = f.cad.as_ref().unwrap().mtext.as_ref().unwrap();
        assert_eq!(source.request.content, "A\\PB\\Ctest;");
        assert_eq!(source.request.align, 5);
        let m = source.request.matrix;
        assert!(m[0][0].abs() < 1e-12);
        assert!((m[1][0] - 1.).abs() < 1e-12);
        assert_eq!([m[0][3], m[1][3], m[2][3]], [100., 200., 12.]);
        let rebuilt: Source =
            serde_json::from_str(&serde_json::to_string(source).unwrap()).unwrap();
        rebuilt.validate().unwrap();
        assert_eq!(rebuilt.request.content, source.request.content);
    }
    #[test]
    fn explicit_direction_and_bad_axis() {
        let mut f = feature();
        parse(
            &tags("10\n25\n20\n40\n11\n0\n21\n-1\n31\n0\n1\nText\n"),
            &mut f,
        )
        .unwrap();
        assert_eq!(f.cad.unwrap().mtext.unwrap().request.matrix[1][0], -1.);
        let mut invalid = feature();
        assert!(parse(
            &tags("10\n25\n20\n40\n11\n0\n21\n0\n31\n1\n1\nText\n"),
            &mut invalid
        )
        .is_err());
        assert!(invalid.cad.is_none());
    }
    #[test]
    fn subclass_tail_order_tiny_direction_and_embedded_source_are_preserved() {
        let mut f = feature();
        let data=tags("100\nAcDbEntity\n1\nignored\n100\nAcDbMText\n10\n0\n20\n0\n11\n0.00000000000000000001\n21\n0\n1\ntail\n3\nhead\n101\nEmbedded Object\n1\nembedded\n");
        parse(&data, &mut f).unwrap();
        let source = f.cad.unwrap().mtext.unwrap();
        assert_eq!(source.request.content, "headtail");
        assert_eq!(source.request.matrix[0][0], 1.);
        assert_eq!(source.extra.last().unwrap().value, "embedded");
        assert_eq!(source.entity.unwrap().direction, Some([1e-20, 0., 0.]));
    }
}
