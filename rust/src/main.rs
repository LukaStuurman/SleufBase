mod ui;

use sleufbase_rust::{load_job_features, write_dxf, write_xlsx, RULES};
use std::path::PathBuf;

fn print_help() {
    println!("SleufBase Rust 0.1.0 — lokale KickTheMap-objecten");
    println!("sleufbase-rust.exe                               Open Windows desktopapp");
    println!("sleufbase-rust.exe --xlsx uitvoer.xlsx jobFeatures.json [extra.json ...]");
    println!("sleufbase-rust.exe --dxf uitvoer.dxf jobFeatures.json [extra.json ...]");
    println!("Bestaande uitvoerbestanden worden niet overschreven.");
    println!("De volledige Python-sjabloonexport en live KickTheMap-login ontbreken nog.");
}

fn run_cli(args: &[String]) -> Result<(), String> {
    if args.len() < 3 {
        return Err("Gebruik --xlsx of --dxf, een nieuw uitvoerbestand en minimaal één jobFeatures.json".into());
    }
    let output = PathBuf::from(&args[1]);
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