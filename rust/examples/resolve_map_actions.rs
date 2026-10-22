//! Explicit comment batches through production native functions, without a UI/account.
use serde::Deserialize;
use serde_json::{json, Value};
use sleufbase_rust::{
    geo::GeoTransform, geotiff::GeoTiffInfo, map_comments, project::Comment, Bounds,
};
#[path = "support/atomic_json.rs"]
mod atomic_json;
#[derive(Deserialize)]
struct Raster {
    metadata: Value,
    transform: GeoTransform,
    bounds: Bounds,
    width: u32,
    height: u32,
}
#[derive(Deserialize)]
struct Request {
    rasters: Vec<Raster>,
    comments: Vec<Comment>,
    selected: usize,
    edit: Option<String>,
}
fn main() -> Result<(), String> {
    atomic_json::run(2048, |request: Request, out| {
        let result = (|| {
            let mut rasters = request
                .rasters
                .into_iter()
                .enumerate()
                .map(|(i, r)| GeoTiffInfo {
                    path: format!("Proefsleuf {i} Ø.tif").into(),
                    width: r.width,
                    height: r.height,
                    transform: r.transform,
                    bounds: r.bounds,
                    epsg: Some(28992),
                    kickthemap_job_id: None,
                    kickthemap_job_title: String::new(),
                    kickthemap_job_sidecar: None,
                    metadata: r.metadata,
                })
                .collect::<Vec<_>>();
            if request.selected >= rasters.len() {
                return Err("Ongeldige geselecteerde TIFF".to_owned());
            }
            let mut comments = request.comments;
            let texts = rasters
                .iter()
                .map(|r| map_comments::text(&r.metadata))
                .collect::<Result<Vec<_>, _>>()?;
            let combined = map_comments::combined(&comments, &rasters)?;
            let formatted = rasters
                .iter()
                .map(|r| sleufbase_rust::template_workflow::comments(r.bounds, &combined, ""))
                .collect::<Vec<_>>();
            if let Some(text) = request.edit {
                map_comments::set(&mut rasters[request.selected], &text)?;
            }
            let metadata = |rasters: &[GeoTiffInfo]| {
                rasters
                    .iter()
                    .map(|r| r.metadata.clone())
                    .collect::<Vec<_>>()
            };
            let after_edit = metadata(&rasters);
            if !map_comments::text(&rasters[request.selected].metadata)?.is_empty() {
                map_comments::set(&mut rasters[request.selected], "")?;
            }
            let after_selected_clear = metadata(&rasters);
            let count = map_comments::clear(&mut comments, &mut rasters)?;
            Ok::<_, String>(
                json!({"texts":texts,"combined":combined,"formatted":formatted,"after_edit":after_edit,"after_selected_clear":after_selected_clear,"after_clear":metadata(&rasters),"clear_status":if count==0{"Geen opmerkingen om te wissen.".into()}else{format!("{count} opmerking(en) gewist.")}}),
            )
        })();
        serde_json::to_writer(out, &result.unwrap_or_else(|e| json!({"error":e})))
            .map_err(|e| e.to_string())
    })
}
