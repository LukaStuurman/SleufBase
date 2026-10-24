//! Original cadastral overview mask/component/contour and centerline helpers.
use crate::{cadastral::CadLinework, geo::GeoTransform, input, Bounds};
use image::RgbaImage;
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::AtomicBool,
};
type P = [f64; 2];
struct Mask {
    w: usize,
    h: usize,
    data: Vec<bool>,
}
fn allocate<T: Clone>(n: usize, value: T) -> Result<Vec<T>, String> {
    let mut data = Vec::new();
    data.try_reserve_exact(n)
        .map_err(|_| "Onvoldoende geheugen voor het volledige bronmasker")?;
    data.resize(n, value);
    Ok(data)
}
fn push<T>(data: &mut Vec<T>, value: T, limit: usize) -> Result<(), String> {
    if data.len() == data.capacity() {
        let growth = data.len().max(4096).min(limit.saturating_sub(data.len()));
        if growth == 0 {
            return Err("Bronmasker overschrijdt het pixelbudget".into());
        }
        data.try_reserve_exact(growth)
            .map_err(|_| "Onvoldoende geheugen voor maskercomponent")?;
    }
    data.push(value);
    Ok(())
}
fn collect<T>(
    items: impl Iterator<Item = T>,
    limit: usize,
    cancel: &AtomicBool,
) -> Result<Vec<T>, String> {
    let mut data = Vec::new();
    for item in items {
        if data.len() % 16384 == 0 {
            input::check(cancel)?;
        }
        push(&mut data, item, limit)?;
    }
    Ok(data)
}
impl Mask {
    fn empty(w: usize, h: usize) -> Result<Self, String> {
        let n = w.checked_mul(h).ok_or("Maskerafmetingen lopen over")?;
        if n == 0 || n as u64 > crate::geotiff::MAX_GEOTIFF_PIXELS {
            return Err("Bronmasker overschrijdt het bestaande GeoTIFF-pixelbudget".into());
        }
        Ok(Self {
            w,
            h,
            data: allocate(n, false)?,
        })
    }
    fn get(&self, x: isize, y: isize) -> bool {
        x >= 0
            && y >= 0
            && (x as usize) < self.w
            && (y as usize) < self.h
            && self.data[y as usize * self.w + x as usize]
    }
    fn neighbors(&self, i: usize) -> [Option<usize>; 4] {
        let x = i % self.w;
        let y = i / self.w;
        [
            y.checked_sub(1).map(|y| y * self.w + x),
            (y + 1 < self.h).then_some((y + 1) * self.w + x),
            x.checked_sub(1).map(|x| y * self.w + x),
            (x + 1 < self.w).then_some(i + 1),
        ]
    }
    fn component(
        &self,
        seed: usize,
        seen: &mut [bool],
        cancel: &AtomicBool,
    ) -> Result<Vec<usize>, String> {
        let mut stack = Vec::new();
        let mut result = Vec::new();
        push(&mut stack, seed, self.data.len())?;
        seen[seed] = true;
        while let Some(i) = stack.pop() {
            if result.len() % 16384 == 0 {
                input::check(cancel)?;
            }
            push(&mut result, i, self.data.len())?;
            for j in self.neighbors(i).into_iter().flatten() {
                if self.data[j] && !seen[j] {
                    seen[j] = true;
                    push(&mut stack, j, self.data.len())?;
                }
            }
        }
        Ok(result)
    }
    fn selected(&self, ids: &[usize], cancel: &AtomicBool) -> Result<Self, String> {
        let mut out = Self::empty(self.w, self.h)?;
        for (n, &i) in ids.iter().enumerate() {
            if n % 16384 == 0 {
                input::check(cancel)?;
            }
            out.data[i] = true;
        }
        Ok(out)
    }
    fn alpha_component(&self, cancel: &AtomicBool) -> Result<Option<Self>, String> {
        let cx = (self.w as f64 - 1.) / 2.;
        let cy = (self.h as f64 - 1.) / 2.;
        let mut best = f64::INFINITY;
        let mut seed = None;
        for i in 0..self.data.len() {
            if i % 16384 == 0 {
                input::check(cancel)?;
            }
            if self.data[i] {
                let d = ((i % self.w) as f64 - cx).powi(2) + ((i / self.w) as f64 - cy).powi(2);
                if d < best {
                    best = d;
                    seed = Some(i);
                }
            }
        }
        let Some(seed) = seed else { return Ok(None) };
        let mut seen = allocate(self.data.len(), false)?;
        let ids = self.component(seed, &mut seen, cancel)?;
        Ok(Some(self.selected(&ids, cancel)?))
    }
    fn morphology(&self, dilate: bool, cancel: &AtomicBool) -> Result<Self, String> {
        let mut out = Self::empty(self.w, self.h)?;
        for y in 0..self.h {
            if y % 64 == 0 {
                input::check(cancel)?;
            }
            for x in 0..self.w {
                if x % 16384 == 0 {
                    input::check(cancel)?;
                }
                let values = (-1..=1).flat_map(|dy| {
                    (-1..=1).map(move |dx| self.get(x as isize + dx, y as isize + dy))
                });
                out.data[y * self.w + x] = if dilate {
                    values.into_iter().any(|v| v)
                } else {
                    values.into_iter().all(|v| v)
                };
            }
        }
        Ok(out)
    }
    fn fill(&self, cancel: &AtomicBool) -> Result<Self, String> {
        let mut outside = allocate(self.data.len(), false)?;
        let mut stack = Vec::new();
        for i in 0..self.data.len() {
            if i % 16384 == 0 {
                input::check(cancel)?;
            }
            let x = i % self.w;
            let y = i / self.w;
            if (x == 0 || y == 0 || x + 1 == self.w || y + 1 == self.h)
                && !self.data[i]
                && !outside[i]
            {
                outside[i] = true;
                push(&mut stack, i, self.data.len())?;
            }
        }
        let mut nodes = 0usize;
        while let Some(i) = stack.pop() {
            if nodes % 16384 == 0 {
                input::check(cancel)?;
            }
            nodes += 1;
            for j in self.neighbors(i).into_iter().flatten() {
                if !self.data[j] && !outside[j] {
                    outside[j] = true;
                    push(&mut stack, j, self.data.len())?;
                }
            }
        }
        for (n, v) in outside.iter_mut().enumerate() {
            if n % 16384 == 0 {
                input::check(cancel)?;
            }
            *v = !*v;
        }
        Ok(Self {
            w: self.w,
            h: self.h,
            data: outside,
        })
    }
    fn best(&self, cancel: &AtomicBool) -> Result<Option<Self>, String> {
        let mut seen = allocate(self.data.len(), false)?;
        let mut best = None;
        let mut score = f64::INFINITY;
        for i in 0..self.data.len() {
            if i % 16384 == 0 {
                input::check(cancel)?;
            }
            if !self.data[i] || seen[i] {
                continue;
            }
            let ids = self.component(i, &mut seen, cancel)?;
            let ratio = ids.len() as f64 / self.data.len() as f64;
            if !(0.005..=0.45).contains(&ratio) {
                continue;
            }
            let (mut x0, mut y0, mut x1, mut y1) = (self.w, self.h, 0, 0);
            let (mut sx, mut sy) = (0., 0.);
            for (n, &j) in ids.iter().enumerate() {
                if n % 16384 == 0 {
                    input::check(cancel)?;
                }
                let x = j % self.w;
                let y = j / self.w;
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
                sx += x as f64;
                sy += y as f64;
            }
            let (w, h) = ((x1 - x0 + 1) as f64, (y1 - y0 + 1) as f64);
            let n = ids.len() as f64;
            let candidate = (sx / n - self.w as f64 / 2.).hypot(sy / n - self.h as f64 / 2.)
                + (w.max(h) / w.min(h) - 2.).abs() * 18.
                + (n / (w * h) - 0.55).abs() * 80.;
            if candidate < score {
                score = candidate;
                best = Some(self.selected(&ids, cancel)?);
            }
        }
        Ok(best)
    }
}
fn masks(
    image: &RgbaImage,
    prefer_trench: bool,
    cancel: &AtomicBool,
) -> Result<Option<Mask>, String> {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let alpha = Mask {
        w,
        h,
        data: collect(image.pixels().map(|p| p[3] > 15), w * h, cancel)?,
    };
    if !alpha.data.iter().any(|v| *v) {
        return Ok(None);
    }
    if prefer_trench {
        let gray: Vec<f32> = collect(
            image.pixels().map(|p| {
                p[0] as f32 * 0.299_f32 + p[1] as f32 * 0.587_f32 + p[2] as f32 * 0.114_f32
            }),
            w * h,
            cancel,
        )?;
        let mut values: Vec<f32> = collect(
            (0..w * h)
                .filter(|&i| {
                    alpha.data[i]
                        && i / w >= (h as f64 * 0.2) as usize
                        && i / w < (h as f64 * 0.8) as usize
                        && i % w >= (w as f64 * 0.2) as usize
                        && i % w < (w as f64 * 0.8) as usize
                })
                .map(|i| gray[i]),
            w * h,
            cancel,
        )?;
        if values.len() < 64 {
            values = collect(
                (0..w * h).filter(|&i| alpha.data[i]).map(|i| gray[i]),
                w * h,
                cancel,
            )?;
        }
        input::check(cancel)?;
        let q = (values.len() - 1) as f64 * 0.2;
        let low = q.floor() as usize;
        let high = q.ceil() as usize;
        values.select_nth_unstable_by(low, f32::total_cmp);
        input::check(cancel)?;
        let lower = values[low];
        let upper = if high == low {
            lower
        } else {
            *values[low + 1..]
                .iter()
                .min_by(|a, b| a.total_cmp(b))
                .ok_or("Percentiel ontbreekt")?
        };
        let percentile = lower as f64 + (upper as f64 - lower as f64) * (q - low as f64);
        let base = (percentile + 18.).clamp(85., 145.);
        for threshold in [base, (base + 12.).min(155.), (base + 24.).min(165.)] {
            input::check(cancel)?;
            let candidate = Mask {
                w,
                h,
                data: collect(
                    (0..w * h).map(|i| alpha.data[i] && gray[i] as f64 <= threshold),
                    w * h,
                    cancel,
                )?,
            };
            let opened = candidate
                .morphology(false, cancel)?
                .morphology(true, cancel)?;
            if let Some(best) = opened.best(cancel)? {
                let filled = best
                    .fill(cancel)?
                    .morphology(true, cancel)?
                    .morphology(false, cancel)?;
                let ratio = filled.data.iter().filter(|v| **v).count() as f64 / (w * h) as f64;
                if (0.005..=0.45).contains(&ratio) {
                    return Ok(Some(filled));
                }
            }
        }
    }
    alpha.alpha_component(cancel)
}
pub fn polygon(
    image: &RgbaImage,
    tx: GeoTransform,
    bounds: Bounds,
    cancel: &AtomicBool,
) -> Result<Vec<P>, String> {
    input::check(cancel)?;
    let Some(mask) = masks(image, false, cancel)? else {
        return Ok(rectangle(bounds));
    };
    // Keep first-seen edge order (Python dict insertion order), including ties.
    let mut edges: Vec<((usize, usize), (usize, usize))> = Vec::new();
    for y in 0..mask.h {
        if y % 128 == 0 {
            input::check(cancel)?;
        }
        for x in 0..mask.w {
            if x % 16384 == 0 {
                input::check(cancel)?;
            }
            if !mask.get(x as isize, y as isize) {
                continue;
            }
            if !mask.get(x as isize, y as isize - 1) {
                push(&mut edges, ((x, y), (x + 1, y)), mask.data.len() * 4)?;
            }
            if !mask.get(x as isize + 1, y as isize) {
                push(
                    &mut edges,
                    ((x + 1, y), (x + 1, y + 1)),
                    mask.data.len() * 4,
                )?;
            }
            if !mask.get(x as isize, y as isize + 1) {
                push(
                    &mut edges,
                    ((x + 1, y + 1), (x, y + 1)),
                    mask.data.len() * 4,
                )?;
            }
            if !mask.get(x as isize - 1, y as isize) {
                push(&mut edges, ((x, y + 1), (x, y)), mask.data.len() * 4)?;
            }
        }
    }
    let mut adjacency: HashMap<_, Vec<_>> = HashMap::new();
    adjacency
        .try_reserve(edges.len())
        .map_err(|_| "Onvoldoende geheugen voor maskercontouren")?;
    let mut order = Vec::new();
    for (n, &(a, b)) in edges.iter().enumerate() {
        if n % 16384 == 0 {
            input::check(cancel)?;
        }
        if !adjacency.contains_key(&a) {
            push(&mut order, a, edges.len())?;
        }
        adjacency.entry(a).or_default().push(b);
    }
    let mut used = HashSet::new();
    used.try_reserve(edges.len())
        .map_err(|_| "Onvoldoende geheugen voor maskercontouren")?;
    let mut largest = Vec::new();
    let mut area = 0.;
    for start in order {
        for &end in &adjacency[&start] {
            if !used.insert((start, end)) {
                continue;
            }
            let mut path = vec![start, end];
            let mut current = end;
            while current != start {
                if path.len() % 16384 == 0 {
                    input::check(cancel)?;
                }
                let next = adjacency
                    .get(&current)
                    .and_then(|v| v.iter().find(|&&p| !used.contains(&(current, p))))
                    .copied();
                let Some(next) = next else {
                    path.clear();
                    break;
                };
                used.insert((current, next));
                push(&mut path, next, adjacency.len() + 8)?;
                current = next;
                if path.len() > adjacency.len() + 7 {
                    path.clear();
                    break;
                }
            }
            if path.len() < 4 {
                continue;
            }
            path.pop();
            let a = path
                .iter()
                .zip(path.iter().cycle().skip(1))
                .map(|(a, b)| a.0 as f64 * b.1 as f64 - b.0 as f64 * a.1 as f64)
                .sum::<f64>()
                .abs();
            if a > area {
                area = a;
                largest = path;
            }
        }
    }
    if largest.len() < 3 {
        return Ok(rectangle(bounds));
    }
    let collinear = |a: (usize, usize), b: (usize, usize), c: (usize, usize)| {
        (a.0 == b.0 && b.0 == c.0) || (a.1 == b.1 && b.1 == c.1)
    };
    let mut simple = Vec::new();
    let limit = largest.len();
    for (n, p) in largest.into_iter().enumerate() {
        if n % 16384 == 0 {
            input::check(cancel)?;
        }
        push(&mut simple, p, limit)?;
        while simple.len() >= 3 {
            let n = simple.len();
            if collinear(simple[n - 3], simple[n - 2], simple[n - 1]) {
                simple.remove(n - 2);
            } else {
                break;
            }
        }
    }
    loop {
        let mut changed = false;
        let n = simple.len();
        if n >= 3 && collinear(simple[n - 2], simple[n - 1], simple[0]) {
            simple.pop();
            changed = true;
        }
        let n = simple.len();
        if n >= 3 && collinear(simple[n - 1], simple[0], simple[1]) {
            simple.remove(0);
            changed = true;
        }
        if !changed || simple.len() < 3 {
            break;
        }
    }
    Ok(simple
        .into_iter()
        .map(|(x, y)| {
            let (x, y) = tx.pixel_to_world(x as f64, y as f64);
            [x, y]
        })
        .collect())
}
pub fn rectangle(b: Bounds) -> Vec<P> {
    vec![
        [b.min_x, b.min_y],
        [b.max_x, b.min_y],
        [b.max_x, b.max_y],
        [b.min_x, b.max_y],
    ]
}
pub fn centerline(
    image: &RgbaImage,
    tx: GeoTransform,
    bounds: Bounds,
    linework: &[CadLinework],
    cancel: &AtomicBool,
) -> Result<Vec<P>, String> {
    input::check(cancel)?;
    let visible = masks(image, false, cancel)?;
    if let Some(mask) = &visible {
        let center = [image.width() as f64 / 2., image.height() as f64 / 2.];
        let world = tx.pixel_to_world(center[0], center[1]);
        let mut best = None;
        let mut distance = f64::INFINITY;
        for group in linework {
            input::check(cancel)?;
            for path in &group.paths {
                for (n, pair) in path.windows(2).enumerate() {
                    if n % 16384 == 0 {
                        input::check(cancel)?;
                    }
                    let (a, b) = (pair[0], pair[1]);
                    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                    let len = dx * dx + dy * dy;
                    if len <= 1e-12 {
                        continue;
                    }
                    let f = ((world.0 - a.0) * dx + (world.1 - a.1) * dy) / len;
                    if !(0.0..=1.0).contains(&f) {
                        continue;
                    }
                    let p = (a.0 + f * dx, a.1 + f * dy);
                    let d = (p.0 - world.0).powi(2) + (p.1 - world.1).powi(2);
                    if d < distance {
                        best = Some(p);
                        distance = d;
                    }
                }
            }
        }
        if let Some(p) = best {
            let p = tx.world_to_pixel(p.0, p.1)?;
            let mut direction = [p.0 - center[0], p.1 - center[1]];
            let n = direction[0].hypot(direction[1]);
            if n > 1e-6 {
                direction[0] /= n;
                direction[1] /= n;
                let span = (mask.w as f64).hypot(mask.h as f64) + 2.;
                let mut t = -span;
                let mut start = None;
                let mut runs = Vec::new();
                let mut previous = None;
                let mut samples = 0usize;
                while t <= span + 1e-9 {
                    if samples % 16384 == 0 {
                        input::check(cancel)?;
                    }
                    samples += 1;
                    let x = (center[0] + direction[0] * t).round_ties_even() as isize;
                    let y = (center[1] + direction[1] * t).round_ties_even() as isize;
                    let inside = mask.get(x, y);
                    if inside && start.is_none() {
                        start = Some(t);
                    } else if !inside && start.is_some() {
                        runs.push((start.take().unwrap(), previous.unwrap()));
                    }
                    previous = Some(t);
                    t += 0.25;
                }
                if let Some(start) = start {
                    runs.push((start, previous.unwrap()));
                }
                let run = runs.iter().find(|&&(a, b)| a <= 0. && b >= 0.).or_else(|| {
                    runs.iter().min_by(|a, b| {
                        a.0.abs()
                            .min(a.1.abs())
                            .total_cmp(&b.0.abs().min(b.1.abs()))
                            .then_with(|| (b.1 - b.0).total_cmp(&(a.1 - a.0)))
                    })
                });
                if let Some(&(a, b)) = run {
                    return Ok([a, b]
                        .into_iter()
                        .map(|t| {
                            let (x, y) = tx.pixel_to_world(
                                center[0] + direction[0] * t,
                                center[1] + direction[1] * t,
                            );
                            [x, y]
                        })
                        .collect());
                }
            }
        }
    }
    let mask = masks(image, true, cancel)?.or(visible);
    if let Some(mask) = mask {
        let points: Vec<P> = collect(
            (0..mask.data.len())
                .filter(|&i| mask.data[i])
                .map(|i| [(i % mask.w) as f64, (i / mask.w) as f64]),
            mask.data.len(),
            cancel,
        )?;
        if points.len() >= 2 {
            let n = points.len() as f64;
            let mut c = [0., 0.];
            for (i, p) in points.iter().enumerate() {
                if i % 16384 == 0 {
                    input::check(cancel)?;
                }
                c[0] += p[0];
                c[1] += p[1];
            }
            c[0] /= n;
            c[1] /= n;
            let (mut xx, mut xy, mut yy) = (0., 0., 0.);
            for (i, p) in points.iter().enumerate() {
                if i % 16384 == 0 {
                    input::check(cancel)?;
                }
                let x = p[0] - c[0];
                let y = p[1] - c[1];
                xx += x * x;
                xy += x * y;
                yy += y * y;
            }
            let angle = 0.5 * (2. * xy).atan2(xx - yy);
            let mut direction = [angle.cos(), angle.sin()];
            // LAPACK's SVD uses a negative first component for the first
            // right-singular vector (except a vertical axis with zero x).
            if direction[0].abs() > 1e-12 {
                direction[0] = -direction[0];
                direction[1] = -direction[1];
            }
            let projections: Vec<_> = collect(
                points
                    .iter()
                    .map(|p| (p[0] - c[0]) * direction[0] + (p[1] - c[1]) * direction[1]),
                points.len(),
                cancel,
            )?;
            let min = projections.iter().copied().fold(f64::INFINITY, f64::min);
            let max = projections
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
            let span = max - min;
            let endpoints = if span < 1e-6 {
                let x0 = points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
                let x1 = points
                    .iter()
                    .map(|p| p[0])
                    .fold(f64::NEG_INFINITY, f64::max);
                let y0 = points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
                let y1 = points
                    .iter()
                    .map(|p| p[1])
                    .fold(f64::NEG_INFINITY, f64::max);
                if x1 - x0 >= y1 - y0 {
                    [[x0, c[1]], [x1, c[1]]]
                } else {
                    [[c[0], y0], [c[0], y1]]
                }
            } else if span < 1. {
                [
                    [c[0] + direction[0] * min, c[1] + direction[1] * min],
                    [c[0] + direction[0] * max, c[1] + direction[1] * max],
                ]
            } else {
                let window = (span * 0.1).max(1.5);
                let mean = |first: bool| -> Result<P, String> {
                    let mut sum = [0., 0.];
                    let mut count = 0.;
                    for (i, (p, &q)) in points.iter().zip(&projections).enumerate() {
                        if i % 16384 == 0 {
                            input::check(cancel)?;
                        }
                        if (first && q <= min + window) || (!first && q >= max - window) {
                            sum[0] += p[0];
                            sum[1] += p[1];
                            count += 1.;
                        }
                    }
                    Ok([sum[0] / count, sum[1] / count])
                };
                [mean(true)?, mean(false)?]
            };
            return Ok(endpoints
                .into_iter()
                .map(|p| {
                    let (x, y) = tx.pixel_to_world(p[0] + 0.5, p[1] + 0.5);
                    [x, y]
                })
                .collect());
        }
    }
    Ok(
        if bounds.max_x - bounds.min_x >= bounds.max_y - bounds.min_y {
            vec![
                [bounds.min_x, (bounds.min_y + bounds.max_y) / 2.],
                [bounds.max_x, (bounds.min_y + bounds.max_y) / 2.],
            ]
        } else {
            vec![
                [(bounds.min_x + bounds.max_x) / 2., bounds.min_y],
                [(bounds.min_x + bounds.max_x) / 2., bounds.max_y],
            ]
        },
    )
}
