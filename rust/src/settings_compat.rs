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
#[serde(default,deny_unknown_fields)]
pub struct ProfilePreferences {
    pub marker_diameter:f64,pub show_reference:bool,pub use_custom_maaiveld:bool,
    pub auto_fill_bgt:bool,pub avoid_collisions:bool,pub clip_markers:bool,pub diameter_prefix:String,
}
impl Default for ProfilePreferences {
    fn default()->Self{Self{marker_diameter:0.02,show_reference:true,use_custom_maaiveld:true,auto_fill_bgt:true,avoid_collisions:true,clip_markers:false,diameter_prefix:"Ø".into()}}
}
impl ProfilePreferences {
    pub fn from_settings(settings:&MigratedSettings)->Self{
        let p=&settings.export_preferences;Self{marker_diameter:p.template_cross_section_marker_diameter,
            show_reference:p.template_show_profile_direction,use_custom_maaiveld:p.template_enable_maaiveld_edit_points,
            auto_fill_bgt:settings.template_auto_fill_bgt_fysiek_voorkomen,avoid_collisions:p.template_avoid_multileader_collisions,
            clip_markers:p.template_clip_cross_section_markers_to_profile,diameter_prefix:p.template_cross_section_diameter_text.clone()}
    }
    pub fn validate(&self)->Result<(),String>{if !self.marker_diameter.is_finite()||!(0.005..=1.).contains(&self.marker_diameter)||self.diameter_prefix.len()>32768{return Err("Ongeldige profielvoorkeuren".into());}Ok(())}
    pub fn apply(&self,profile:&mut crate::template_export::Profile){profile.marker_diameter=self.marker_diameter;profile.show_reference=self.show_reference;profile.use_custom_maaiveld=self.use_custom_maaiveld;profile.auto_fill_bgt=self.auto_fill_bgt;profile.avoid_collisions=self.avoid_collisions;profile.clip_markers=self.clip_markers;profile.diameter_prefix=self.diameter_prefix.clone();}
}

