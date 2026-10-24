//! Native, read-only compatibility for Python SleufBase KickTheMap jobs_cache.
//! Never reads credentials/cookies. The snapshot may be stale and is not an API check.

use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path,PathBuf};

#[derive(Clone,Debug)]
pub struct Job {
    pub job_id:u64,
    pub title:String,
    pub address:String,
    pub municipality:String,
    pub coordinates:String,
    pub client_date:String,
    pub status:i32,
    pub downloadable:bool,
    pub prefix:String,
}

#[derive(Clone,Debug)]
pub struct JobsCache {
    pub email:String,
    pub saved_at:f64,
    pub jobs:Vec<Job>,
    pub skipped:usize,
    pub source_path:PathBuf,
}

fn text(value:Option<&Value>)->String {
    match value {
        Some(Value::String(s))=>s.trim().into(),
        Some(Value::Number(n))=>n.to_string(),
        _=>String::new()
    }
}
fn positive_id(value:Option<&Value>)->Option<u64> {
    match value? {
        Value::Number(n)=>n.as_u64().filter(|n|*n>0),
        Value::String(s)=>s.trim().parse::<u64>().ok().filter(|n|*n>0),
        _=>None
    }
}
fn integer(value:Option<&Value>)->i32 {
    match value {
        Some(Value::Number(n))=>n.as_i64().and_then(|x|i32::try_from(x).ok()).unwrap_or(0),
        Some(Value::String(s))=>s.parse::<i32>().unwrap_or(0),
        _=>0
    }
}
fn boolean(value:Option<&Value>)->bool {
    match value {
        Some(Value::Bool(v))=>*v,
        Some(Value::Number(v))=>v.as_u64().is_some_and(|v|v>0),
        Some(Value::String(v))=>matches!(v.trim().to_ascii_lowercase().as_str(),"true"|"1"|"yes"),
        _=>false
    }
}

pub fn parse_cache(body:&str,path:&Path)->Result<JobsCache,String> {
    let value:Value=serde_json::from_str(body)
        .map_err(|e|format!("KickTheMap jobcache JSON ongeldig: {e}"))?;
    if value.get("version").and_then(Value::as_u64)!=Some(1) {
        return Err("Onbekende KickTheMap jobcacheversie".into());
    }
    let Some(raw_jobs)=value.get("jobs").and_then(Value::as_array) else {
        return Err("Cache bevat geen jobs-lijst".into());
    };
    let mut skipped=0;
    let mut jobs=Vec::new();
    let mut seen=HashSet::new();
    for item in raw_jobs {
        let Some(id)=positive_id(item.get("job_id")) else {skipped+=1;continue;};
        if !seen.insert(id) {skipped+=1;continue;}
        let title=text(item.get("title"));
        jobs.push(Job {
            job_id:id,
            title:if title.is_empty(){format!("Job {id}")}else{title},
            address:text(item.get("address")),
            municipality:text(item.get("municipality")),
            coordinates:text(item.get("coordinates")),
            client_date:text(item.get("client_date")),
            status:integer(item.get("status")),
            downloadable:boolean(item.get("download_available")),
            prefix:text(item.get("prefix"))
        });
    }
    if jobs.is_empty() && !raw_jobs.is_empty() {
        return Err("Alle jobs in cache zijn ongeldig".into());
    }
    Ok(JobsCache {
        email:text(value.get("email")),
        saved_at:value.get("saved_at").and_then(Value::as_f64).unwrap_or(0.0),
        jobs,skipped,source_path:path.to_owned()
    })
}

