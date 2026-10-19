//! Native donor installation/promotion audit driver; not the full template UI.
use sleufbase_rust::{
    dxf_document::Document,
    profile_leader_donor, storage,
    template_geometry::{self, Layer},
};
use std::{path::PathBuf, sync::atomic::AtomicBool};
#[derive(serde::Deserialize)]
struct Reference {
    description: String,
    depth: String,
    start: [f64; 2],
    scale: f64,
    color: Option<u16>,
}
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let input = PathBuf::from(args.next().ok_or("input DXF required")?);
    let output = PathBuf::from(args.next().ok_or("fresh output DXF required")?);
    let install = match args.next().as_deref() {
        None => false,
        Some("--install") => true,
        _ => return Err("unexpected argument".into()),
    };
    let references = args.next().map(PathBuf::from);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let cancel = AtomicBool::new(false);
    let mut doc = Document::load(&input, &cancel)?;
    let changed = if install {
        profile_leader_donor::install(&mut doc, &cancel)?
    } else {
        profile_leader_donor::promote(&mut doc, &cancel)?
    };
    if profile_leader_donor::promote(&mut doc, &cancel)? {
        return Err("promotion is not idempotent".into());
    }
    if install && profile_leader_donor::install(&mut doc, &cancel)? {
        return Err("installation is not idempotent".into());
    }
    if let Some(path) = references {
        let requests: Vec<Reference> =
            serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let entities = requests
            .into_iter()
            .map(|r| {
                profile_leader_donor::reference(r.description, r.depth, r.start, r.scale, r.color)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let layer = Layer {
            name: "X-XX-AL-VERWIJZING-SD".into(),
            color: 7,
            true_color: None,
            lineweight: Some(0),
        };
        let split = entities.len() / 2;
        template_geometry::add(
            &mut doc,
            std::slice::from_ref(&layer),
            &entities[..split],
            &cancel,
        )?;
        template_geometry::add(&mut doc, &[layer], &entities[split..], &cancel)?;
    }
    storage::atomic_write(&output, false, |out| doc.write_to(out, &cancel))?;
    println!("dynamic donor added: {changed}");
    Ok(())
}
