//! Original A4 landscape map-page geometry, coordinate frame and annotations.
use crate::{input, map_canvas as canvas, map_text::Fonts, Bounds};
use image::RgbaImage;
use std::sync::atomic::AtomicBool;
pub type XY = [f64; 2];
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub dpi: u32,
    pub scale: f64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            dpi: 300,
            scale: 225.,
        }
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Layout {
    pub page_size: [u32; 2],
    pub map_size: [u32; 2],
    pub origin: XY,
    pub bounds: Bounds,
    pub meters_per_pixel: f64,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Op {
    Line {
        points: Vec<XY>,
        fill: [u8; 4],
        width: u32,
    },
    Rect {
        bounds: [f64; 4],
        fill: Option<[u8; 4]>,
        outline: Option<[u8; 4]>,
        width: u32,
        radius: f64,
    },
    Ellipse {
        bounds: [f64; 4],
        fill: [u8; 4],
        outline: [u8; 4],
        width: u32,
    },
    Polygon {
        points: Vec<XY>,
        fill: [u8; 4],
    },
    Text {
        position: XY,
        text: String,
        size: f32,
        bold: bool,
        anchor: String,
        fill: [u8; 4],
        stroke: Option<([u8; 4], u32)>,
        angle: f64,
        frame_rotation: bool,
    },
}
const BLACK: [u8; 4] = [0, 0, 0, 255];
const WHITE: [u8; 4] = [255; 4];
fn valid(b: Bounds) -> bool {
    [b.min_x, b.min_y, b.max_x, b.max_y]
        .into_iter()
        .all(|v| v.is_finite() && v.abs() <= 1e12)
        && b.min_x < b.max_x
        && b.min_y < b.max_y
}
fn expand(mut b: Bounds, aspect: f64) -> Bounds {
    let w = b.max_x - b.min_x;
    let h = b.max_y - b.min_y;
    if (w / h - aspect).abs() < 1e-9 {
        return b;
    }
    if w / h < aspect {
        let p = (h * aspect - w) / 2.;
        b.min_x -= p;
        b.max_x += p;
    } else {
        let p = (w / aspect - h) / 2.;
        b.min_y -= p;
        b.max_y += p;
    }
    b
}
pub fn layout(bounds: Bounds, settings: Settings) -> Result<Layout, String> {
    if !valid(bounds)
        || !(30..=600).contains(&settings.dpi)
        || !settings.scale.is_finite()
        || settings.scale <= 0.
        || settings.scale > 100_000.
    {
        return Err("Ongeldige kaartuitsnede, DPI of schaal".into());
    }
    let dpi = settings.dpi as f64;
    let page_size = [
        ((297. / 25.4) * dpi).round_ties_even() as u32,
        ((210. / 25.4) * dpi).round_ties_even() as u32,
    ];
    if u64::from(page_size[0]) * u64::from(page_size[1]) > 50_000_000
        || page_size.iter().any(|&n| n <= 164)
    {
        return Err("Kaartpagina overschrijdt het pixelbudget of is te klein".into());
    }
    let map_size = [page_size[0] - 164, page_size[1] - 164];
    let pixels_per_meter = dpi * 39.37007874 / settings.scale;
    let span = (bounds.max_x - bounds.min_x).max(bounds.max_y - bounds.min_y);
    let padding = (span * 0.1).clamp(1., 4.);
    let required = Bounds {
        min_x: bounds.min_x - padding,
        min_y: bounds.min_y - padding,
        max_x: bounds.max_x + padding,
        max_y: bounds.max_y + padding,
    };
    let width = (required.max_x - required.min_x).max(map_size[0] as f64 / pixels_per_meter);
    let height = (required.max_y - required.min_y).max(map_size[1] as f64 / pixels_per_meter);
    let cx = (required.min_x + required.max_x) / 2.;
    let cy = (required.min_y + required.max_y) / 2.;
    let mut candidate = expand(
        Bounds {
            min_x: cx - width / 2.,
            max_x: cx + width / 2.,
            min_y: cy - height / 2.,
            max_y: cy + height / 2.,
        },
        map_size[0] as f64 / map_size[1] as f64,
    );
    for (min, max, low, high) in [
        (
            &mut candidate.min_x,
            &mut candidate.max_x,
            required.min_x,
            required.max_x,
        ),
        (
            &mut candidate.min_y,
            &mut candidate.max_y,
            required.min_y,
            required.max_y,
        ),
    ] {
        if *min > low {
            let d = *min - low;
            *min -= d;
            *max -= d;
        }
        if *max < high {
            let d = high - *max;
            *min += d;
            *max += d;
        }
    }
    let meters_per_pixel = (candidate.max_x - candidate.min_x) / map_size[0] as f64;
    Ok(Layout {
        page_size,
        map_size,
        origin: [82., 82.],
        bounds: candidate,
        meters_per_pixel,
    })
}
impl Layout {
    pub fn screen(&self, p: XY) -> XY {
        [
            (p[0] - self.bounds.min_x) / (self.bounds.max_x - self.bounds.min_x)
                * self.map_size[0] as f64,
            self.map_size[1] as f64
                - (p[1] - self.bounds.min_y) / (self.bounds.max_y - self.bounds.min_y)
                    * self.map_size[1] as f64,
        ]
    }
    fn page(&self, p: XY) -> XY {
        let s = self.screen(p);
        [self.origin[0] + s[0], self.origin[1] + s[1]]
    }
}
fn line(ops: &mut Vec<Op>, points: Vec<XY>, fill: [u8; 4], width: u32) {
    ops.push(Op::Line {
        points,
        fill,
        width,
    });
}
fn text(
    ops: &mut Vec<Op>,
    position: XY,
    value: impl Into<String>,
    size: f32,
    bold: bool,
    anchor: &str,
) {
    ops.push(Op::Text {
        position,
        text: value.into(),
        size,
        bold,
        anchor: anchor.into(),
        fill: BLACK,
        stroke: None,
        angle: 0.,
        frame_rotation: false,
    });
}
fn outlined(ops: &mut Vec<Op>, position: XY, value: &str, size: f32, bold: bool, width: u32) {
    ops.push(Op::Text {
        position,
        text: value.into(),
        size,
        bold,
        anchor: "ms".into(),
        fill: WHITE,
        stroke: Some((BLACK, width)),
        angle: 0.,
        frame_rotation: false,
    });
}
fn rect(
    ops: &mut Vec<Op>,
    bounds: [f64; 4],
    fill: Option<[u8; 4]>,
    outline: Option<[u8; 4]>,
    width: u32,
    radius: f64,
) {
    ops.push(Op::Rect {
        bounds,
        fill,
        outline,
        width,
        radius,
    });
}
pub fn plan(
    layout: &Layout,
    attribution: Option<&str>,
    reference: Option<XY>,
    measure: &mut impl FnMut(&str, f32, bool) -> Result<[f64; 4], String>,
    cancel: &AtomicBool,
) -> Result<Vec<Op>, String> {
    if !valid(layout.bounds)
        || layout.map_size.contains(&0)
        || layout.map_size.iter().any(|&n| n > 10_000)
        || layout
            .origin
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 1e9)
        || !layout.meters_per_pixel.is_finite()
        || layout.meters_per_pixel <= 0.
    {
        return Err("Ongeldige kaartpagina".into());
    }
    input::check(cancel)?;
    let [left, top] = layout.origin;
    let [w, h] = layout.map_size.map(|n| n as f64);
    let mut ops = Vec::new();
    rect(
        &mut ops,
        [left, top, left + w, top + h],
        None,
        Some(BLACK),
        2,
        0.,
    );
    let b = layout.bounds;
    let startx = (b.min_x / 20.).ceil() as i64;
    let endx = (b.max_x / 20.).floor() as i64;
    let starty = (b.min_y / 20.).ceil() as i64;
    let endy = (b.max_y / 20.).floor() as i64;
    let nx = (endx - startx + 1).max(0) as u64;
    let ny = (endy - starty + 1).max(0) as u64;
    if nx.saturating_mul(ny) > 40_000 || nx + ny > 2000 {
        return Err("Te veel coördinaatkruisen voor één kaartpagina".into());
    }
    for i in startx..=endx {
        input::check(cancel)?;
        let c = i as f64 * 20.;
        if c < b.min_x || c > b.max_x {
            continue;
        }
        let x = left + layout.screen([c, b.min_y])[0].round_ties_even();
        line(&mut ops, vec![[x, top - 22.], [x, top]], BLACK, 3);
        line(&mut ops, vec![[x, top + h], [x, top + h + 22.]], BLACK, 3);
        text(
            &mut ops,
            [x, top - 28.],
            format!("{}", c.round_ties_even() as i64),
            36.,
            false,
            "ms",
        );
        text(
            &mut ops,
            [x, top + h + 28.],
            format!("{}", c.round_ties_even() as i64),
            36.,
            false,
            "mt",
        );
    }
    for i in starty..=endy {
        input::check(cancel)?;
        let c = i as f64 * 20.;
        if c < b.min_y || c > b.max_y {
            continue;
        }
        let y = top + layout.screen([b.min_x, c])[1].round_ties_even();
        line(&mut ops, vec![[left - 22., y], [left, y]], BLACK, 3);
        line(&mut ops, vec![[left + w, y], [left + w + 22., y]], BLACK, 3);
        for (x, angle) in [(left - 50., 90.), (left + w + 50., 270.)] {
            ops.push(Op::Text {
                position: [x, y],
                text: format!("{}", c.round_ties_even() as i64),
                size: 36.,
                bold: false,
                anchor: "la".into(),
                fill: BLACK,
                stroke: None,
                angle,
                frame_rotation: true,
            });
        }
    }
    for ix in startx..=endx {
        input::check(cancel)?;
        for iy in starty..=endy {
            let [sx, sy] = layout.screen([ix as f64 * 20., iy as f64 * 20.]);
            let x = left + sx.round_ties_even();
            let y = top + sy.round_ties_even();
            line(
                &mut ops,
                vec![[x - 11., y], [x + 11., y]],
                [0, 0, 0, 190],
                2,
            );
            line(
                &mut ops,
                vec![[x, y - 11.], [x, y + 11.]],
                [0, 0, 0, 190],
                2,
            );
        }
    }
    let segment = ((20. / layout.meters_per_pixel).round_ties_even() as i64 / 4).max(1) as f64;
    let x = left + 72.;
    let y = top + h - 108.;
    for i in 0..4 {
        rect(
            &mut ops,
            [
                x + i as f64 * segment,
                y,
                x + (i + 1) as f64 * segment,
                y + 40.,
            ],
            Some(if i % 2 == 0 { BLACK } else { WHITE }),
            Some(BLACK),
            4,
            0.,
        );
    }
    for (i, value) in ["0", "5", "10", "15", "20 m"].iter().enumerate() {
        outlined(
            &mut ops,
            [x + i as f64 * segment, y - 14.],
            value,
            40.,
            true,
            5,
        );
    }
    let mut legend = None;
    if let Some(reference) = reference {
        if reference.iter().any(|v| !v.is_finite()) {
            return Err("Ongeldig kaartreferentiepunt".into());
        }
        let [x, y] = layout.page(reference);
        ops.push(Op::Ellipse {
            bounds: [x - 11., y - 11., x + 11., y + 11.],
            fill: [255, 255, 255, 240],
            outline: BLACK,
            width: 4,
        });
        let title = measure("Legenda", 84., true)?;
        let item = measure("Referentiepunt", 72., false)?;
        let titlewidth = title[2] - title[0];
        let titleheight = title[3] - title[1];
        let itemwidth = item[2] - item[0];
        let itemheight = item[3] - item[1];
        let width = titlewidth.max(192. + 42. + itemwidth) + 108.;
        let height = 42. + titleheight + 42. + 78f64.max(itemheight) + 42.;
        let right = left + w - 1.;
        let bottom = top + h - 1.;
        let l = right - width;
        let t = bottom - height;
        rect(
            &mut ops,
            [l, t, right, bottom],
            Some(WHITE),
            Some(BLACK),
            4,
            0.,
        );
        text(&mut ops, [l + 54., t + 42.], "Legenda", 84., true, "la");
        let cy = t + 42. + titleheight + 42. + 78f64.max(itemheight) / 2.;
        let cx = l + 54. + 42.;
        ops.push(Op::Ellipse {
            bounds: [cx - 30., cy - 30., cx + 30., cy + 30.],
            fill: [255, 255, 255, 240],
            outline: BLACK,
            width: 8,
        });
        text(
            &mut ops,
            [l + 54. + 192. + 42., cy],
            "Referentiepunt",
            72.,
            false,
            "lm",
        );
        legend = Some([
            l as i64 as f64,
            t as i64 as f64,
            right as i64 as f64,
            bottom as i64 as f64,
        ]);
    }
    let (cx, bottom, height, half, notch) = if let Some([l, t, _, bottom]) = legend {
        let height = (bottom - t + 10.).clamp(138., 182.);
        let half = (height * 0.3).round_ties_even().max(42.);
        (
            (left + half + 28.).max(l - half - 34.),
            bottom - 18.,
            height,
            half,
            (height * 0.26).round_ties_even().max(34.),
        )
    } else {
        (left + w - 146., top + h - 28., 270., 84., 72.)
    };
    let tip = bottom - height;
    let outer = vec![
        [cx, tip],
        [cx - half, bottom],
        [cx, bottom - notch],
        [cx + half, bottom],
    ];
    ops.push(Op::Polygon {
        points: outer.clone(),
        fill: WHITE,
    });
    let mut closed = outer;
    closed.push(closed[0]);
    line(&mut ops, closed, BLACK, 6);
    outlined(&mut ops, [cx, tip - 24.], "N", 80., true, 5);
    if let Some(value) = attribution.filter(|s| !s.is_empty()) {
        let bbox = measure(value, 20., false)?;
        let width = bbox[2] - bbox[0];
        let height = bbox[3] - bbox[1];
        let x = left + w - width - 28.;
        let y = legend.map_or(top + h - height - 22., |r| {
            (top + 18.).max(r[1] - height - 22.)
        });
        rect(
            &mut ops,
            [x - 10., y - 6., x + width + 10., y + height + 6.],
            Some([255, 255, 255, 220]),
            None,
            1,
            8.,
        );
        ops.push(Op::Text {
            position: [x, y],
            text: value.into(),
            size: 20.,
            bold: false,
            anchor: "la".into(),
            fill: [35, 35, 35, 255],
            stroke: None,
            angle: 0.,
            frame_rotation: false,
        });
    }
    Ok(ops)
}
fn rounded_rect(image: &mut RgbaImage, b: [f64; 4], radius: f64, fill: [u8; 4]) {
    let r = radius
        .min((b[2] - b[0]) / 2.)
        .min((b[3] - b[1]) / 2.)
        .max(0.);
    if r == 0. {
        canvas::rect(image, b, fill);
        return;
    }
    canvas::rect(image, [b[0] + r, b[1], b[2] - r, b[3]], fill);
    canvas::rect(image, [b[0], b[1] + r, b[2], b[3] - r], fill);
    for (x, y) in [
        (b[0], b[1]),
        (b[2] - 2. * r, b[1]),
        (b[0], b[3] - 2. * r),
        (b[2] - 2. * r, b[3] - 2. * r),
    ] {
        canvas::ellipse(image, [x, y, x + 2. * r, y + 2. * r], fill);
    }
}
pub(crate) fn draw(
    image: &mut RgbaImage,
    ops: &[Op],
    fonts: &mut Fonts,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let mut budget = 100_000_000;
    for op in ops {
        input::check(cancel)?;
        match op {
            Op::Line {
                points,
                fill,
                width,
            } => canvas::line(image, points, *width, *fill, cancel, &mut budget)?,
            Op::Polygon { points, fill } => canvas::polygon(image, points, *fill),
            Op::Rect {
                bounds,
                fill,
                outline,
                width,
                radius,
            } => {
                if let Some(c) = fill {
                    if let Some(c) = outline {
                        rounded_rect(image, *bounds, *radius, *c);
                    }
                    let d = if outline.is_some() { *width as f64 } else { 0. };
                    rounded_rect(
                        image,
                        [bounds[0] + d, bounds[1] + d, bounds[2] - d, bounds[3] - d],
                        (*radius - d).max(0.),
                        *c,
                    );
                } else if let Some(c) = outline {
                    let [l, t, r, b] = *bounds;
                    let w = *width as f64;
                    canvas::rect(image, [l, t, r, t + w - 1.], *c);
                    canvas::rect(image, [l, b - w + 1., r, b], *c);
                    canvas::rect(image, [l, t, l + w - 1., b], *c);
                    canvas::rect(image, [r - w + 1., t, r, b], *c);
                }
            }
            Op::Ellipse {
                bounds,
                fill,
                outline,
                width,
            } => {
                canvas::ellipse(image, *bounds, *outline);
                let d = *width as f64;
                canvas::ellipse(
                    image,
                    [bounds[0] + d, bounds[1] + d, bounds[2] - d, bounds[3] - d],
                    *fill,
                );
            }
            Op::Text {
                position,
                text,
                size,
                bold,
                anchor,
                fill,
                stroke,
                angle,
                frame_rotation,
            } => {
                if *frame_rotation {
                    let bbox = fonts.measure(text, *size, *bold)?;
                    let mut temp = crate::raster_rotation::allocate(640, 220)?;
                    fonts.draw(
                        &mut temp,
                        text,
                        [
                            (640. - (bbox[2] - bbox[0])) / 2.,
                            (220. - (bbox[3] - bbox[1])) / 2.,
                        ],
                        *size,
                        *bold,
                        "la",
                        *fill,
                        None,
                        0.,
                        cancel,
                    )?;
                    let temp = crate::raster_rotation::rotate(temp, *angle, cancel)?;
                    let ox = (position[0] - temp.width() as f64 / 2.) as i32;
                    let oy = (position[1] - temp.height() as f64 / 2.) as i32;
                    for y in 0..temp.height() {
                        for x in 0..temp.width() {
                            canvas::pixel(
                                image,
                                ox + x as i32,
                                oy + y as i32,
                                temp.get_pixel(x, y).0,
                                true,
                            );
                        }
                    }
                } else {
                    fonts.draw(
                        image, text, *position, *size, *bold, anchor, *fill, *stroke, *angle,
                        cancel,
                    )?;
                }
            }
        }
    }
    Ok(())
}
