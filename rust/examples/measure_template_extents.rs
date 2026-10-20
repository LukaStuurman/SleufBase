//! Independent driver for actual local fonts and generated-profile fast bounds.
use serde::{Deserialize, Serialize};
use sleufbase_rust::{
    dxf_document::Document,
    dxf_text_metrics::{Font, Text},
    template_extents::Index,
};
use std::{collections::BTreeMap, io::Read, path::PathBuf, sync::atomic::AtomicBool};
#[derive(Deserialize)]
struct TextRequest {
    font: String,
    text: Text,
}
#[derive(Deserialize)]
struct Request {
    fonts: BTreeMap<String, PathBuf>,
    #[serde(default)]
    texts: Vec<TextRequest>,
    drawing: Option<PathBuf>,
    #[serde(default)]
    groups: Vec<Vec<u64>>,
}
#[derive(Serialize)]
struct Group {
    bounds: Option<sleufbase_rust::template_extents::Bounds>,
    selector: Option<[f64; 3]>,
}
fn main() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err("input manifest and fresh output JSON required".into());
    }
    let cancel = AtomicBool::new(false);
    let mut data = Vec::new();
    std::fs::File::open(&args[0])
        .map_err(|e| e.to_string())?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > 64 * 1024 * 1024 {
        return Err("Metric audit manifest too large".into());
    }
    let request: Request = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    if request.fonts.len() > 32 || request.texts.len() > 100_000 || request.groups.len() > 5000 {
        return Err("Metric audit budget exceeded".into());
    }
    let mut fonts = BTreeMap::new();
    let mut sources = BTreeMap::<PathBuf, Font>::new();
    let mut bytes = 0;
    for (name, path) in request.fonts {
        fonts.insert(
            name,
            if path.to_str() == Some("*monospace") {
                Font::monospace()
            } else {
                let path = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
                if let Some(f) = sources.get(&path) {
                    f.clone()
                } else {
                    let font = Font::load(&path, &cancel)?;
                    bytes += font.source_bytes();
                    if bytes > 256 * 1024 * 1024 {
                        return Err("Metric audit font budget exceeded".into());
                    }
                    sources.insert(path, font.clone());
                    font
                }
            },
        );
    }
    let mut texts = Vec::new();
    for t in request.texts {
        texts.push(
            fonts
                .get_mut(&t.font)
                .ok_or("Unknown audit font")?
                .extent(&t.text, &cancel)?,
        );
    }
    let mut groups = Vec::new();
    if let Some(path) = request.drawing {
        let doc = Document::load(&path, &cancel)?;
        let index = Index::new(&doc, &cancel)?;
        let mut resolve = |name: &str| {
            fonts
                .get(name)
                .cloned()
                .ok_or_else(|| format!("Unknown drawing font {name}"))
        };
        for ids in request.groups {
            groups.push(Group {
                bounds: index.bounds(&ids, &mut resolve, &cancel)?,
                selector: index.selector(&ids, &mut resolve, &cancel)?,
            });
        }
    } else if !request.groups.is_empty() {
        return Err("Groups require a drawing".into());
    }
    let out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])
        .map_err(|e| e.to_string())?;
    serde_json::to_writer(out, &serde_json::json!({"texts":texts,"groups":groups}))
        .map_err(|e| e.to_string())?;
    Ok(())
}
