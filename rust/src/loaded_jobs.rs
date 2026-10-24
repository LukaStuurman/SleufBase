//! KickTheMap active-loaded-jobs manifest compatibility.
//! Matches the heartbeat contract in Python app.py and jobs browser:
 //! version=1, updated_at <= 6 seconds, only positive job IDs.

use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const ACTIVE_STATE_MAX_AGE_SECONDS: f64 = 6.0;
pub fn active_rust_job_ids(
    datasets:&[crate::model::Dataset],
    rasters:&[crate::geotiff::GeoTiffInfo]
)->std::collections::BTreeSet<u64>{
    let mut ids=datasets.iter().map(|d|d.job_id)
        .filter(|id|*id>0).collect::<std::collections::BTreeSet<_>>();
    ids.extend(rasters.iter().filter_map(|r|r.kickthemap_job_id)
        .filter(|id|*id>0));
    ids
}


pub fn kickthemap_cache_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("SleufBase").join("KickTheMap")
}

/// Deliberately fail closed for bad/stale/future timestamps, other versions,
/// invalid IDs and absent files. It is a read-only compatibility bridge.
pub fn parse_active_loaded_job_ids(content: &str, now_unix: f64) -> BTreeSet<u64> {
    let mut result = BTreeSet::new();
    let Ok(data) = serde_json::from_str::<Value>(content) else { return result };
    if data.get("version").and_then(Value::as_u64) != Some(1) {
        return result;
    }
    let Some(updated_at) = data.get("updated_at").and_then(Value::as_f64) else {
        return result;
    };
    let age = now_unix - updated_at;
    if !now_unix.is_finite() || !updated_at.is_finite() ||
        updated_at <= 0.0 || age < 0.0 || age > ACTIVE_STATE_MAX_AGE_SECONDS {
        return result;
    }
    let Some(values) = data.get("job_ids").and_then(Value::as_array) else {
        return result;
    };
    for value in values {
        let parsed = value.as_u64().or_else(|| {
            value.as_str().and_then(|s| s.trim().parse::<u64>().ok())
        });
        if let Some(id) = parsed.filter(|id| *id > 0) {
            result.insert(id);
        }
    }
    result
}

pub fn read_active_loaded_job_ids(path: &Path, now_unix: f64) -> BTreeSet<u64> {
    // Prevent unexpectedly huge/untrusted manifests being loaded into memory.
    if fs::metadata(path).map(|m| m.len() > 1_048_576).unwrap_or(true) {
        return BTreeSet::new();
    }
    fs::read_to_string(path)
        .map(|body| parse_active_loaded_job_ids(&body, now_unix))
        .unwrap_or_default()
}

pub fn only_active<'a, T>(
    items: impl IntoIterator<Item = &'a T>,
    active: &BTreeSet<u64>,
    job_id: impl Fn(&T) -> u64
) -> Vec<&'a T> {
    items.into_iter().filter(|job| active.contains(&job_id(job))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_loaded_jobs_only_current_data_not_cache_or_other_sessions() {
        use crate::model::{Dataset,Bounds};
        use crate::geo::GeoTransform;
        use crate::geotiff::GeoTiffInfo;
        use std::path::PathBuf;
        let dataset=Dataset{
            job_id:42,title:"PS42".into(),source_path:PathBuf::new(),features:vec![]
        };
        let raster=GeoTiffInfo{
            path:PathBuf::new(),width:1,height:1,
            transform:GeoTransform{a:1.,b:0.,c:0.,d:0.,e:-1.,f:0.},
            bounds:Bounds{min_x:0.,min_y:-1.,max_x:1.,max_y:0.},
            epsg:Some(28992),kickthemap_job_id:Some(99),
            kickthemap_job_title:"PS99".into(),kickthemap_job_sidecar:None,metadata:Default::default()
        };
        assert_eq!(active_rust_job_ids(&[dataset],&[raster])
            .into_iter().collect::<Vec<_>>(),vec![42,99]);
        assert!(active_rust_job_ids(&[],&[]).is_empty());
    }
    #[test]
    fn fresh_and_expired_behaviour() {
        let body = r#"{"version":1,"updated_at":100.0,"job_ids":[12,"34",0,-1,"bad",12]}"#;
        assert_eq!(parse_active_loaded_job_ids(body, 104.0).into_iter().collect::<Vec<_>>(), vec![12,34]);
        assert!(parse_active_loaded_job_ids(body, 107.0).is_empty());
        assert!(parse_active_loaded_job_ids(body, 99.0).is_empty());
        assert!(parse_active_loaded_job_ids("{}", 104.0).is_empty());
    }
    #[test]
    fn filter_is_intersection_not_any_historically_downloaded_file() {
        let ids = parse_active_loaded_job_ids(
            r#"{"version":1,"updated_at":100.0,"job_ids":[2,3]}"#, 101.0
        );
        let known = [1u64,2u64,999u64];
        assert_eq!(only_active(known.iter(), &ids, |i| *i), vec![&2]);
    }
}