//! Affine GeoTIFF and map viewport geometry, matched to models.py/geotiff.py.

use crate::model::Bounds;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeoTransform {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl GeoTransform {
    pub fn pixel_to_world(self, col: f64, row: f64) -> (f64, f64) {
        (
            self.a.mul_add(col, self.b.mul_add(row, self.c)),
            self.d.mul_add(col, self.e.mul_add(row, self.f)),
        )
    }

    pub fn world_to_pixel(self, x: f64, y: f64) -> Result<(f64, f64), String> {
        let determinant = self.a * self.e - self.b * self.d;
        if !determinant.is_finite() || determinant.abs() <= 1e-15 {
            return Err("GeoTIFF-transformatie is singulier".into());
        }
        let dx = x - self.c;
        let dy = y - self.f;
        let inverse = 1.0 / determinant;
        Ok(((self.e * dx - self.b * dy) * inverse,
            (-self.d * dx + self.a * dy) * inverse))
    }

    pub fn bounds(self, width: u32, height: u32) -> Result<Bounds, String> {
        if width == 0 || height == 0 {
            return Err("GeoTIFF-afmetingen moeten positief zijn".into());
        }
        let corners = [
            self.pixel_to_world(0.0, 0.0),
            self.pixel_to_world(f64::from(width), 0.0),
            self.pixel_to_world(f64::from(width), f64::from(height)),
            self.pixel_to_world(0.0, f64::from(height)),
        ];
        let bounds = Bounds {
            min_x: corners.iter().map(|v| v.0).fold(f64::INFINITY, f64::min),
            min_y: corners.iter().map(|v| v.1).fold(f64::INFINITY, f64::min),
            max_x: corners.iter().map(|v| v.0).fold(f64::NEG_INFINITY, f64::max),
            max_y: corners.iter().map(|v| v.1).fold(f64::NEG_INFINITY, f64::max),
        };
        if [bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y]
            .iter().any(|v| !v.is_finite())
            || bounds.max_x <= bounds.min_x
            || bounds.max_y <= bounds.min_y
        {
            return Err("Ongeldige of lege GeoTIFF-grenzen".into());
        }
        Ok(bounds)
    }

    /// Rotate about the georeferenced image center, not the pixel origin.
    /// Matches Python geotiff.set_geotiff_layer_rotation (absolute angle).
    pub fn rotate_degrees(self, degrees: f64, width: u32, height: u32) -> Result<Self, String> {
        if !degrees.is_finite() {
            return Err("Ongeldige rotatiehoek".into());
        }
        let (cx, cy) = self.pixel_to_world(f64::from(width)/2.0, f64::from(height)/2.0);
        let angle = degrees.to_radians();
        let (s, c) = angle.sin_cos();
        let rotate = |x: f64, y: f64| (
            cx + c * (x - cx) - s * (y - cy),
            cy + s * (x - cx) + c * (y - cy)
        );
        let origin = rotate(self.c, self.f);
        Ok(Self {
            a: c*self.a - s*self.d,
            b: c*self.b - s*self.e,
            c: origin.0,
            d: s*self.a + c*self.d,
            e: s*self.b + c*self.e,
            f: origin.1
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ViewportTransform {
    pub bounds: Bounds,
    pub width_px: u32,
    pub height_px: u32,
}

impl ViewportTransform {
    fn validated(self) -> Result<Self, String> {
        let b = self.bounds;
        if self.width_px == 0 || self.height_px == 0 ||
            !b.min_x.is_finite() || !b.min_y.is_finite() ||
            !b.max_x.is_finite() || !b.max_y.is_finite() ||
            b.max_x <= b.min_x || b.max_y <= b.min_y
        {
            return Err("Viewport-afmetingen/grenzen zijn ongeldig".into());
        }
        Ok(self)
    }
    pub fn world_to_screen(self, x: f64, y: f64) -> Result<(f64, f64), String> {
        let v = self.validated()?;
        Ok(((x-v.bounds.min_x)/(v.bounds.max_x-v.bounds.min_x)*f64::from(v.width_px),
            f64::from(v.height_px)-(y-v.bounds.min_y)/(v.bounds.max_y-v.bounds.min_y)*f64::from(v.height_px)))
    }
    pub fn screen_to_world(self, x: f64, y: f64) -> Result<(f64, f64), String> {
        let v = self.validated()?;
        Ok((v.bounds.min_x+x/f64::from(v.width_px)*(v.bounds.max_x-v.bounds.min_x),
            v.bounds.min_y+(f64::from(v.height_px)-y)/f64::from(v.height_px)*(v.bounds.max_y-v.bounds.min_y)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> GeoTransform {
        GeoTransform { a: 0.05,b:0.0,c:168000.0,d:0.0,e:-0.05,f:401000.0 }
    }
    #[test]
    fn geo_roundtrip() {
        let t = sample();
        let world = t.pixel_to_world(234.0, 454.0);
        let p = t.world_to_pixel(world.0, world.1).unwrap();
        assert!((p.0 - 234.0).abs() < 1e-6 && (p.1 - 454.0).abs() < 1e-6);
        let bounds = t.bounds(400, 200).unwrap();
        assert_eq!(bounds.min_x, 168000.0);
        assert_eq!(bounds.max_y, 401000.0);
    }
    #[test]
    fn rotation_keeps_center() {
        let t = sample();
        let center = t.pixel_to_world(100.0, 200.0);
        let rotated = t.rotate_degrees(90.0, 200, 400).unwrap();
        let rc = rotated.pixel_to_world(100.0, 200.0);
        assert!((center.0 - rc.0).abs() < 1e-6);
        assert!((center.1 - rc.1).abs() < 1e-6);
        assert!(rotated.bounds(200,400).is_ok());
    }
    #[test]
    fn singular_transform_rejected() {
        let t = GeoTransform {a:0.0,b:0.0,c:0.0,d:0.0,e:0.0,f:0.0};
        assert!(t.world_to_pixel(0.0,0.0).is_err());
    }
    #[test]
    fn viewport_roundtrip() {
        let v = ViewportTransform {
            bounds: Bounds { min_x:5.,min_y:10.,max_x:105.,max_y:60. },
            width_px:1000,height_px:500
        };
        let screen = v.world_to_screen(30.0,20.0).unwrap();
        let world = v.screen_to_world(screen.0,screen.1).unwrap();
        assert!((world.0-30.0).abs() < 1e-9);
        assert!((world.1-20.0).abs() < 1e-9);
    }
}