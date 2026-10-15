mod ui;

use sleufbase_rust::{
    load_job_features, write_dxf, write_xlsx, write_dxf_with_trenches, RULES,
    marxact, project::{NativeProject,save_project},
    settings_compat,
};
use std::path::PathBuf;

fn print_help() {
    println!("SleufBase Rust 0.1.0 — lokale KickTheMap-objecten");
    println!("sleufbase-rust.exe                               Open Windows desktopapp");
    println!("sleufbase-rust.exe --xlsx uitvoer.xlsx jobFeatures.json [extra.json ...]");
    println!("sleufbase-rust.exe --dxf uitvoer.dxf jobFeatures.json [extra.json ...]");
    println!("sleufbase-rust.exe --marxact uitvoer.json meting.dxf [extra.dxf ...]");
    println!("sleufbase-rust.exe --marxact-dxf uitvoer.dxf meting.dxf [extra.dxf ...]");
    println!("Bestaande uitvoerbestanden worden niet overschreven.");
    println!("De volledige Python-sjabloonexport en interactieve KickTheMap-WebView2 zijn nog niet gemigreerd.");
}

fn run_cli(args: &[String]) -> Result<(), String> {
    if args.len() < 3 {
        return Err("Gebruik --xlsx/--dxf met jobFeatures.json of --marxact/--marxact-dxf met MarXact-DXF".into());
    }
    let output = PathBuf::from(&args[1]);
    if matches!(args[0].as_str(),"--marxact"|"--marxact-dxf") {
        if output.exists(){
            return Err("Uitvoerbestand bestaat al; overschrijven is niet toegestaan".into());
        }
        let settings=settings_compat::load_rust_settings(
            &sleufbase_rust::project::app_data_directory().join("settings.json"));
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
            save_project(&output,&project)?;
        } else {
            write_dxf_with_trenches(&output,&[],&trenches,RULES)?;
        }
        println!("MarXact-conversie opgeslagen naar {}",output.display());
        return Ok(());
    }
    let mut datasets = Vec::new();
    for input in &args[2..] {
        datasets.push(load_job_features(&PathBuf::from(input))?);
    }
    match args[0].as_str() {
        "--xlsx" => write_xlsx(&output, &datasets, RULES)?,
        "--dxf" => write_dxf(&output, &datasets, RULES)?,
        _ => return Err(format!("Onbekende opdracht: {}", args[0]))
    }
    println!("{} job(s) geëxporteerd naar {}", datasets.len(), output.display());
    Ok(())
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
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