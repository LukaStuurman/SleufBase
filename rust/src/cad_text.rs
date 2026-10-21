//! One-line DXF TEXT source and native vector contours. Alignment follows
//! ezdxf 1.4.4 addons/text2path.py (MIT, Manfred Moitzi); not fast-bbox corners.
use crate::{
    cad_geometry::{self as g, Affine},
    cad_text_path::{Path, MAX_CONTROLS},
    dxf_text_metrics::{self as metrics, Font, Text},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub layout: Text,
    pub style: String,
    pub font: String,
    #[serde(default)]
    pub big_font: String,
    pub contours: Path,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribute: Option<Box<Attribute>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attribute {
    pub tag: String,
    pub flags: u32,
    pub field_length: u32,
    pub lock_position: bool,
    pub attribute_type: u8,
    /// Preserve XRECORD/embedded-MTEXT tags for later native layout support.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<crate::cad::Tag>,
}
impl Attribute {
    pub fn multiline(&self) -> bool {
        self.attribute_type != 1
            || self
                .extra
                .iter()
                .any(|t| t.code == 101 && t.value == "Embedded Object")
    }
    fn validate(&self) -> Result<(), String> {
        if self.tag.len() > 1_048_576
            || self.flags > 65535
            || self.field_length > 65535
            || !matches!(self.attribute_type, 1 | 2 | 4)
            || self.extra.len() > 1_000_000
            || self
                .extra
                .iter()
                .any(|t| !(0..=1071).contains(&t.code) || t.value.len() > 1_048_576)
        {
            return Err("Ongeldige CAD-attribuutgegevens".into());
        }
        Ok(())
    }
}
impl Source {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(attribute) = &self.attribute {
            attribute.validate()?;
        }
        let t = &self.layout;
        if t.content.len() > 1_048_576
            || self.style.len() > 1_048_576
            || self.font.len() > 32768
            || self.big_font.len() > 32768
            || t.height <= 0.
            || !t.height.is_finite()
            || !t.width.is_finite()
            || !t.rotation.is_finite()
            || !t.oblique.is_finite()
            || t.halign > 5
            || t.valign > 3
        {
            return Err("Ongeldige opgeslagen CAD-tekst".into());
        }
        g::finite(t.insert)?;
        if let Some(p) = t.align_point {
            g::finite(p)?;
        }
        Affine::ocs(t.normal)?;
        self.contours.validate()
    }
}
#[derive(Clone, Debug, Default)]
pub(crate) struct Style {
    pub font: String,
    pub big_font: String,
    pub vertical: bool,
}
type Glyph = (Path, f64);
pub struct Renderer<'a> {
    catalog: &'a mut crate::dxf_fonts::Catalog,
    fonts: HashMap<String, Font>,
    glyphs: HashMap<((u8, usize), char), Arc<Glyph>>,
    cache_bytes: usize,
    controls: usize,
    work: usize,
    cancel: &'a AtomicBool,
}
impl<'a> Renderer<'a> {
    pub fn new(catalog: &'a mut crate::dxf_fonts::Catalog, cancel: &'a AtomicBool) -> Self {
        Self {
            catalog,
            fonts: HashMap::new(),
            glyphs: HashMap::new(),
            cache_bytes: 0,
            controls: 0,
            work: 0,
            cancel,
        }
    }
    pub fn prepare(&mut self, source: &mut Source) -> Result<(), String> {
        source.validate()?;
        crate::input::check(self.cancel)?;
        if source
            .attribute
            .as_deref()
            .is_some_and(Attribute::multiline)
        {
            source.contours = Path::default();
            return Ok(());
        }
        let content = metrics::plain_text(&source.layout.content);
        if content.is_empty() {
            source.contours = Path::default();
            return Ok(());
        }
        if !self.fonts.contains_key(&source.font) {
            if self.fonts.len() >= 4096 {
                return Err("Te veel CAD-tekstlettertypen".into());
            }
            self.fonts.insert(
                source.font.clone(),
                self.catalog.resolve(&source.font, self.cancel)?,
            );
        }
        let font = self.fonts.get_mut(&source.font).unwrap();
        let (_, fm) = font.measure("", source.layout.height, 1., self.cancel)?;
        let mut path = Path::default();
        let available = MAX_CONTROLS.saturating_sub(self.controls);
        let height = source.layout.height;
        if font.kind() == "MonospaceFont" {
            let w = content.chars().count() as f64 * height;
            for (i, p) in [
                [0., 0., 0.],
                [w, 0., 0.],
                [w, height, 0.],
                [0., height, 0.],
                [0., 0., 0.],
            ]
            .into_iter()
            .enumerate()
            {
                path.push(if i == 0 { 0 } else { 1 }, &[p], available)?;
            }
        } else {
            let id = font.source_id().ok_or("CAD-font mist bronidentiteit")?;
            let mut offset = 0.;
            for (i, ch) in content.chars().enumerate() {
                self.work += 1;
                if i.is_multiple_of(256) {
                    crate::input::check(self.cancel)?;
                }
                let key = (id, ch);
                let glyph = if let Some(g) = self.glyphs.get(&key) {
                    g.clone()
                } else {
                    let (path, advance, work) = font.glyph_path(ch, self.cancel)?;
                    self.work = self.work.saturating_add(work);
                    let glyph = Arc::new((path, advance));
                    let bytes = glyph.0.points.len() * 24 + glyph.0.kinds.len() + 128;
                    if self.cache_bytes.saturating_add(bytes) <= 8 * 1024 * 1024
                        && self.glyphs.len() < 8192
                    {
                        self.cache_bytes += bytes;
                        self.glyphs.insert(key, glyph.clone());
                    }
                    glyph
                };
                if self.work > 20_000_000 {
                    return Err("CAD-tekst overschrijdt glyph-werkbudget".into());
                }
                path.append(
                    &glyph.0,
                    Affine([
                        [height, 0., 0., offset],
                        [0., height, 0., 0.],
                        [0., 0., 1., 0.],
                    ]),
                    available,
                )?;
                offset += glyph.1 * height;
                if !offset.is_finite() {
                    return Err("CAD-tekstlengte valt buiten getalbereik".into());
                }
            }
        }
        // An empty/space-only outline has no alignment box and no ink.
        if path.kinds.iter().all(|&k| k == 0) {
            source.contours = Path::default();
            return Ok(());
        }
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for p in &path.points {
            for i in 0..2 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        let t = &source.layout;
        let dx = match t.halign {
            0 => 0.,
            2 => -max[0],
            _ => -(min[0] + max[0]) / 2.,
        };
        let dy = if t.halign == 4 {
            -fm.cap_height + (fm.cap_height + fm.descender_height) / 2.
        } else {
            match t.valign {
                0 => 0.,
                1 => fm.descender_height,
                2 => -fm.cap_height / 2.,
                _ => -fm.cap_height,
            }
        };
        let fit = t.halign == 3 || t.halign == 5;
        let align = t.align_point.unwrap_or(t.insert);
        let delta = g::sub(align, t.insert);
        let span = max[0] - min[0];
        if fit && span == 0. {
            return Err("FIT/ALIGNED-tekst heeft geen tekenbreedte".into());
        }
        let sx = if fit { g::length(delta) / span } else { 1. };
        let sy = if t.halign == 3 { sx } else { 1. };
        let width = if fit { 1. } else { t.width };
        let mx = if t.generation & 2 != 0 { -width } else { width };
        let my = if t.generation & 4 != 0 { -1. } else { 1. };
        let shear = t.oblique.to_radians().tan();
        let shape = Affine([
            [
                sx * mx,
                sy * shear * mx,
                0.,
                (sx * dx + sy * dy * shear) * mx,
            ],
            [0., sy * my, 0., sy * dy * my],
            [0., 0., 1., 0.],
        ]);
        let angle = if fit {
            delta[1].atan2(delta[0])
        } else {
            t.rotation.to_radians()
        };
        let (s, c) = angle.sin_cos();
        let insert = if fit {
            g::add(t.insert, g::scale(delta, 0.5))
        } else if t.halign == 0 && t.valign == 0 {
            t.insert
        } else {
            align
        };
        let rotation = Affine([
            [c, -s, 0., insert[0]],
            [s, c, 0., insert[1]],
            [0., 0., 1., insert[2]],
        ]);
        path.transform(Affine::ocs(t.normal)?.compose(rotation)?.compose(shape)?)?;
        self.controls = self
            .controls
            .checked_add(path.points.len())
            .filter(|&n| n <= MAX_CONTROLS)
            .ok_or("CAD-tekst overschrijdt contourbudget")?;
        source.contours = path;
        crate::input::check(self.cancel)
    }
}

pub(crate) fn prepare_overlay(
    overlay: &mut crate::dxf_read::DxfOverlay,
    styles: &std::collections::BTreeMap<String, Style>,
    catalog: Option<&mut crate::dxf_fonts::Catalog>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let has_text = overlay
        .source_features()
        .any(|f| f.cad.as_deref().is_some_and(|p| p.text.is_some()));
    if !has_text {
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
    let mut renderer = Renderer::new(catalog, cancel);
    let mut points = 0usize;
    let mut warnings = std::collections::BTreeSet::new();
    fn prepare_one(
        feature: &mut crate::dxf_read::DxfFeature,
        renderer: &mut Renderer<'_>,
        styles: &std::collections::BTreeMap<String, Style>,
        points: &mut usize,
        warnings: &mut std::collections::BTreeSet<String>,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        crate::input::check(cancel)?;
        let Some(cad) = feature.cad.as_deref_mut() else {
            return Ok(());
        };
        let Some(text) = cad.text.as_deref_mut() else {
            return Ok(());
        };
        if text.attribute.as_deref().is_some_and(Attribute::multiline) && warnings.len() < 100 {
            warnings.insert("ATTRIB: ingebouwde MTEXT bewaard; opmaak nog niet weergegeven".into());
        }
        if let Some(style) = styles.get(&text.style.to_lowercase()) {
            text.font = style.font.clone();
            text.big_font = style.big_font.clone();
            if style.vertical && warnings.len() < 100 {
                warnings.insert("TEXT: verticale tekststijl nog niet toegepast".into());
            }
        } else {
            text.font = "txt".into();
        }
        if !text.big_font.is_empty() && warnings.len() < 100 {
            warnings.insert("TEXT: aanvullend BIGFONT nog niet toegepast".into());
        }
        renderer.prepare(text)?;
        let flattened = text.contours.flatten(
            Affine::IDENTITY,
            0.005,
            10_000_000usize.saturating_sub(*points),
            cancel,
        )?;
        *points += flattened.points.len();
        feature.points = flattened.points;
        cad.path_breaks = flattened.breaks;
        Ok(())
    }
    for feature in overlay.features.iter_mut().chain(
        overlay
            .blocks
            .values_mut()
            .flat_map(|b| Arc::make_mut(b).features.iter_mut()),
    ) {
        prepare_one(
            feature,
            &mut renderer,
            styles,
            &mut points,
            &mut warnings,
            cancel,
        )?;
        if let Some(cad) = feature.cad.as_deref_mut() {
            for attribute in &mut cad.linked_attributes {
                prepare_one(
                    attribute,
                    &mut renderer,
                    styles,
                    &mut points,
                    &mut warnings,
                    cancel,
                )?;
            }
        }
    }
    overlay.warnings.extend(warnings);
    Ok(())
}
