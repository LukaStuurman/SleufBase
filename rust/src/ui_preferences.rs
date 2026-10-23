//! Native controls for the original non-secret export settings. Changes are
//! saved by the existing settings dialog's atomic save action.
use sleufbase_rust::{settings_compat::{ExportPreferences,MigratedSettings},template_branding::Logos};
use std::path::{Path,PathBuf};
use eframe::egui;

pub(super) fn fields(ui:&mut egui::Ui,preferences:&mut ExportPreferences){
    ui.collapsing("DXF- en sjabloonvoorkeuren",|ui|{
        ui.horizontal(|ui|{ui.label("Standaard DXF-sjabloon");ui.text_edit_singleline(&mut preferences.cadastral_template_path);if ui.button("Kiezen…").clicked(){if let Some(path)=rfd::FileDialog::new().add_filter("AutoCAD DXF",&["dxf"]).pick_file(){preferences.cadastral_template_path=path.to_string_lossy().into_owned();}}});
        ui.checkbox(&mut preferences.template_include_cross_sections,"Dwarsprofielen opnemen");
        ui.checkbox(&mut preferences.template_show_profile_direction,"Profielrichting en referentie tonen");
        ui.checkbox(&mut preferences.template_enable_maaiveld_edit_points,"Bewerkte maaiveldpunten gebruiken");
        ui.checkbox(&mut preferences.template_avoid_multileader_collisions,"Botsingen tussen profiellabels vermijden");
        ui.checkbox(&mut preferences.template_clip_cross_section_markers_to_profile,"Objectmarkers binnen profiel houden");
        ui.horizontal(|ui|{ui.label("Markerdiameter (m)");ui.add(egui::DragValue::new(&mut preferences.template_cross_section_marker_diameter).range(0.005..=0.2).speed(0.001));});
        ui.horizontal(|ui|{ui.label("Diametertekst (leeg = Ø)");ui.text_edit_singleline(&mut preferences.template_cross_section_diameter_text);});
        egui::ComboBox::from_label("Opdrachtgeverlogo").selected_text(preferences.template_client_logo_preset.clone()).show_ui(ui,|ui|{for (key,label) in [("","Geen"),("brabant_water","Brabant Water"),("enexis","Enexis"),("wml","WML"),("hurkmans","Hurkmans groep"),("hanab","Hanab"),("bgm","BGM"),("apk","APK"),("custom","Eigen bestand")]{ui.selectable_value(&mut preferences.template_client_logo_preset,key.into(),label);}});
        if preferences.template_client_logo_preset=="custom"{ui.horizontal(|ui|{ui.text_edit_singleline(&mut preferences.template_client_logo_path);if ui.button("Logo kiezen…").clicked(){if let Some(path)=rfd::FileDialog::new().add_filter("Afbeelding",&["png","jpg","jpeg"]).pick_file(){preferences.template_client_logo_path=path.to_string_lossy().into_owned();}}});}
    });
}
fn assets()->Option<PathBuf>{
    let mut roots=Vec::new();if let Ok(exe)=std::env::current_exe(){if let Some(parent)=exe.parent(){roots.push(parent.join("assets"));}}
    if let Ok(cwd)=std::env::current_dir(){roots.push(cwd.join("assets"));}
    roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets"));
    roots.into_iter().find(|p|p.join("template_logos").is_dir())
}
pub(super) fn logos(preferences:&ExportPreferences)->Logos{
    let directory=assets().map(|p|p.join("template_logos"));
    let techbase=directory.as_ref().map(|p|p.join("techbase_full.png")).filter(|p|p.is_file());
    let client=match preferences.template_client_logo_preset.as_str(){
        ""=>None,"custom"=>(!preferences.template_client_logo_path.is_empty()).then(||PathBuf::from(&preferences.template_client_logo_path)),
        preset=>directory.map(|p|p.join(format!("{preset}.{}",if matches!(preset,"hanab"|"wml"){ "jpg" }else{"png"}))),
    };Logos{techbase,client}
}
pub(super) fn apply_order_defaults(state:&mut sleufbase_rust::template_order::State,settings:&MigratedSettings){
    let p=&settings.export_preferences;state.template=(!p.cadastral_template_path.is_empty()).then(||p.cadastral_template_path.clone().into());
    state.profiles=p.template_include_cross_sections;
    // template_reverse_patch.py:412 always includes both variants, starting
    // normally. The archived legacy reverse-sheet flag must not flip the base.
    state.show_reference=p.template_show_profile_direction;state.avoid_collisions=p.template_avoid_multileader_collisions;state.marker_diameter=p.template_cross_section_marker_diameter;
    state.use_custom_maaiveld=p.template_enable_maaiveld_edit_points;state.clip_markers=p.template_clip_cross_section_markers_to_profile;state.diameter_prefix=p.template_cross_section_diameter_text.clone();
    state.auto_bgt=settings.template_auto_fill_bgt_fysiek_voorkomen;state.label_gap=settings.dxf_label_gap;state.logos=logos(p);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn order_dialog_defaults_include_profile_options_and_custom_logo(){
        let mut settings=MigratedSettings::default();
        let p=&mut settings.export_preferences;
        p.cadastral_template_path="chosen.dxf".into();p.template_client_logo_preset="custom".into();p.template_client_logo_path="chosen.png".into();
        p.template_include_cross_sections=false;p.template_generate_reverse_cross_section_sheet=true;
        p.template_cross_section_marker_diameter=0.06;p.template_clip_cross_section_markers_to_profile=false;
        p.template_cross_section_diameter_text="diameter".into();settings.template_auto_fill_bgt_fysiek_voorkomen=false;
        let mut state=sleufbase_rust::template_order::State::default();apply_order_defaults(&mut state,&settings);
        assert_eq!(state.template,Some(PathBuf::from("chosen.dxf")));
        assert_eq!(state.logos.client,Some(PathBuf::from("chosen.png")));
        assert!(!state.profiles);assert!(!state.reverse);assert!(state.paired);assert_eq!(state.marker_diameter,0.06);
        assert!(!state.clip_markers);assert!(!state.auto_bgt);assert_eq!(state.diameter_prefix,"diameter");
    }
}
