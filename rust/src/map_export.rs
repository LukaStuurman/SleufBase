//! Native A4 template map generation from a full TIFF, prepared map or PDOK WMS,
//! CAD paths, the shared profile reference and the original page annotations.
use crate::{
    input, map_canvas,
    map_page::{self, Layout, Settings},
    map_raster,
    map_text::{Fonts, Paths},
    template_raster, Bounds,
};
use image::{ImageReader, Limits, Rgba, RgbaImage};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Background {
    Cadastral,
    Solid {
        color: [u8; 4],
    },
    Png {
        path: PathBuf,
    },
    #[default]
    Pdok,
}
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stroke {
    pub points: Vec<[f64; 2]>,
    pub color: [u8; 3],
    #[serde(default)]
    pub selected: bool,
    #[serde(default)]
    pub highlighted: bool,
}
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Source {
    pub settings: Settings,
    pub background: Background,
    pub tiff: Option<template_raster::Source>,
    /// By default use tiff above, or the slot's unrotated full-resolution TIFF.
    /// An image-only/vector map must explicitly opt out rather than omit a TIFF.
    pub include_tiff: bool,
    /// Template maps default to opaque TIFFs; standalone maps retain 70%.
    pub tiff_opacity: f64,
    pub overlays: Arc<Vec<Stroke>>,
    pub attribution: Option<String>,
    /// Already in displayed RD coordinates. None inherits the measured profile.
    pub reference: Option<[f64; 2]>,
    pub fonts: Option<Paths>,
}
impl Default for Source {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            background: Background::default(),
            tiff: None,
            include_tiff: true,
            tiff_opacity: 1.,
            overlays: Arc::new(Vec::new()),
            attribution: None,
            reference: None,
            fonts: None,
        }
    }
}
impl Source {
    pub fn resolve_paths(&mut self, parent: &Path) {
        let resolve = |p: &mut PathBuf| {
            if p.is_relative() {
                *p = parent.join(&*p);
            }
        };
        if let Background::Png { path } = &mut self.background {
            resolve(path);
        }
        if let Some(tiff) = &mut self.tiff {
            resolve(&mut tiff.path);
        }
        if let Some(fonts) = &mut self.fonts {
            resolve(&mut fonts.regular);
            resolve(&mut fonts.bold);
        }
    }
    pub fn input_paths(&self) -> Vec<&Path> {
        let mut result = Vec::new();
        if let Background::Png { path } = &self.background {
            result.push(path.as_path());
        }
        if let Some(tiff) = &self.tiff {
            result.push(tiff.path.as_path());
        }
        if let Some(fonts) = &self.fonts {
            result.extend([fonts.regular.as_path(), fonts.bold.as_path()]);
        }
        result
    }
    fn validate(&self) -> Result<(), String> {
        if !self.tiff_opacity.is_finite() || !(0.0..=1.0).contains(&self.tiff_opacity) {
            return Err("Ongeldige TIFF-dekking voor kaartuitvoer".into());
        }
        if self.overlays.len() > 200_000
            || self.overlays.iter().map(|s| s.points.len()).sum::<usize>() > 2_000_000
        {
            return Err("Te veel CAD-kaartpunten voor één blad".into());
        }
        if self
            .overlays
            .iter()
            .flat_map(|s| &s.points)
            .flatten()
            .any(|v| !v.is_finite() || v.abs() > 1e12)
            || self
                .reference
                .is_some_and(|p| p.iter().any(|v| !v.is_finite() || v.abs() > 1e12))
        {
            return Err("Ongeldige RD-kaartcoördinaten".into());
        }
        if self.attribution.as_ref().is_some_and(|s| s.len() > 16_384) {
            return Err("Kaartbronvermelding is te lang".into());
        }
        Ok(())
    }
}
#[derive(Default)]
pub struct Renderer {
    fonts: Option<(Option<Paths>, Fonts)>,
    cadastral: Option<crate::cadastral_tiles::Client>,
}
/// A slot may supply either an actual GeoTIFF or its native virtual RD raster.
/// Both use the same affine sampler, without temporary TIFF files or copies.
pub enum Raster<'a> {
    Tiff(&'a template_raster::Source),
    Virtual(&'a crate::virtual_raster::Source),
}
pub struct Page {
    pub image: RgbaImage,
    pub layout: Layout,
}
fn background(
    source: &Background,
    layout: &Layout,
    cancel: &AtomicBool,
    cadastral: &mut Option<crate::cadastral_tiles::Client>,
) -> Result<RgbaImage, String> {
    match source {
        Background::Cadastral => {
            if cadastral.is_none() {
                *cadastral = Some(crate::cadastral_tiles::Client::new(
                    crate::project::app_data_directory().join("cadastral-tiles"),
                )?);
            }
            cadastral.as_ref().unwrap().request(layout.bounds, layout.map_size, cancel)
        }
        Background::Solid { color } => {
            let mut image =
                crate::raster_rotation::allocate(layout.map_size[0], layout.map_size[1])?;
            for p in image.pixels_mut() {
                *p = Rgba(*color);
            }
            Ok(image)
        }
        Background::Pdok => crate::pdok_wms::request_tiled(
            layout.bounds,
            layout.map_size[0],
            layout.map_size[1],
            cancel,
        ),
        Background::Png { path } => {
            let bytes = input::read(path, 256 * 1024 * 1024, "Kaartachtergrond", cancel)?;
            let mut reader = ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
            let mut limits = Limits::default();
            limits.max_image_width = Some(layout.map_size[0]);
            limits.max_image_height = Some(layout.map_size[1]);
            limits.max_alloc = Some(256 * 1024 * 1024);
            reader.limits(limits);
            let image = reader
                .decode()
                .map_err(|e| format!("Ongeldige kaartachtergrond: {e}"))?
                .into_rgba8();
            if image.dimensions() != (layout.map_size[0], layout.map_size[1]) {
                return Err(
                    "Kaartachtergrond moet exact bij de berekende kaartuitsnede passen".into(),
                );
            }
            Ok(image)
        }
    }
}
fn overlays(
    image: &mut RgbaImage,
    source: &Source,
    layout: &Layout,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let width = if layout.meters_per_pixel < 0.05 {
        4
    } else if layout.meters_per_pixel < 0.2 {
        3
    } else {
        2
    };
    let mut budget = 200_000_000;
    for stroke in source.overlays.iter() {
        input::check(cancel)?;
        if stroke.points.len() < 2 {
            continue;
        }
        let points = stroke
            .points
            .iter()
            .map(|p| layout.screen(*p))
            .collect::<Vec<_>>();
        let color = [stroke.color[0], stroke.color[1], stroke.color[2], 230];
        if stroke.highlighted {
            map_canvas::line(image, &points, 10, [0, 160, 255, 105], cancel, &mut budget)?;
        }
        if stroke.selected {
            map_canvas::line(image, &points, 7, [255, 215, 0, 215], cancel, &mut budget)?;
        }
        map_canvas::line(image, &points, width, color, cancel, &mut budget)?;
        if points.len() == 2 && points[0] == points[1] {
            let [x, y] = points[0];
            let radius = (width + 2).max(4) as f64;
            let mut circle =
                |r: f64, c| map_canvas::ellipse(image, [x - r, y - r, x + r, y + r], c, None, cancel, &mut budget);
            if stroke.highlighted {
                circle(radius + 4., [0, 160, 255, 105])?;
            }
            if stroke.selected {
                circle(radius + 3., [255, 215, 0, 215])?;
            }
            circle(radius, color)?;
        }
    }
    Ok(())
}
impl Renderer {
    pub fn render(
        &mut self,
        source: &Source,
        bounds: Bounds,
        slot_tiff: Option<&template_raster::Source>,
        profile_reference: Option<[f64; 2]>,
        cancel: &AtomicBool,
    ) -> Result<Page, String> {
        self.render_with_raster(
            source,
            bounds,
            slot_tiff.map(Raster::Tiff),
            profile_reference,
            cancel,
        )
    }
    pub fn render_with_raster(
        &mut self,
        source: &Source,
        bounds: Bounds,
        fallback: Option<Raster<'_>>,
        profile_reference: Option<[f64; 2]>,
        cancel: &AtomicBool,
    ) -> Result<Page, String> {
        input::check(cancel)?;
        source.validate()?;
        let layout = map_page::layout(bounds, source.settings)?;
        let tiff = if source.include_tiff {
            Some(
                source
                    .tiff
                    .as_ref()
                    .map(Raster::Tiff)
                    .or(fallback)
                    .ok_or("Een volledige TIFF-bron is nodig voor dit kaartblad")?,
            )
        } else {
            None
        };
        if self.fonts.as_ref().is_none_or(|f| f.0 != source.fonts) {
            self.fonts = Some((
                source.fonts.clone(),
                Fonts::load(source.fonts.as_ref(), cancel)?,
            ));
        }
        let fonts = &mut self.fonts.as_mut().unwrap().1;
        let attribution = source
            .attribution
            .as_deref()
            .or(match source.background {
                Background::Pdok => Some("Luchtfoto: PDOK"),
                Background::Cadastral => Some("Kadastrale kaart: Kadaster / PDOK"),
                _ => None,
            });
        let operations = crate::map_annotations::plan(
            &layout,
            attribution,
            source.reference.or(profile_reference),
            &mut |t, s, b| fonts.measure(t, s, b),
            cancel,
        )?;
        let mut map = background(&source.background, &layout, cancel, &mut self.cadastral)?;
        if let Some(tiff) = tiff {
            let (image, transform) = match tiff {
                Raster::Tiff(tiff) => {
                    let raster = crate::raster::decode_full_with_cancel(&tiff.path, cancel)?;
                    let image = RgbaImage::from_raw(raster.width, raster.height, raster.rgba)
                        .ok_or("Ongeldige kaart-TIFF-pixelbuffer")?;
                    (image, tiff.transform)
                }
                Raster::Virtual(source) => {
                    let info = crate::virtual_raster::layout(&source.trench, source.quality)?;
                    (
                        crate::virtual_raster::render(source, cancel)?,
                        info.transform(),
                    )
                }
            };
            map_raster::paint_with_opacity(&mut map, image, transform, &layout, source.tiff_opacity, cancel)?;
        }
        overlays(&mut map, source, &layout, cancel)?;
        let mut image = crate::raster_rotation::allocate(layout.page_size[0], layout.page_size[1])?;
        for p in image.pixels_mut() {
            *p = Rgba([255; 4]);
        }
        map_raster::composite(&mut image, &map, layout.origin.map(|v| v as i32), cancel)?;
        drop(map);
        map_page::draw(&mut image, &operations, fonts, cancel)?;
        Ok(Page { image, layout })
    }
}
