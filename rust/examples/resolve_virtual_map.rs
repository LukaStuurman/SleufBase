//! Explicit original virtual-map cases through the production native core.
use serde::Deserialize;
use serde_json::json;
use sleufbase_rust::{virtual_map as map, virtual_trench::VirtualTrench};
#[path = "support/atomic_json.rs"]
mod atomic_json;
#[derive(Deserialize)]
struct Creation {
    start: [f64; 2],
    too_close: [f64; 2],
    end: [f64; 2],
}
#[derive(Deserialize)]
struct Request {
    source: VirtualTrench,
    reference_world: [f64; 2],
    anchor: [f64; 2],
    target: [f64; 2],
    picks: Vec<[f64; 2]>,
    mpp: f64,
    selection_tolerance: f64,
    creation: Creation,
    names: Vec<String>,
}
fn main() -> Result<(), String> {
    atomic_json::run(256, |r: Request, out| {
        if r.picks.len() > 1000 || r.source.objects.len() > 100000 {
            return Err("Te grote virtuele kaartaudit".into());
        }
        let mut t = r.source;
        t.validate()?;
        let layers = [t.clone(), t.clone()];
        let hits = r
            .picks
            .iter()
            .map(|&p| map::pick(&layers, p, r.mpp, r.selection_tolerance))
            .collect::<Vec<_>>();
        drop(layers);
        map::set_reference(&mut t, r.reference_world)?;
        let raw = t.metadata["template_reference_point"].clone();
        let display = map::reference(&t)?;
        map::translate(&mut t, r.target[0] - r.anchor[0], r.target[1] - r.anchor[1])?;
        let close_error = map::create(
            "Created".into(),
            r.creation.start,
            r.creation.too_close,
            0.6,
        )
        .is_err();
        let created = map::create("Created".into(), r.creation.start, r.creation.end, 0.6)?;
        serde_json::to_writer(out,&json!({"next_name":map::next_name(r.names),"expected_picks":hits,"reference_raw":raw,"reference_display":display,"expected":t,"close_error":close_error,"created":[[created.start.x,created.start.y],[created.end.x,created.end.y]]})).map_err(|e|e.to_string())
    })
}
