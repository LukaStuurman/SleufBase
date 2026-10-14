//! Safe, allowlisted import of legacy SleufBase settings into Rust-only settings.
//! KickTheMap passwords, cookies, tokens and encrypted secrets are never read
//! into this structure nor copied to Rust configuration files.

use serde::{Deserialize,Serialize};
use serde_json::Value;
use std::collections::{BTreeMap,HashSet};
use std::fs;
use std::path::Path;
use std::io::Write;
use tempfile::NamedTempFile;

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
pub struct MigratedSettings {
    pub template_auto_fill_bgt_fysiek_voorkomen:bool,
    pub template_auto_export_discipline_excel:bool,
    pub kickthemap_material_choices:Vec<String>,
    pub kickthemap_profile_extra_choices:Vec<String>,
    pub marxact_name_mappings:BTreeMap<String,String>,
}
impl Default for MigratedSettings {
    fn default()->Self {
        Self {
            template_auto_fill_bgt_fysiek_voorkomen:true,
            template_auto_export_discipline_excel:true,
            kickthemap_material_choices:Vec::new(),
            kickthemap_profile_extra_choices:Vec::new(),
            marxact_name_mappings:BTreeMap::new()
        }
    }
}
fn boolean(value:Option<&Value>,default:bool)->bool {
    match value {
        Some(Value::Bool(b))=>*b,
        Some(Value::Number(n))=>n.as_i64().is_some_and(|v|v!=0),
        Some(Value::String(s))=>match s.trim().to_ascii_lowercase().as_str(){
            "0"|"false"|"nee"|"no"|"off"|"uit"=>false,
            "1"|"true"|"ja"|"yes"|"on"|"aan"=>true,
            _=>default
        }
        _=>default
    }
}
fn normalize_list(value:Option<&Value>)->Vec<String> {
    let mut seen=HashSet::new();
    let mut out=Vec::new();
    if let Some(items)=value.and_then(Value::as_array) {
        for item in items {
            let Some(value)=item.as_str() else{continue};
            let value=value.trim();
            if !value.is_empty() && seen.insert(value.to_lowercase()) {
                out.push(value.to_owned());
            }
        }
    }
    out
}
fn normalize_mappings(value:Option<&Value>)->BTreeMap<String,String> {
    let mut map=BTreeMap::new();
    let mut seen=HashSet::new();
    if let Some(raw)=value.and_then(Value::as_object) {
        for (name,value) in raw {
            let Some(target)=value.as_str() else{continue};
            let key=name.split_whitespace().collect::<Vec<_>>().join(" ");
            let target=target.trim();
            if !key.is_empty() && !target.is_empty() && seen.insert(key.to_lowercase()) {
                map.insert(key,target.to_owned());
            }
        }
    }
    map
}
pub fn from_legacy_json(content:&str)->Result<MigratedSettings,String> {
    let value:Value=serde_json::from_str(content)
        .map_err(|e|format!("Instellingen JSON ongeldig: {e}"))?;
    let obj=value.as_object().ok_or("Instellingen moeten een JSON-object zijn")?;
    Ok(MigratedSettings{
        template_auto_fill_bgt_fysiek_voorkomen:boolean(
            obj.get("template_auto_fill_bgt_fysiek_voorkomen"),true),
        template_auto_export_discipline_excel:boolean(
            obj.get("template_auto_export_discipline_excel"),true),
        kickthemap_material_choices:normalize_list(obj.get("kickthemap_material_choices")),
        kickthemap_profile_extra_choices:normalize_list(obj.get("kickthemap_profile_extra_choices")),
        marxact_name_mappings:normalize_mappings(obj.get("marxact_name_mappings"))
    })
}
pub fn import_legacy_file(path:&Path)->Result<MigratedSettings,String> {
    if fs::metadata(path).map_err(|e|e.to_string())?.len()>2*1024*1024 {
        return Err("Instellingenbestand is onverwacht groot".into());
    }
    let body=fs::read_to_string(path).map_err(|e|e.to_string())?;
    from_legacy_json(&body)
}
pub fn load_rust_settings(path:&Path)->MigratedSettings {
    fs::read_to_string(path).ok()
        .and_then(|v|serde_json::from_str::<MigratedSettings>(&v).ok())
        .unwrap_or_default()
}
pub fn save_rust_settings(path:&Path,config:&MigratedSettings)->Result<(),String> {
    let dir=path.parent().filter(|p|!p.as_os_str().is_empty())
        .unwrap_or_else(||Path::new("."));
    fs::create_dir_all(dir).map_err(|e|e.to_string())?;
    let mut temp=NamedTempFile::new_in(dir).map_err(|e|e.to_string())?;
    serde_json::to_writer_pretty(temp.as_file_mut(),config).map_err(|e|e.to_string())?;
    temp.as_file_mut().write_all(b"\n").map_err(|e|e.to_string())?;
    temp.as_file_mut().sync_all().map_err(|e|e.to_string())?;
    temp.persist(path).map_err(|e|e.error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migrations_do_not_carry_secrets() {
        let source=r#"{
            "template_auto_export_discipline_excel":"off",
            "template_auto_fill_bgt_fysiek_voorkomen":"ja",
            "kickthemap_material_choices":[" Water ","water","LS"],
            "marxact_name_mappings":{"Water":"Waterleiding"},
            "kickthemap_saved_accounts":[{"email":"secret@localhost","password":"SECRET"}],
            "token":"DO_NOT_COPY"
        }"#;
        let result=from_legacy_json(source).unwrap();
        assert!(!result.template_auto_export_discipline_excel);
        assert!(result.template_auto_fill_bgt_fysiek_voorkomen);
        assert_eq!(result.kickthemap_material_choices,vec!["Water","LS"]);
        let json=serde_json::to_string(&result).unwrap();
        assert!(!json.contains("SECRET")&&!json.contains("DO_NOT_COPY"));
        assert!(!json.contains("secret@localhost"));
    }
    #[test]
    fn invalid_settings_fall_back_or_error() {
        assert!(from_legacy_json("[]").is_err());
        assert!(from_legacy_json("invalid").is_err());
        assert_eq!(MigratedSettings::default().kickthemap_profile_extra_choices.len(),0);
    }
}
