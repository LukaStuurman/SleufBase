//! Cubic control geometry shared by native CAD stroke fonts.
//! Ported from ezdxf 1.4.4 (MIT, Manfred Moitzi); retain third_party/ezdxf-LICENSE.txt.
pub(crate) type Point = [f64; 2];
fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1]]
}
fn mul(a: Point, s: f64) -> Point {
    [a[0] * s, a[1] * s]
}
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-15_f64.max(1e-9 * a.abs().max(b.abs()))
}
fn span(start: f64, mut end: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    if close(start, end) {
        return 0.;
    }
    let start = start.rem_euclid(tau);
    if close(start, end.rem_euclid(tau)) {
        return tau;
    }
    if !close(end, tau) {
        end = end.rem_euclid(tau);
    }
    if end < start {
        end += tau;
    }
    end - start
}
pub(crate) fn ellipse(
    center: Point,
    radius: f64,
    start: f64,
    end: f64,
) -> Result<Vec<[Point; 4]>, String> {
    let span = span(start, end);
    if span.abs() < 1e-9 {
        return Ok(Vec::new());
    }
    // Keep the source's global-axis parameterization and segment rounding.
    // Rotating a local basis changes quarter-boundary rounding and fast bounds.
    let mut angle = start.rem_euclid(std::f64::consts::TAU);
    let delta = (angle + span) - angle;
    let count = (delta / std::f64::consts::PI * 2.).ceil().max(1.) as usize;
    if count > 8 || !radius.is_finite() || radius <= 0. {
        return Err("Ongeldige CAD-letterboog".into());
    }
    let step = delta / count as f64;
    let k = 4. / 3. * (step / 4.).tan();
    let transform = |p: Point| add(center, mul(p, radius));
    let mut point = [angle.cos(), angle.sin()];
    let mut curves = Vec::with_capacity(count);
    for _ in 0..count {
        let begin = point;
        angle += step;
        point = [angle.cos(), angle.sin()];
        curves.push([
            transform(begin),
            transform(add(begin, [-begin[1] * k, begin[0] * k])),
            transform(add(point, [point[1] * k, -point[0] * k])),
            transform(point),
        ]);
    }
    Ok(curves)
}
