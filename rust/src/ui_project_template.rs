//! End-user GUI for building template exports directly from project sources.
use super::*;
use sleufbase_rust::template_project::{self, BackgroundKind, Options, Source};

pub(super) struct Dialog {
    pub options: Options,
    pub output: Option<PathBuf>,
    pub error: String,
}
impl Dialog {
    fn new(app: &SleufBaseApp) -> Self {
        let mut options = Options {
            project: app.project_name.clone(),
            choices: template_project::suggest(
                &app.datasets, &app.geotiffs, &app.virtual_trenches),
            ..Options::default()
        };
        // Do not silently reuse stale project-specific settings from another
        // project. Users explicitly select the desired CAD template each time.
        options.include_bgt = true;
        Self { options, output: None, error: String::new() }
    }
}

impl SleufBaseApp {
    pub(super) fn open_project_template_dialog(&mut self) {
        if self.template_job.is_some() || self.export_intent.is_some() ||
           self.file_intent.is_some() || self.pending_project_action.is_some() {
            self.message = "Rond de lopende taak eerst af".into();
            return;
        }
        if self.datasets.is_empty() && self.virtual_trenches.is_empty() {
            self.message = "Laad eerst jobs of maak virtuele proefsleuven".into();
            return;
        }
        self.project_template_dialog = Some(Dialog::new(self));
    }
    pub(super) fn show_project_template_dialog(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut launch = None;
        let rasters = &self.geotiffs;
        if let Some(dialog) = &mut self.project_template_dialog {
            egui::Window::new("Project naar sjabloon-DXF — volledig native Rust")
                .open(&mut open)
                .default_width(730.)
                .default_height(650.)
                .resizable(true)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().max_height(570.).show(ui, |ui| {
                        ui.heading("1. CAD-sjabloon en uitvoer");
                        ui.horizontal(|ui| {
                            if ui.button("DXF-sjabloon kiezen…").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("AutoCAD DXF", &["dxf"]).pick_file() {
                                    dialog.options.template = path;
                                }
                            }
                            ui.label(dialog.options.template.display().to_string());
                        });
                        ui.horizontal(|ui| {
                            if ui.button("Nieuwe uitvoer-DXF kiezen…").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("AutoCAD DXF", &["dxf"])
                                    .set_file_name("SleufBase-Project-Sjabloon-Rust.dxf")
                                    .save_file() { dialog.output = Some(path); }
                            }
                            if let Some(path) = &dialog.output {
                                ui.label(path.display().to_string());
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Project:");
                            ui.text_edit_singleline(&mut dialog.options.project);
                            ui.label("Getekend door:");
                            ui.text_edit_singleline(&mut dialog.options.drawn_by);
                        });
                        ui.separator();
                        ui.heading("2. Kaarten en profielen");
                        ui.horizontal(|ui| {
                            ui.label("Resolutie (DPI)");
                            ui.add(egui::DragValue::new(&mut dialog.options.dpi).range(72..=300));
                            ui.label("Kaartschaal 1:");
                            ui.add(egui::DragValue::new(&mut dialog.options.scale).range(1.0..=100_000.).speed(1.));
                        });
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut dialog.options.background, BackgroundKind::Blank,
                                "Lokaal / geen netwerk (witte kaart)");
                            ui.radio_value(&mut dialog.options.background, BackgroundKind::Pdok,
                                "PDOK-luchtfoto (netwerk)");
                        });
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut dialog.options.include_bgt, "Zichtbare BGT-gegevens op kaart");
                            ui.checkbox(&mut dialog.options.include_cadastral,
                                "Zichtbare Kadaster-lijnen op kaart");
                        });
                        ui.label("De volledige GeoTIFF wordt op het proefsleufblad én op de kaart gebruikt. Geen previewafbeeldingen.");
                        ui.separator();
                        ui.heading("3. Proefsleufvakken en GeoTIFF-koppelingen");
                        let selected = dialog.options.choices.iter().filter(|s|s.include).count();
                        ui.label(format!("{selected} vakken geselecteerd; {} GeoTIFF's geladen",
                            rasters.len()));
                        if selected > template_project::MAX_SLOTS {
                            ui.colored_label(DARK, "Te veel proefsleufvakken");
                        }
                        for (idx, slot) in dialog.options.choices.iter_mut().enumerate() {
                            ui.push_id(idx, |ui| {
                                ui.group(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.checkbox(&mut slot.include, "");
                                        ui.strong(match slot.source {
                                            Source::Job(i) => format!("KickTheMap-job {}", i + 1),
                                            Source::Virtual(i) => format!("Virtuele sleuf {}", i + 1),
                                        });
                                        ui.text_edit_singleline(&mut slot.title);
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("GeoTIFF:");
                                        let selected_name = slot.raster.and_then(|i| rasters.get(i))
                                            .and_then(|r|r.path.file_name())
                                            .map(|s|s.to_string_lossy().to_string())
                                            .unwrap_or_else(||"Ontbreekt — selecteer een TIFF".into());
                                        egui::ComboBox::from_id_salt("raster")
                                            .selected_text(selected_name)
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(&mut slot.raster, None,
                                                    "Geen (niet exporteerbaar)");
                                                for (i, info) in rasters.iter().enumerate() {
                                                    let filename = info.path.file_name()
                                                        .map(|s|s.to_string_lossy().to_string())
                                                        .unwrap_or_else(||"GeoTIFF".into());
                                                    ui.selectable_value(&mut slot.raster, Some(i),
                                                        format!("{} ({})", filename,
                                                            info.kickthemap_job_id.map_or("?".into(),|id|id.to_string())));
                                                }
                                            });
                                        ui.checkbox(&mut slot.profile, "Profiel tekenen");
                                        ui.checkbox(&mut slot.reverse, "Richting omkeren");
                                    });
                                    if slot.include && slot.raster.is_none() {
                                        ui.colored_label(DARK, "Koppel eerst een GeoTIFF voordat dit vak geëxporteerd kan worden.");
                                    }
                                    ui.collapsing("Opmerkingen", |ui| {
                                        ui.text_edit_multiline(&mut slot.comments);
                                    });
                                });
                            });
                        }
                        ui.separator();
                        if !dialog.error.is_empty() {
                            ui.colored_label(DARK, &dialog.error);
                        }
                        ui.small("De oorspronkelijke jobs, virtuele sleuven en projectbestanden blijven behouden. Een bestaande uitvoer wordt niet overschreven.");
                        let ready = dialog.options.template.is_file() &&
                            dialog.output.as_ref().is_some_and(|p|!p.exists()) &&
                            dialog.options.choices.iter().any(|c|c.include) &&
                            dialog.options.choices.iter().filter(|c|c.include).all(|c|c.raster.is_some());
                        if ui.add_enabled(ready, egui::Button::new("Volledige project-sjabloonexport starten")).clicked() {
                            launch = dialog.output.clone();
                        }
                    });
                });
        }
        if !open { self.project_template_dialog = None; }
        if let Some(output) = launch {
            let options = std::mem::take(&mut self.project_template_dialog.as_mut().unwrap().options);
            self.project_template_dialog = None;
            self.start_project_template_export(options, output);
        }
    }
}
