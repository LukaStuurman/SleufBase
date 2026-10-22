mod ui;
mod rule_editor;
mod export_dialog;
mod template_dialog;

use sleufbase_rust::{
    load_job_features, write_xlsx,
    marxact, project::{NativeProject,save_project_new},
    settings_compat,
};
use std::path::PathBuf;

fn print_help() {
    println!("SleufBase Rust 0.1.0 — lokale KickTheMap-objecten");
    println!("sleufbase-rust.exe                               Open Windows desktopapp");
    println!("sleufbase-rust.exe --xlsx uitvoer.xlsx jobFeatures.json [extra.json ...]");
    println!("sleufbase-rust.exe --dxf uitvoer.dxf jobFeatures.json [extra.json ...]");
    println!("sleufbase-rust.exe --template-dxf uitvoer.dxf voorbereide-export.json");
    println!("sleufbase-rust.exe --marxact uitvoer.json meting.dxf [extra.dxf ...]");
    println!("sleufbase-rust.exe --marxact-dxf uitvoer.dxf meting.dxf [extra.dxf ...]");
    println!("sleufbase-rust.exe --import-python uitvoer.json project.klicproject");
    println!("Optioneel vóór de opdracht: --settings Rust-instellingen.json");
    println!("Licenties van overgenomen onderdelen: --licenses");
    println!("Bestaande uitvoerbestanden worden niet overschreven.");
    println!("De volledige Python-sjabloonexport en interactieve KickTheMap-WebView2 zijn nog niet gemigreerd.");
}

fn run_cli(args: &[String]) -> Result<(), String> {
    let (settings_path,args)=if args.first().is_some_and(|s|s=="--settings") {
        if args.len()<5 {return Err("Gebruik --settings instellingen.json --dxf/--xlsx uitvoer invoer".into());}
        (PathBuf::from(&args[1]),&args[2..])
    } else {(sleufbase_rust::project::app_data_directory().join("settings.json"),args)};
    if args.len() < 3 {
        return Err("Gebruik --xlsx/--dxf met jobFeatures.json of --marxact/--marxact-dxf met MarXact-DXF".into());
    }
    let output = PathBuf::from(&args[1]);
    if args[0]=="--template-dxf" {
        if args.len()!=3 { return Err("Gebruik --template-dxf uitvoer.dxf voorbereide-export.json".into()); }
        if output.exists() { return Err("Uitvoerbestand bestaat al; overschrijven is niet toegestaan".into()); }
        let control=sleufbase_rust::export_control::Control::default();
        let request=sleufbase_rust::template_export::read_request(&PathBuf::from(&args[2]),&control)?;
        let exported=sleufbase_rust::template_export::export_with_progress(&output,&request,&control,|stage|println!("{stage}"))?;
        println!("{} vakken en {} profielen opgeslagen naar {}",exported.filled_slots,exported.profiles,exported.published.drawing.display());
        if let Some(path)=exported.companion {println!("Discipline-Excel opgeslagen naar {}",path.display());}
        for warning in exported.warnings {eprintln!("Melding: {warning}");}
        return Ok(());
    }
    if args[0]=="--import-python" {
        if args.len()!=3 {return Err("Gebruik --import-python uitvoer.json project.klicproject".into());}
        let project=sleufbase_rust::legacy_project::read_legacy_project(&PathBuf::from(&args[2]))?;
        save_project_new(&output,&project)?;
        println!("Python-project geïmporteerd naar {}",output.display());
        for notice in &project.legacy_archive.as_ref().unwrap().notices {println!("Melding: {notice}");}
        return Ok(());
    }
    if matches!(args[0].as_str(),"--marxact"|"--marxact-dxf") {
        if output.exists(){
            return Err("Uitvoerbestand bestaat al; overschrijven is niet toegestaan".into());
        }
        let settings=settings_compat::read_rust_settings(&settings_path)?;
        let mut trenches=Vec::new();
        for source in &args[2..] {
            let result=marxact::import_dxf(
                &PathBuf::from(source),&settings.marxact_name_mappings)?;
            trenches.extend(result.trenches);
        }
        if trenches.is_empty(){return Err("Geen MarXact-proefsleuven gevonden".into());}
        if args[0]=="--marxact" {
            let project=NativeProject{
                name:"MarXact import".into(),
                virtual_trenches:trenches,
                ..NativeProject::default()
            };
            save_project_new(&output,&project)?;
        } else {
            sleufbase_rust::object_dxf::write_with_layers(&output,&[],&trenches,None,&[],&settings.object_rules,settings.object_dxf_options())?;
        }
        println!("MarXact-conversie opgeslagen naar {}",output.display());
        return Ok(());
    }
    let mut datasets = Vec::new();
    let settings=settings_compat::read_rust_settings(&settings_path)?;
    for input in &args[2..] {
        datasets.push(load_job_features(&PathBuf::from(input))?);
    }
    match args[0].as_str() {
        "--xlsx" => write_xlsx(&output, &datasets, &settings.object_rules)?,
        "--dxf" => sleufbase_rust::object_dxf::write_with_layers(&output,&datasets,&[],None,&[],&settings.object_rules,settings.object_dxf_options())?,
        _ => return Err(format!("Onbekende opdracht: {}", args[0]))
    }
    println!("{} job(s) geëxporteerd naar {}", datasets.len(), output.display());
    Ok(())
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        if args[0]=="--licenses" {println!("Pillow 11.3.0 — affine rasterrotatie, bilineaire kaartresampling en alphacompositie\n{}\nezdxf 1.4.4 — SHP/SHX/LFF-protocol, fontmetingen en vector-tekstuitlijning (native Rust-port met correcties)\n{}\nproj4rs 0.2.1 — native GPS/RD-navigatie\n{}",include_str!("../third_party/Pillow-LICENSE.txt"),include_str!("../third_party/ezdxf-LICENSE.txt"),include_str!("../third_party/proj4rs-NOTICES.txt"));return Ok(());}
        if ["-h", "--help", "help"].contains(&args[0].as_str()) {
            print_help();
            return Ok(());
        }
        if let Err(error) = run_cli(&args) {
            eprintln!("Fout: {error}");
            print_help();
            std::process::exit(2);
        }
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1380.0, 800.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native("Techbase SleufBase Rust", options,
        Box::new(|_cc| Ok(Box::<ui::SleufBaseApp>::default())))
}
