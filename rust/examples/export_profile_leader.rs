//! Raw donor-promotion audit driver; geometry/prototype installation is separate.
use sleufbase_rust::{dxf_document::Document, profile_leader_donor, storage};
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let input = PathBuf::from(args.next().ok_or("input DXF required")?);
    let output = PathBuf::from(args.next().ok_or("fresh output DXF required")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let cancel = AtomicBool::new(false);
    let mut doc = Document::load(&input, &cancel)?;
    let changed = profile_leader_donor::promote(&mut doc, &cancel)?;
    if profile_leader_donor::promote(&mut doc, &cancel)? {
        return Err("promotion is not idempotent".into());
    }
    storage::atomic_write(&output, false, |out| doc.write_to(out, &cancel))?;
    println!("dynamic donor added: {changed}");
    Ok(())
}
