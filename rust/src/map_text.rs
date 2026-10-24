//! CPU font outlines with HarfBuzz shaping and Unicode bidirectional runs.
use crate::{input, map_canvas};
use ab_glyph::{point, Font, FontArc, Glyph, GlyphId, PxScale};
use image::RgbaImage;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};
use unicode_script::{Script, UnicodeScript};
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paths {
    pub regular: PathBuf,
    pub bold: PathBuf,
}
struct Layout {
    glyphs: Vec<Glyph>,
    bounds: [f32; 4],
    advance: f32,
    ascent: f32,
    descent: f32,
}
pub struct Fonts {
    regular: FontArc,
    bold: FontArc,
    cache: HashMap<(String, u32, bool), Arc<Layout>>,
    cached_glyphs: usize,
}
impl Fonts {
    pub fn load(paths: Option<&Paths>, cancel: &AtomicBool) -> Result<Self, String> {
        fn read(path: &Path, cancel: &AtomicBool) -> Result<FontArc, String> {
            FontArc::try_from_vec(input::read(
                path,
                32 * 1024 * 1024,
                "Kaartlettertype",
                cancel,
            )?)
            .map_err(|_| "Ongeldig kaartlettertype".into())
        }
        let (regular, bold) = if let Some(p) = paths {
            (read(&p.regular, cancel)?, read(&p.bold, cancel)?)
        } else {
            let folder = std::env::var_os("WINDIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("C:/Windows"))
                .join("Fonts");
            let regular = read(&folder.join("segoeui.ttf"), cancel)
                .or_else(|_| read(&folder.join("arial.ttf"), cancel))?;
            let bold = read(&folder.join("segoeuib.ttf"), cancel)
                .or_else(|_| read(&folder.join("arialbd.ttf"), cancel))?;
            (regular, bold)
        };
        Ok(Self {
            regular,
            bold,
            cache: HashMap::new(),
            cached_glyphs: 0,
        })
    }
    fn layout(&mut self, text: &str, size: f32, bold: bool) -> Result<Arc<Layout>, String> {
        if text.len() > 16_384
            || !size.is_finite()
            || !(1. ..=512.).contains(&size)
            || text.chars().any(|c| c.is_control() && c != '\n')
        {
            return Err("Ongeldige kaarttekst of tekstgrootte".into());
        }
        let key = (text.to_owned(), size.to_bits(), bold);
        if let Some(v) = self.cache.get(&key) {
            return Ok(v.clone());
        }
        let font = if bold { &self.bold } else { &self.regular };
        let face = rustybuzz::Face::from_slice(font.font_data(), 0)
            .ok_or("Ongeldig tekstvormingslettertype")?;
        let factor = size / face.units_per_em() as f32;
        let scale = PxScale::from(
            size * font.height_unscaled() / font.units_per_em().ok_or("Lettertype mist em-maat")?,
        );
        let ascent = (font.ascent_unscaled() * factor).ceil();
        let descent = (font.descent_unscaled() * factor).floor();
        let mut glyphs = Vec::new();
        let mut advance: f32 = 0.;
        let mut bounds = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        let bidi = unicode_bidi::BidiInfo::new(text, None);
        for (line, paragraph) in bidi.paragraphs.iter().enumerate() {
            let (levels, runs) = bidi.visual_runs(paragraph, paragraph.range.clone());
            let mut pen = 0.;
            let baseline = line as f32 * (ascent - descent + 4.);
            for run in runs {
                let rtl = levels[run.start].is_rtl();
                let part = &text[run.clone()];
                let first = part
                    .chars()
                    .map(|c| c.script())
                    .find(|s| !matches!(s, Script::Common | Script::Inherited | Script::Unknown))
                    .unwrap_or(Script::Latin);
                let mut script = first;
                let mut start = 0;
                let mut pieces = Vec::new();
                for (i, c) in part.char_indices() {
                    let s = c.script();
                    if !matches!(s, Script::Common | Script::Inherited | Script::Unknown)
                        && s != script
                    {
                        if i > start {
                            pieces.push((start..i, script));
                        }
                        start = i;
                        script = s;
                    }
                }
                if start < part.len() {
                    pieces.push((start..part.len(), script));
                }
                if rtl {
                    pieces.reverse();
                }
                for (range, script) in pieces {
                    let mut buffer = rustybuzz::UnicodeBuffer::new();
                    buffer.push_str(&part[range]);
                    buffer.set_direction(if rtl {
                        rustybuzz::Direction::RightToLeft
                    } else {
                        rustybuzz::Direction::LeftToRight
                    });
                    let tag = rustybuzz::ttf_parser::Tag::from_bytes(
                        script
                            .short_name()
                            .as_bytes()
                            .try_into()
                            .map_err(|_| "Ongeldig tekstscript")?,
                    );
                    if let Some(script) = rustybuzz::Script::from_iso15924_tag(tag) {
                        buffer.set_script(script);
                    }
                    buffer.guess_segment_properties();
                    let shaped = rustybuzz::shape(&face, &[], buffer);
                    if glyphs.len() + shaped.len() > 100_000 {
                        return Err("Kaarttekst overschrijdt het glyphbudget".into());
                    }
                    for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
                        let quantize = |v: i32| ((v as f32 * factor * 64.).round()) / 64.;
                        let glyph = Glyph {
                            id: GlyphId(
                                u16::try_from(info.glyph_id).map_err(|_| "Glyph buiten bereik")?,
                            ),
                            scale,
                            position: point(
                                pen + quantize(pos.x_offset),
                                baseline - quantize(pos.y_offset),
                            ),
                        };
                        if let Some(outline) = font.outline_glyph(glyph.clone()) {
                            let b = outline.px_bounds();
                            bounds[0] = bounds[0].min(b.min.x);
                            bounds[1] = bounds[1].min(b.min.y);
                            bounds[2] = bounds[2].max(b.max.x);
                            bounds[3] = bounds[3].max(b.max.y);
                        }
                        pen += quantize(pos.x_advance);
                        glyphs.push(glyph);
                    }
                }
            }
            advance = advance.max(pen);
        }
        if !bounds[0].is_finite() {
            bounds = [0.; 4];
        }
        let layout = Arc::new(Layout {
            glyphs,
            bounds,
            advance,
            ascent,
            descent,
        });
        if self.cache.len() >= 1024 || self.cached_glyphs + layout.glyphs.len() > 100_000 {
            self.cache.clear();
            self.cached_glyphs = 0;
        }
        self.cached_glyphs += layout.glyphs.len();
        self.cache.insert(key, layout.clone());
        Ok(layout)
    }
    pub fn measure(&mut self, text: &str, size: f32, bold: bool) -> Result<[f64; 4], String> {
        let l = self.layout(text, size, bold)?;
        Ok([
            l.bounds[0].min(0.) as f64,
            (l.bounds[1] + l.ascent) as f64,
            l.bounds[2].max(l.advance.ceil()) as f64,
            (l.bounds[3] + l.ascent) as f64,
        ])
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw(
        &mut self,
        image: &mut RgbaImage,
        text: &str,
        position: [f64; 2],
        size: f32,
        bold: bool,
        anchor: &str,
        fill: [u8; 4],
        stroke: Option<([u8; 4], u32)>,
        angle: f64,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        if position.iter().any(|n| !n.is_finite() || n.abs() > 1e9)
            || !angle.is_finite()
            || stroke.is_some_and(|s| s.1 > 16)
        {
            return Err("Ongeldige tekstpositie, rotatie of lijndikte".into());
        }
        input::check(cancel)?;
        let l = self.layout(text, size, bold)?;
        let font = if bold { &self.bold } else { &self.regular };
        let mut anchor_chars = anchor.chars();
        let x = match anchor_chars.next().unwrap_or('l') {
            'm' => -l.advance / 2.,
            'r' => -l.advance,
            _ => 0.,
        };
        let y = match anchor_chars.next().unwrap_or('a') {
            's' => 0.,
            't' => -l.bounds[1],
            'b' => -l.bounds[3],
            'm' => (l.ascent + l.descent) / 2.,
            _ => l.ascent,
        };
        let radius = stroke.map_or(0, |s| s.1) as i32;
        let left = l.bounds[0] as i32 - radius;
        let top = l.bounds[1] as i32 - radius;
        let width = (l.bounds[2] - l.bounds[0]).max(1.) as u32 + 2 * radius as u32;
        let height = (l.bounds[3] - l.bounds[1]).max(1.) as u32 + 2 * radius as u32;
        if u64::from(width) * u64::from(height) > 16_000_000 || radius > 16 {
            return Err("Kaarttekst overschrijdt het rasterbudget".into());
        }
        let mut mask = vec![0u8; width as usize * height as usize];
        for glyph in &l.glyphs {
            input::check(cancel)?;
            if let Some(outline) = font.outline_glyph(glyph.clone()) {
                let b = outline.px_bounds();
                outline.draw(|px, py, coverage| {
                    let dx = px as i32 + b.min.x as i32 - left;
                    let dy = py as i32 + b.min.y as i32 - top;
                    if dx >= 0 && dy >= 0 && dx < width as i32 && dy < height as i32 {
                        let i = dy as usize * width as usize + dx as usize;
                        mask[i] = mask[i].max((coverage * 255.).round() as u8);
                    }
                });
            }
        }
        let mut overlay = crate::raster_rotation::allocate(width, height)?;
        for py in 0..height as i32 {
            input::check(cancel)?;
            for px in 0..width as i32 {
                let mut p = [0, 0, 0, 0];
                if let Some((color, _)) = stroke {
                    let mut coverage = 0;
                    for dy in -radius..=radius {
                        for dx in -radius..=radius {
                            if dx * dx + dy * dy > radius * radius {
                                continue;
                            }
                            let sx = px + dx;
                            let sy = py + dy;
                            if sx >= 0 && sy >= 0 && sx < width as i32 && sy < height as i32 {
                                coverage =
                                    coverage.max(mask[sy as usize * width as usize + sx as usize]);
                            }
                        }
                    }
                    p = color;
                    p[3] = (u16::from(color[3]) * u16::from(coverage) / 255) as u8;
                }
                let mut color = fill;
                color[3] = (u16::from(color[3])
                    * u16::from(mask[py as usize * width as usize + px as usize])
                    / 255) as u8;
                map_canvas::blend(&mut p, color);
                overlay.put_pixel(px as u32, py as u32, image::Rgba(p));
            }
        }
        if angle.rem_euclid(360.) != 0. {
            overlay = crate::raster_rotation::rotate(overlay, angle, cancel)?;
        }
        let (ox, oy) = if angle.rem_euclid(360.) == 0. {
            (
                (position[0] + x as f64 + left as f64).round_ties_even() as i32,
                (position[1] + y as f64 + top as f64).round_ties_even() as i32,
            )
        } else {
            (
                (position[0] - overlay.width() as f64 / 2.).round_ties_even() as i32,
                (position[1] - overlay.height() as f64 / 2.).round_ties_even() as i32,
            )
        };
        for py in 0..overlay.height() {
            input::check(cancel)?;
            for px in 0..overlay.width() {
                map_canvas::pixel(
                    image,
                    ox + px as i32,
                    oy + py as i32,
                    overlay.get_pixel(px, py).0,
                    true,
                );
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_layouts_share_glyphs_and_large_distinct_texts_cannot_grow_cache_unbounded() {
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/map-export");
        let paths = Paths {
            regular: root.join("Hack-Regular.ttf"),
            bold: root.join("Hack-Regular.ttf"),
        };
        let mut fonts = Fonts::load(Some(&paths), &AtomicBool::new(false)).unwrap();
        let first = fonts.layout("Referentiepunt", 72., false).unwrap();
        let repeated = fonts.layout("Referentiepunt", 72., false).unwrap();
        assert!(Arc::ptr_eq(&first, &repeated));
        for i in 0..12 {
            fonts
                .layout(&format!("{i}{}", "A".repeat(10_000)), 20., false)
                .unwrap();
        }
        assert!(fonts.cache.values().map(|l| l.glyphs.len()).sum::<usize>() <= 100_000);
        assert!(fonts.cached_glyphs <= 100_000);
        assert!(fonts.cache.len() < 12);
        assert_eq!(first.glyphs.len(), 14);
    }
    #[test]
    fn glyphs_and_positions_match_independent_harfbuzz_for_latin_combining_rtl_and_indic() {
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/map-export");
        let paths = Paths {
            regular: root.join("Hack-Regular.ttf"),
            bold: root.join("Hack-Regular.ttf"),
        };
        let mut fonts = Fonts::load(Some(&paths), &AtomicBool::new(false)).unwrap();
        let fixture: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("reference.json")).unwrap()).unwrap();
        for c in fixture["shaping"].as_array().unwrap() {
            let layout = fonts
                .layout(
                    c["text"].as_str().unwrap(),
                    c["size"].as_f64().unwrap() as f32,
                    false,
                )
                .unwrap();
            let expected = c["glyphs"].as_array().unwrap();
            assert_eq!(layout.glyphs.len(), expected.len(), "{}", c["text"]);
            for (g, e) in layout.glyphs.iter().zip(expected) {
                assert_eq!(u64::from(g.id.0), e[0].as_u64().unwrap());
                assert_eq!(f64::from(g.position.x), e[1].as_f64().unwrap());
                assert_eq!(f64::from(g.position.y), e[2].as_f64().unwrap());
            }
            assert_eq!(f64::from(layout.advance), c["advance"].as_f64().unwrap());
        }
    }
}
