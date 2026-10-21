//! Fresh-drawing audit of the actual native import and cached map scene.
use serde::{Deserialize, Serialize};
use sleufbase_rust::{
    cad_text::Attribute, dxf_fonts::Catalog, dxf_read::parse_dxf_bytes_with_catalog,
};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    directories: Vec<PathBuf>,
    drawings: Vec<PathBuf>,
}
#[derive(Serialize)]
struct Node<'a> {
    kind: &'a str,
    handle: &'a str,
    blockpath: String,
    layer: &'a str,
    rgb: [u8; 3],
    visible: bool,
    attribute: Option<&'a Attribute>,
    content: &'a str,
    paths: Vec<Vec<[f64; 3]>>,
}
fn read(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Scene audit input budget exceeded".into());
    }
    Ok(bytes)
}
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("input manifest and fresh output JSON required".into());
    }
    let input: Request = serde_json::from_slice(&read(Path::new(&args[0]), 64 * 1024 * 1024)?)
        .map_err(|e| e.to_string())?;
    if input.drawings.len() > 32 {
        return Err("Scene audit drawing budget exceeded".into());
    }
    let cancel = AtomicBool::new(false);
    let mut catalog = Catalog::from_dirs(&input.directories, &cancel)?;
    let file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&args[1])
        .map_err(|e| e.to_string())?;
    let mut out = std::io::BufWriter::new(file);
    out.write_all(b"[").map_err(|e| e.to_string())?;
    let mut points = 0usize;
    for (drawing, path) in input.drawings.iter().enumerate() {
        let overlay = parse_dxf_bytes_with_catalog(
            &read(path, 256 * 1024 * 1024)?,
            path,
            &cancel,
            &mut catalog,
        )?;
        let indices = overlay
            .features
            .iter()
            .enumerate()
            .filter(|(_, f)| f.entity_type != "INSERT")
            .map(|(i, _)| i)
            .chain(
                overlay.features.len()..overlay.features.len() + overlay.expanded.features.len(),
            );
        let mut nodes = Vec::new();
        for i in indices {
            let draw = overlay.inspect_feature(i).ok_or("Missing scene source")?;
            points = points
                .checked_add(draw.points.len())
                .filter(|&n| n <= 1_000_000)
                .ok_or("Scene audit point budget exceeded")?;
            let details = overlay.inspect_details(i).ok_or("Missing scene details")?;
            nodes.push(Node {
                kind: &draw.source.entity_type,
                handle: &draw.source.handle,
                blockpath: details.get("Blokpad").cloned().unwrap_or_default(),
                layer: draw.layer,
                rgb: draw.rgb,
                visible: draw.visible,
                attribute: draw
                    .source
                    .cad
                    .as_deref()
                    .and_then(|p| p.text.as_deref())
                    .and_then(|t| t.attribute.as_deref()),
                content: &draw.source.raw_text,
                paths: draw
                    .paths()
                    .filter(|p| !p.is_empty())
                    .map(|p| p.iter().map(|p| [p.x, p.y, p.z.unwrap_or(0.)]).collect())
                    .collect(),
            });
        }
        if drawing > 0 {
            out.write_all(b",").map_err(|e| e.to_string())?;
        }
        serde_json::to_writer(&mut out, &nodes).map_err(|e| e.to_string())?;
    }
    out.write_all(b"]")
        .and_then(|_| out.flush())
        .map_err(|e| e.to_string())
}
