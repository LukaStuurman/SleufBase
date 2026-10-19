//! Same prepared geometry/probe corpus as the original Python benchmark.
use sleufbase_rust::{
    bgt_surface::{Feature, Index},
    profile::Profile,
    Vertex,
};
use std::{sync::atomic::AtomicBool, time::Instant};
#[derive(serde::Deserialize)]
struct Case {
    start: [f64; 2],
    end: [f64; 2],
    axis_length: f64,
    features: Vec<Feature>,
}
fn hash(h: &mut u64, text: Option<&str>) {
    for b in text.unwrap_or("").bytes().chain([255]) {
        *h = (*h ^ b as u64).wrapping_mul(0x100000001b3);
    }
}
fn main() -> Result<(), String> {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("../tests/fixtures/bgt-surface/reference.json"))
            .map_err(|e| e.to_string())?;
    let profiles = cases
        .iter()
        .map(|c| Profile {
            start: Vertex {
                x: c.start[0],
                y: c.start[1],
                z: None,
            },
            end: Vertex {
                x: c.end[0],
                y: c.end[1],
                z: None,
            },
            axis_dx: c.end[0] - c.start[0],
            axis_dy: c.end[1] - c.start[1],
            axis_length: c.axis_length,
            reference_level: 0.,
            points: Vec::new(),
        })
        .collect::<Vec<_>>();
    let cancel = AtomicBool::new(false);
    let begin = Instant::now();
    let indices = cases
        .iter()
        .map(|c| Index::new(&c.features, &cancel))
        .collect::<Result<Vec<_>, _>>()?;
    let prepare_ms = begin.elapsed().as_secs_f64() * 1000.;
    let begin = Instant::now();
    let mut h = 0xcbf29ce484222325u64;
    let loops = 10;
    for _ in 0..loops {
        for ((c, index), profile) in cases.iter().zip(&indices).zip(&profiles) {
            hash(&mut h, index.dominant(c.start, c.end, &cancel)?);
            let texts = index.texts(profile, &cancel)?;
            for key in ["start", "middle", "end"] {
                hash(&mut h, texts.get(key).and_then(|v| v.as_str()));
            }
        }
    }
    let queries_ms = begin.elapsed().as_secs_f64() * 1000.;
    println!(
        "{}",
        serde_json::json!({"cases":cases.len(),"loops":loops,"prepare_ms":prepare_ms,"queries_ms":queries_ms,"checksum":format!("{h:016x}")})
    );
    Ok(())
}
