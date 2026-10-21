//! Independent compiled-driver audit of actual vector glyphs, not font boxes.
use serde::Deserialize;
use sleufbase_rust::{
    cad_text::{Renderer, Source},
    dxf_fonts::Catalog,
    dxf_text_metrics::Text,
};
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::atomic::AtomicBool,
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    font: String,
    text: Text,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    directories: Vec<PathBuf>,
    texts: Vec<Entry>,
}
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("input manifest and fresh output JSON required".into());
    }
    let mut data = Vec::new();
    std::fs::File::open(&args[0])
        .map_err(|e| e.to_string())?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > 64 * 1024 * 1024 {
        return Err("Vector audit manifest too large".into());
    }
    let request: Request = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    if request.texts.len() > 100_000 {
        return Err("Vector audit request budget exceeded".into());
    }
    let cancel = AtomicBool::new(false);
    let mut catalog = Catalog::from_dirs(&request.directories, &cancel)?;
    let mut renderer = Renderer::new(&mut catalog, &cancel);
    let mut output = Vec::new();
    for entry in request.texts {
        let mut source = Source {
            layout: entry.text,
            style: "Standard".into(),
            font: entry.font,
            big_font: String::new(),
            attribute: None,
            contours: Default::default(),
        };
        renderer.prepare(&mut source)?;
        output.push(source.contours);
    }
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])
        .map_err(|e| e.to_string())?;
    let mut writer = std::io::BufWriter::new(file);
    serde_json::to_writer(&mut writer, &output).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())
}