/// Non-secret original export preferences. Defaults are the production
/// AppSettings defaults; account, browser and password fields are excluded.
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(default,deny_unknown_fields)]
pub struct ExportPreferences {
    pub dxf_trench_export_mode:String,
    pub include_tiffs_in_dxf_export:bool,
    pub dxf_centerline_color:String,
    pub dxf_label_color:String,
    pub cadastral_template_path:String,
    pub template_client_logo_preset:String,
    pub template_client_logo_path:String,
    pub template_include_cross_sections:bool,
    pub template_generate_reverse_cross_section_sheet:bool,
    pub template_show_profile_direction:bool,
    pub template_enable_maaiveld_edit_points:bool,
    pub template_avoid_multileader_collisions:bool,
    pub template_clip_cross_section_markers_to_profile:bool,
    pub template_cross_section_marker_diameter:f64,
    pub template_cross_section_diameter_text:String,
}
impl Default for ExportPreferences {
    fn default()->Self {Self {
        dxf_trench_export_mode:"centerline".into(),include_tiffs_in_dxf_export:false,
        dxf_centerline_color:"#000000".into(),dxf_label_color:"#000000".into(),
        cadastral_template_path:String::new(),template_client_logo_preset:"bgm".into(),
        template_client_logo_path:String::new(),template_include_cross_sections:true,
        template_generate_reverse_cross_section_sheet:false,template_show_profile_direction:true,
        template_enable_maaiveld_edit_points:true,template_avoid_multileader_collisions:true,
        template_clip_cross_section_markers_to_profile:true,
        template_cross_section_marker_diameter:0.02,template_cross_section_diameter_text:String::new(),
    }}
}
impl ExportPreferences {
    fn from_legacy(value:&Value)->Self {
        let mut out=Self::default();
        macro_rules! flag {($($name:ident),*)=>{$(out.$name=legacy_boolean(value.get(stringify!($name)),out.$name);)*}}
        flag!(include_tiffs_in_dxf_export,template_include_cross_sections,
            template_generate_reverse_cross_section_sheet,template_show_profile_direction,
            template_enable_maaiveld_edit_points,template_avoid_multileader_collisions,
            template_clip_cross_section_markers_to_profile);
        macro_rules! text {($($name:ident),*)=>{$(if let Some(v)=value.get(stringify!($name)).filter(|v|!v.is_null()){let s=crate::python_values::string(Some(v)).unwrap_or_default();let s=crate::python_values::trim(&s);if s.len()<=32768{out.$name=s.to_owned();}})*}}
        text!(cadastral_template_path,template_client_logo_path,template_cross_section_diameter_text);
        if let Some(mode)=value.get("dxf_trench_export_mode").and_then(Value::as_str){let mode=crate::python_values::trim(mode).to_lowercase();if matches!(mode.as_str(),"centerline"|"polygon"|"none"){out.dxf_trench_export_mode=mode;}}
        if let Some(preset)=value.get("template_client_logo_preset").and_then(Value::as_str){let preset=crate::python_values::trim(preset).to_lowercase();if matches!(preset.as_str(),""|"brabant_water"|"enexis"|"wml"|"hurkmans"|"hanab"|"bgm"|"apk"|"custom"){out.template_client_logo_preset=preset;}}
        if value.get("template_client_logo_preset").is_some_and(Value::is_null){out.template_client_logo_preset.clear();}
        for (key,target) in [("dxf_centerline_color",&mut out.dxf_centerline_color),("dxf_label_color",&mut out.dxf_label_color)] {
            if let Some(s)=value.get(key).and_then(Value::as_str){let s=crate::python_values::trim(s).to_uppercase();if s.len()==7&&s.starts_with('#')&&s[1..].bytes().all(|b|b.is_ascii_hexdigit()){*target=s;}}
        }
        if let Some(d)=crate::python_values::number(value.get("template_cross_section_marker_diameter")).filter(|d|!d.is_nan()){out.template_cross_section_marker_diameter=d.clamp(0.005,0.2);}
        out
    }
    pub fn validate(&self)->Result<(),String>{
        if self.cadastral_template_path.len()>32768||self.template_client_logo_path.len()>32768||self.template_cross_section_diameter_text.len()>32768||self.template_client_logo_preset.len()>256
            || !self.template_cross_section_marker_diameter.is_finite()||!(0.005..=0.2).contains(&self.template_cross_section_marker_diameter)
            || !matches!(self.dxf_trench_export_mode.as_str(),"centerline"|"polygon"|"none")
            || [&self.dxf_centerline_color,&self.dxf_label_color].iter().any(|s|s.len()!=7||!s.starts_with('#')||!s[1..].bytes().all(|b|b.is_ascii_hexdigit()))
        {return Err("Ongeldige oorspronkelijke exportvoorkeuren".into());}Ok(())
    }
}

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
pub struct MigratedSettings {
    #[serde(default)]
    pub export_preferences:ExportPreferences,
    #[serde(default)]
    pub map_export:crate::map_batch::Settings,
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
            export_preferences:ExportPreferences::default(),
            map_export:Default::default(),
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
        self.export_preferences.validate()?;
        self.map_export.validate()?;
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
        Some(Value::Number(n))=>n.as_f64().is_some_and(|v|v!=0.),
        Some(Value::String(s))=>match s.trim().to_ascii_lowercase().as_str(){
            "0"|"false"|"nee"|"no"|"off"|"uit"=>false,
            "1"|"true"|"ja"|"yes"|"on"|"aan"=>true,
            _=>default
        }
        _=>default
    }
}
fn legacy_boolean(value:Option<&Value>,default:bool)->bool {
    if value.and_then(Value::as_str).is_some_and(|s|matches!(crate::python_values::trim(s).to_ascii_lowercase().as_str(),"aan"|"uit")){default}else{boolean(value,default)}
}
fn normalize_list(value:Option<&Value>)->Vec<String> {
    let mut seen=HashSet::new();
    let mut out=Vec::new();
    if let Some(items)=value.and_then(Value::as_array) {
        for item in items {
            let value=if crate::python_values::truth(item){crate::python_values::string(Some(item)).unwrap_or_default()}else{String::new()};
            let value=crate::python_values::trim(&value);
            if !value.is_empty() && seen.insert(crate::python_values::casefold(value)) {
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
            let target=if crate::python_values::truth(value){crate::python_values::string(Some(value)).unwrap_or_default()}else{String::new()};
            let key=name.split(|c:char|c.is_whitespace()||matches!(c,'\u{1c}'..='\u{1f}')).filter(|s|!s.is_empty()).collect::<Vec<_>>().join(" ");
            let target=crate::python_values::trim(&target);
            if !key.is_empty() && !target.is_empty() && seen.insert(crate::python_values::casefold(&key)) {
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
        export_preferences:ExportPreferences::from_legacy(&value),
        map_export:crate::map_batch::Settings{
            background:obj.get("map_export_background_source").and_then(Value::as_str)
                .filter(|s|matches!(*s,"pdok"|"osm"|"cadastral"))
                .unwrap_or("pdok").to_owned(),
            include_cad:boolean(obj.get("include_klic_in_map_exports"),false),
            ..Default::default()
        },
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
    fn original_nonsecret_preferences_and_normalizers(){
        let fixture:Value=serde_json::from_str(include_str!("../tests/fixtures/preferences.json")).unwrap();
        for case in fixture["cases"].as_array().unwrap(){
            let settings=from_legacy_json(&case["raw"].to_string()).unwrap();
            assert_eq!(serde_json::to_value(settings.export_preferences).unwrap(),case["expected"],"{}",case["raw"]);
        }
        for case in fixture["choices"].as_array().unwrap(){assert_eq!(serde_json::to_value(normalize_list(Some(&case["raw"]))).unwrap(),case["expected"]);}
        for case in fixture["mappings"].as_array().unwrap(){assert_eq!(serde_json::to_value(normalize_mappings(Some(&case["raw"]))).unwrap(),case["expected"]);}
    }
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
