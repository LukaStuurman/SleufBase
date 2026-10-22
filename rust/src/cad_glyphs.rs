//! Raw, already-decoded glyph runs shared by TEXT and MTEXT. Owners keep every
//! resolved Font alive for the cache lifetime, so physical-source keys remain
//! valid. This layer never applies TEXT escape decoding to MTEXT words.
use crate::{cad_geometry::Affine, cad_text_path::Path, dxf_text_metrics::Font};
use std::{
    collections::HashMap,
    sync::{atomic::AtomicBool, Arc},
};
type Glyph = (Path, f64);
#[derive(Default)]
pub(crate) struct Cache {
    glyphs: HashMap<((u8, usize), char), Arc<Glyph>>,
    bytes: usize,
    work: usize,
}
impl Cache {
    fn glyph(&mut self, font: &Font, ch: char, cancel: &AtomicBool) -> Result<Arc<Glyph>, String> {
        crate::input::check(cancel)?;
        self.work = self.work.saturating_add(1);
        let id = font.source_id().ok_or("CAD-font mist bronidentiteit")?;
        let key = (id, ch);
        let glyph = if let Some(g) = self.glyphs.get(&key) {
            g.clone()
        } else {
            let (path, advance, work) = font.glyph_path(ch, cancel)?;
            self.work = self.work.saturating_add(work);
            let glyph = Arc::new((path, advance));
            let bytes = glyph.0.points.len() * 24 + glyph.0.kinds.len() + 128;
            if self.bytes.saturating_add(bytes) <= 8 * 1024 * 1024 && self.glyphs.len() < 8192 {
                self.bytes += bytes;
                self.glyphs.insert(key, glyph.clone());
            }
            glyph
        };
        if self.work > 20_000_000 {
            return Err("CAD-tekst overschrijdt glyph-werkbudget".into());
        }
        Ok(glyph)
    }
    pub fn space_width(
        &mut self,
        font: &Font,
        height: f64,
        width: f64,
        cancel: &AtomicBool,
    ) -> Result<f64, String> {
        crate::input::check(cancel)?;
        let advance = if font.kind() == "MonospaceFont" {
            1.
        } else {
            self.glyph(font, ' ', cancel)?.1
        };
        let w = advance * height * width;
        if !w.is_finite() {
            return Err("CAD-spatiebreedte valt buiten getalbereik".into());
        }
        Ok(w)
    }
    pub fn text(
        &mut self,
        font: &Font,
        content: &str,
        height: f64,
        available: usize,
        cancel: &AtomicBool,
    ) -> Result<Path, String> {
        crate::input::check(cancel)?;
        let mut path = Path::default();
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
            let mut offset = 0.;
            for ch in content.chars() {
                let glyph = self.glyph(font, ch, cancel)?;
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
        Ok(path)
    }
}
