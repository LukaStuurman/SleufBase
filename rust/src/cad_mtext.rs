//! Ordinary DXF MTEXT: retain source, prepare native vector contours and allow
//! parent block transforms. Inline color/background and MTEXT-in-ATTRIB remain
//! separate parity tasks.
use crate::{
    cad_geometry::{self as geo, Affine, V3},
    cad_text_path::{Path, MAX_CONTROLS},
    dxf_entities::{field, number, point},
    dxf_read::{DxfFeature, DxfOverlay},
    dxf_tags::Group,
    mtext::{Context, FontFace},
    mtext_render::{self, Draw, Request},
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
}
impl Source {
    pub fn validate(&self) -> Result<(), String> {
        let r = &self.request;
        if r.content.len() > 1_048_576 || self.style.len() > 32_768
            || !r.width.is_finite() || r.width <= 0.
            || !r.spacing.is_finite() || r.spacing <= 0.
            || !(1..=9).contains(&r.align)
            || r.columns.len() > 128
            || r.at.iter().any(|v| !v.is_finite())
            || r.matrix.iter().flatten().any(|v| !v.is_finite())
        {
            return Err("Ongeldige opgeslagen CAD-MTEXT".into());
        }
        r.initial.validate().map_err(|e| e.to_string())?;
        self.contours.validate()
    }
}
fn unit(v: V3) -> Result<V3, String> {
    geo::finite(v)?;
    let length = geo::length(v);
    if length <= 1e-12 || !length.is_finite() {
        return Err("Ongeldige MTEXT-tekstrichting".into());
    }
    Ok(v.map(|v| v / length))
}
/// DXF insertion group 10 and explicit x-axis 11 are WCS, never OCS points.
fn matrix(groups: &[Group<'_>]) -> Result<[[f64; 4]; 3], String> {
    let insert = point(groups, 10)?;
    let ocs = Affine::ocs([
        number(groups, 210).unwrap_or(0.),
        number(groups, 220).unwrap_or(0.),
        number(groups, 230).unwrap_or(1.),
    ])?;
    let z = ocs.vector([0., 0., 1.]);
    let x = if field(groups, 11).is_some() {
        unit(point(groups, 11)?)?
    } else {
        let angle = number(groups, 50).unwrap_or(0.).to_radians();
        let (s, c) = angle.sin_cos();
        unit(ocs.vector([c, s, 0.]))?
    };
    if geo::dot(x, z).abs() > 1e-6 {
        return Err("MTEXT X-as staat niet loodrecht op de normaal".into());
    }
    let y = unit(geo::cross(z, x))?;
    Ok(std::array::from_fn(|i| [x[i], y[i], z[i], insert[i]]))
}
pub(crate) fn parse(groups: &[Group<'_>], feature: &mut DxfFeature) -> Result<(), String> {
    let mut content = String::new();
    for tag in groups.iter().filter(|g| g.code == 3 || g.code == 1) {
        if content.len().saturating_add(tag.value.len()) > 1_048_576 {
            return Err("MTEXT bevat meer dan 1 MiB tekst".into());
        }
        content.push_str(&tag.value);
    }
    let height = number(groups, 40).unwrap_or(2.5);
    if !height.is_finite() || height <= 0. {
        return Err("Ongeldige MTEXT-teksthoogte".into());
    }
    let fixed_width = number(groups, 41).unwrap_or(0.);
    if fixed_width < 0. { return Err("Ongeldige MTEXT-kaderbreedte".into()); }
    // DXF width zero means no wrap; precise font-based auto width is still open.
    let width = if fixed_width > 0. { fixed_width } else {
        (height * 0.8 * content.chars().count().max(1) as f64).max(height)
    };
    let mut initial = Context { cap_height: height, ..Context::default() };
    initial.aci = if (1..=255).contains(&feature.color_aci) {
        feature.color_aci as u16
    } else { 7 };
    let request = Request {
        content, initial, width, columns: Vec::new(),
        spacing: number(groups, 44).unwrap_or(1.),
        align: u8::try_from(crate::dxf_entities::integer(groups, 71, 1)?)
            .map_err(|_| "Ongeldig MTEXT-ankerpunt")?,
        at: [0., 0.],
        matrix: matrix(groups)?,
    };
    let source = Source {
        request,
        style: field(groups, 7).unwrap_or("Standard").to_owned(),
        contours: Path::default(),
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
    if !overlay.source_features().any(|f| f.cad.as_deref().is_some_and(|p| p.mtext.is_some())) {
        return Ok(());
    }
    let mut system;
    let catalog = if let Some(c) = catalog { c } else {
        let extra: Vec<_> = overlay.path.parent()
            .filter(|p| !p.as_os_str().is_empty() && p.is_dir())
            .map(|p| p.to_owned()).into_iter().collect();
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
        let Some(cad) = feature.cad.as_deref_mut() else { return Ok(()); };
        let Some(source) = cad.mtext.as_deref_mut() else { return Ok(()); };
        if let Some(style) = styles.get(&source.style.to_lowercase()) {
            source.request.initial.font_face = Arc::new(FontFace {
                filename: Arc::from(style.font.as_str()), ..FontFace::default()
            });
        }
        source.validate()?;
        let output = renderer.render(source.request.clone())?;
        let mut path = Path::default();
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
        let flat = path.flatten(
            Affine::IDENTITY, 0.005, 10_000_000usize.saturating_sub(*total), cancel,
        )?;
        *total = total.checked_add(flat.points.len())
            .filter(|n| *n <= 10_000_000)
            .ok_or("Te veel CAD-MTEXT-contourpunten")?;
        source.contours = path;
        feature.points = flat.points;
        cad.path_breaks = flat.breaks;
        Ok(())
    }
    for feature in overlay.features.iter_mut().chain(
        overlay.blocks.values_mut().flat_map(|b| Arc::make_mut(b).features.iter_mut())
    ) {
        prepare_one(feature, styles, &mut renderer, &mut points, cancel)?;
    }
    overlay.warnings.push(
        "MTEXT: vectorcontouren zichtbaar; inline kleuren, achtergrond en automatische breedte nog niet volledig ondersteund".into()
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
            entity_type:"MTEXT".into(), handle:"ABC".into(), layer:"0".into(),
            color_aci:7, points:Vec::new(), raw_text:String::new(),
            block_name:String::new(), closed:None, cad:None,
        }
    }
    #[test]
    fn chunked_content_and_wcs_rotation_survive_serialization() {
        let tags=tags("10\n100\n20\n200\n30\n12\n40\n2.5\n41\n50\n50\n90\n71\n5\n3\nA\\P\n1\nB\\Ctest;\n");
        let mut f=feature();
        parse(&tags,&mut f).unwrap();
        let source=f.cad.as_ref().unwrap().mtext.as_ref().unwrap();
        assert_eq!(source.request.content,"A\\PB\\Ctest;");
        assert_eq!(source.request.align,5);
        let m=source.request.matrix;
        assert!(m[0][0].abs()<1e-12);
        assert!((m[1][0]-1.).abs()<1e-12);
        assert_eq!([m[0][3],m[1][3],m[2][3]],[100.,200.,12.]);
        let rebuilt:Source=serde_json::from_str(&serde_json::to_string(source).unwrap()).unwrap();
        rebuilt.validate().unwrap();
        assert_eq!(rebuilt.request.content,source.request.content);
    }
    #[test]
    fn explicit_direction_and_bad_axis() {
        let mut f=feature();
        parse(&tags("10\n25\n20\n40\n11\n0\n21\n-1\n31\n0\n1\nText\n"),&mut f).unwrap();
        assert_eq!(f.cad.unwrap().mtext.unwrap().request.matrix[1][0],-1.);
        let mut invalid=feature();
        assert!(parse(&tags("10\n25\n20\n40\n11\n0\n21\n0\n31\n1\n1\nText\n"),&mut invalid).is_err());
        assert!(invalid.cad.is_none());
    }
}
