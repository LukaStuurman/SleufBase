//! Explicit original point-mode requests through the production native functions.
use serde::Deserialize;
use serde_json::{json, Value};
use sleufbase_rust::{geo::GeoTransform, geotiff::GeoTiffInfo, raster_points as points, Dataset};
#[path = "support/atomic_json.rs"]
mod atomic_json;
#[derive(Deserialize)]
struct Pick {
    world: [f64; 2],
}
#[derive(Deserialize)]
struct PointRequest {
    metadata: Value,
    transform: GeoTransform,
    dataset: Dataset,
    roads: Vec<Vec<[f64; 2]>>,
    terrain: Vec<Vec<[f64; 2]>>,
    meters_per_pixel: f64,
    selection_tolerance: f64,
    picks: Vec<Pick>,
    reference_click: [f64; 2],
    start_click: [f64; 2],
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Request {
    Points(Box<PointRequest>),
    ContextPages {
        context_pages: sleufbase_rust::profile_context::Kind,
        bounds: sleufbase_rust::Bounds,
        pages: Vec<Value>,
    },
    Context {
        context: sleufbase_rust::profile_context::Kind,
        bounds: sleufbase_rust::Bounds,
    },
}
fn main() -> Result<(), String> {
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let mut context_client = None;
    atomic_json::run(2048, |r: Request, out| {
        let result = (|| {
            let r = match r {
                Request::Points(r) => *r,
                Request::ContextPages {
                    context_pages,
                    bounds,
                    pages,
                } => {
                    let mut pages = pages.into_iter();
                    let mut requests = 0usize;
                    let paths = sleufbase_rust::profile_context::fetch_with(
                        context_pages,
                        bounds,
                        &cancel,
                        |_| {
                            requests += 1;
                            pages
                                .next()
                                .ok_or("Ontbrekende contextreferentiepagina".into())
                        },
                    )?;
                    return Ok(json!({"paths":&*paths,"requests":requests}));
                }
                Request::Context { context, bounds } => {
                    if context_client.is_none() {
                        context_client = Some(sleufbase_rust::profile_context::Client::new()?);
                    }
                    let client = context_client.as_mut().unwrap();
                    let paths = client.fetch(context, bounds, &cancel)?;
                    let repeat = client.fetch(context, bounds, &cancel)?;
                    return Ok(
                        json!({"paths":&*paths,"shared_cache":std::sync::Arc::ptr_eq(&paths,&repeat)}),
                    );
                }
            };
            let mut raster = GeoTiffInfo {
                path: "explicit.tiff".into(),
                width: 20,
                height: 20,
                transform: r.transform,
                bounds: r.transform.bounds(20, 20)?,
                epsg: Some(28992),
                kickthemap_job_id: Some(r.dataset.job_id),
                kickthemap_job_title: String::new(),
                kickthemap_job_sidecar: None,
                metadata: r.metadata,
            };
            points::validate_metadata(&raster.metadata)?;
            let automatic = points::automatic(
                &r.dataset,
                sleufbase_rust::RULES,
                &raster.metadata,
                &r.roads,
                &r.terrain,
                &cancel,
            )?;
            let rows = points::rows(
                &raster,
                &r.dataset,
                sleufbase_rust::RULES,
                &r.roads,
                &r.terrain,
                &cancel,
            )?;
            let picks = r
                .picks
                .into_iter()
                .map(|p| points::pick(&rows, p.world, r.meters_per_pixel, r.selection_tolerance))
                .collect::<Vec<_>>();
            points::set(&mut raster, points::REFERENCE, r.reference_click, true)?;
            let after_reference = raster.metadata.clone();
            let reference_display = points::display(
                &raster,
                points::stored(&raster, points::REFERENCE).ok_or("Geen opgeslagen referentie")?,
            )?;
            points::set(&mut raster, points::START, r.start_click, false)?;
            Ok::<_, String>(
                json!({"automatic":automatic,"rows":rows,"picks":picks,"after_reference":after_reference,"after_start":raster.metadata,"reference_display":reference_display}),
            )
        })();
        serde_json::to_writer(out, &result.unwrap_or_else(|e| json!({"error":e})))
            .map_err(|e| e.to_string())
    })
}