/// Save a safe account-specific snapshot; never includes passwords, tokens
/// or download URLs. This is only an offline fallback, not proof of freshness.
pub fn write_live_cache(path:&Path,email:&str,jobs:&[Job])->Result<(),String>{
    use std::io::Write;
    if jobs.len()>100_000 {return Err("Te veel jobs voor offline opslag".into());}
    let now=std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_secs_f64();
    let data=serde_json::json!({
        "version":1,"saved_at":now,"email":email.trim(),
        "jobs":jobs.iter().map(|j|serde_json::json!({
            "job_id":j.job_id,"title":j.title,"address":j.address,
            "municipality":j.municipality,"coordinates":j.coordinates,
            "client_date":j.client_date,"status":j.status,
            "download_available":j.downloadable,"prefix":j.prefix,
        })).collect::<Vec<_>>()
    });
    crate::storage::atomic_write(path,true,|f|{
        serde_json::to_writer(&mut *f,&data).map_err(|e|e.to_string())?;
        f.write_all(b"\n").map_err(|e|e.to_string())
    })
}
pub fn read_cache(path:&Path)->Result<JobsCache,String> {
    let length=fs::metadata(path).map_err(|e|e.to_string())?.len();
    if length>24*1024*1024 {return Err("KickTheMap jobcache is te groot".into());}
    let content=fs::read_to_string(path).map_err(|e|e.to_string())?;
    parse_cache(&content,path)
}
pub fn cache_path(download_root:&Path,email:&str)->PathBuf {
    let mut name=email.trim().to_ascii_lowercase()
        .chars().map(|c|if c.is_ascii_alphanumeric()||"._-".contains(c){c}else{'_'})
        .collect::<String>();
    name=name.trim_matches(|c|c=='.'||c=='_'||c=='-').into();
    if name.is_empty(){name="default".into();}
    download_root.join("jobs_cache").join(format!("{name}.json"))
}
pub fn filter_jobs<'a>(jobs:&'a [Job],term:&str,active_ids:Option<&HashSet<u64>>)->Vec<&'a Job>{
    let needle=term.trim().to_lowercase();
    jobs.iter().filter(|job| {
        active_ids.is_none_or(|ids|ids.contains(&job.job_id)) &&
        (needle.is_empty()||job.title.to_lowercase().contains(&needle)||
         job.address.to_lowercase().contains(&needle)||
         job.job_id.to_string().contains(&needle))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_jobs_deduplicated_and_filtered() {
        let data=r#"{"version":1,"saved_at":100.0,"email":"abc@test.nl","jobs":[
            {"job_id":42,"title":"PS 42","address":"Made","status":9,"download_available":true},
            {"job_id":42,"title":"Duplicate"},
            {"job_id":0},
            {"job_id":"2","title":"Tweede"}
        ]}"#;
        let cache=parse_cache(data,Path::new("test.json")).unwrap();
        assert_eq!(cache.jobs.len(),2);
        assert_eq!(cache.skipped,2);
        assert_eq!(cache.jobs[0].status,9);
        assert!(cache.jobs[0].downloadable);
        assert_eq!(filter_jobs(&cache.jobs,"made",None).len(),1);
    }
    #[test]
    fn live_snapshot_is_token_free_and_restorable() {
        let dir=tempfile::tempdir().unwrap();
        let file=dir.path().join("jobs.json");
        let j=Job {job_id:42,title:"Proefsleuf".into(),address:"Breda".into(),
            municipality:"Breda".into(),coordinates:"150000,400000".into(),
            client_date:"2026-10-20".into(),status:9,downloadable:true,prefix:"s3_dir".into()};
        write_live_cache(&file,"team@example.com",&[j.clone()]).unwrap();
        let recovered=read_cache(&file).unwrap();
        assert_eq!(recovered.jobs.len(),1);
        assert_eq!(recovered.jobs[0].prefix,j.prefix);
        assert_eq!(recovered.email,"team@example.com");
        write_live_cache(&file,"team@example.com",&[j]).unwrap();
        assert_eq!(read_cache(&file).unwrap().jobs.len(),1);
        let contents=std::fs::read_to_string(&file).unwrap();
        assert!(!contents.contains("password"));
        assert!(!contents.contains("cookie"));
    }
    #[test]
    fn file_name_sanitization() {
        assert_eq!(cache_path(Path::new("C:/data"),"Luka+abc@Test.NL"),
            PathBuf::from("C:/data/jobs_cache/luka_abc_test.nl.json"));
        assert!(parse_cache("{}",Path::new("x")).is_err());
    }
}
