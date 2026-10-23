//! Explicit rotation contracts through the actual native production functions.
use serde::Deserialize;
use serde_json::json;
use sleufbase_rust::{
    geo::ViewportTransform, geotiff::GeoTiffInfo, geotiff_rotation as rotation, Bounds,
};
#[path = "support/atomic_json.rs"]
mod atomic_json;
#[derive(Deserialize)]
struct Viewport {
    bounds: Bounds,
    width_px: u32,
    height_px: u32,
}
#[derive(Deserialize)]
struct Step {
    angle: f64,
    picks: Vec<[f64; 2]>,
    angle_center: [f64; 2],
}
#[derive(Deserialize)]
struct Move {
    pointer: [f64; 2],
}
#[derive(Deserialize)]
struct Drag {
    initial: f64,
    center: [f64; 2],
    press: [f64; 2],
    moves: Vec<Move>,
}
#[derive(Deserialize)]
struct Request {
    input: GeoTiffInfo,
    viewport: Viewport,
    steps: Vec<Step>,
    drag: Drag,
}
fn main() -> Result<(), String> {
    atomic_json::run(2048, |request: Request, out| {
        let result = (|| {
            if request.steps.len() > 32 || request.drag.moves.len() > 1000 {
                return Err("Te veel rotatiestappen".into());
            }
            let mut raster = request.input;
            let v = request.viewport;
            let viewport = ViewportTransform {
                bounds: v.bounds,
                width_px: v.width_px,
                height_px: v.height_px,
            };
            viewport.world_to_screen(0., 0.)?;
            let mut steps = Vec::new();
            for step in request.steps {
                rotation::set(&mut raster, step.angle)?;
                let overlay = rotation::overlay(&raster, |p| {
                    let (x, y) = viewport.world_to_screen(p[0], p[1]).unwrap();
                    [x, y]
                })?;
                let hits = step
                    .picks
                    .iter()
                    .map(|&p| rotation::hit(&overlay, p))
                    .collect::<Vec<_>>();
                let screen_angles = step
                    .picks
                    .iter()
                    .map(|&p| rotation::screen_angle(step.angle_center, p))
                    .collect::<Vec<_>>();
                steps.push(json!({"expected":raster,"overlay":overlay,"hits":hits,"screen_angles":screen_angles}));
            }
            let d = request.drag;
            let start = rotation::screen_angle(d.center, d.press);
            let drag = d
                .moves
                .into_iter()
                .map(|m| {
                    rotation::dragged(
                        d.initial,
                        start,
                        rotation::screen_angle(d.center, m.pointer),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok::<_, String>(json!({"steps":steps,"drag":drag}))
        })();
        serde_json::to_writer(out, &result.unwrap_or_else(|e| json!({"error":e})))
            .map_err(|e| e.to_string())
    })
}
