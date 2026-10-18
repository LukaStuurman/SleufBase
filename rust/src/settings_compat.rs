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
    #[serde(default="default_label_gap")]
    pub dxf_label_gap:f64,
    #[serde(default="crate::rule_settings::defaults")]
    pub object_rules:Vec<crate::disciplines::Rule>,
    pub template_auto_fill_bgt_fysiek_voorkomen:bool,
    pub template_auto_export_discipline_excel:bool,
    pub kickthemap_material_choices:Vec<String>,
    pub kickthemap_profile_extra_choices:Vec<String>,
    pub marxact_name_mappings:BTreeMap<String,String>,
}
impl Default for MigratedSettings {
    fn default()->Self {
        Self {
            dxf_label_gap:default_label_gap(),
            object_rules:crate::rule_settings::defaults(),
            template_auto_fill_bgt_fysiek_voorkomen:true,
            template_auto_export_discipline_excel:true,
            kickthemap_material_choices:Vec::new(),
            kickthemap_profile_extra_choices:Vec::new(),
            marxact_name_mappings:BTreeMap::new()
        }
    }
}
pub fn default_label_gap()->f64 {3.5}
fn label_gap(value:Option<&Value>)->f64 {
    crate::python_values::number(value).filter(|v|!v.is_nan()).map_or_else(default_label_gap,|v|v.clamp(0.,100.))
}
impl MigratedSettings {
    pub fn object_dxf_options(&self)->crate::object_dxf::Options {
        crate::object_dxf::Options{label_gap:self.dxf_label_gap,..Default::default()}
    }
    fn validate(&self)->Result<(),String> {
        crate::rule_settings::validate(&self.object_rules)?;
        if !self.dxf_label_gap.is_finite() || !(0.0..=100.).contains(&self.dxf_label_gap) {
            return Err("DXF-labelafstand moet tussen 0 en 100 meter liggen".into());
        }
        Ok(())
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
        dxf_label_gap:label_gap(obj.get("dxf_label_gap")),
        object_rules:crate::rule_settings::from_legacy(obj.get("kickthemap_object_layer_rules"))?,
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
    let body=crate::input::read(path,2*1024*1024,"Instellingenbestand",&crate::input::NEVER_CANCEL)?;
    from_legacy_json(std::str::from_utf8(&body).map_err(|e|e.to_string())?)
}
pub fn load_rust_settings(path:&Path)->MigratedSettings {
    read_rust_settings(path).unwrap_or_default()
}
pub fn read_rust_settings(path:&Path)->Result<MigratedSettings,String> {
    if !path.exists() { return Ok(MigratedSettings::default()); }
    let body=crate::input::read(path,2*1024*1024,"Instellingenbestand",&crate::input::NEVER_CANCEL)?;
    let settings: MigratedSettings=serde_json::from_slice(&body).map_err(|e|format!("Rust-instellingen ongeldig: {e}"))?;
    settings.validate()?;
    Ok(settings)
}
pub fn save_rust_settings(path:&Path,config:&MigratedSettings)->Result<(),String> {
    config.validate()?;
    let body=serde_json::to_vec_pretty(config).map_err(|e|e.to_string())?;
    if body.len()+1>2*1024*1024 {return Err("Instellingenbestand is groter dan 2 MiB".into());}
    let dir=path.parent().filter(|p|!p.as_os_str().is_empty())
        .unwrap_or_else(||Path::new("."));
    fs::create_dir_all(dir).map_err(|e|e.to_string())?;
    let mut temp=NamedTempFile::new_in(dir).map_err(|e|e.to_string())?;
    temp.as_file_mut().write_all(&body).map_err(|e|e.to_string())?;
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
