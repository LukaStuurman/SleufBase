//! End-user GUI for building template exports directly from project sources.
use super::*;
use sleufbase_rust::template_project::{self, BackgroundKind, Options, Source};

pub(super) struct Dialog {
    pub options: Options,
    pub output: Option<PathBuf>,
    pub error: String,
    pub automatic: Vec<sleufbase_rust::template_match::Suggestion>,
    jobs:Vec<(u64,PathBuf)>,
    rasters:Vec<(PathBuf,Option<u64>)>,
    trenches:Vec<(u64,String)>,
}
impl Dialog {
    fn new(app: &SleufBaseApp) -> Self {
        let mut options = Options {
            project: app.project_name.clone(),
            choices: template_project::suggest(
                &app.datasets, &app.geotiffs, &app.virtual_trenches),
            ..Options::default()
        };
        if let Some(saved)=&app.template_preferences {
            options=saved.restore(&app.datasets,&app.geotiffs,&app.virtual_trenches);
        }
        // Do not silently reuse stale project-specific settings from another
        // project. Users explicitly select the desired CAD template each time.
        if app.template_preferences.is_none() {options.include_bgt = true;}
        let automatic=options.choices.iter().map(|choice| match choice.source {
            Source::Job(i) => sleufbase_rust::template_match::for_job(&app.datasets[i], &app.geotiffs),
            Source::Virtual(i) => sleufbase_rust::template_match::for_trench(&app.virtual_trenches[i], &app.geotiffs),
        }).collect();
        Self { options, output: None, error: String::new(), automatic,
            jobs:app.datasets.iter().map(|d|(d.job_id,d.source_path.clone())).collect(),
            rasters:app.geotiffs.iter().map(|r|(r.path.clone(),r.kickthemap_job_id)).collect(),
            trenches:app.virtual_trenches.iter().map(|t|(t.job_id,t.label.clone())).collect(),
        }
    }
    fn sources_match(&self,app:&SleufBaseApp)->bool {
        self.jobs.len()==app.datasets.len() && self.rasters.len()==app.geotiffs.len()
            && self.trenches.len()==app.virtual_trenches.len()
            && self.jobs.iter().zip(&app.datasets).all(|(key,d)|key.0==d.job_id && key.1==d.source_path)
            && self.rasters.iter().zip(&app.geotiffs).all(|(key,r)|key.0==r.path && key.1==r.kickthemap_job_id)
            && self.trenches.iter().zip(&app.virtual_trenches).all(|(key,t)|key.0==t.job_id && key.1==t.label)
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
        self.template_dialog = None;
        self.template_order_dialog = None;
        self.project_template_dialog = Some(Dialog::new(self));
    }
    pub(super) fn show_project_template_dialog(&mut self, ctx: &egui::Context) {
        // Choices contain vector indices. Never let removed/reordered sources
        // silently turn a reviewed slot into a different measurement job.
        if self.project_template_dialog.as_ref().is_some_and(|d|!d.sources_match(self)) {
            self.project_template_dialog=None;
            self.message="Projectbronnen gewijzigd. Open de sjabloonexport opnieuw om de koppelingen te controleren.".into();
            return;
        }
        let mut open = true;
        let mut launch = None;
        let rasters = &self.geotiffs;
        let raster_previews = &self.raster_textures;
        if let Some(dialog) = &mut self.project_template_dialog {
            let mut reorder = None;
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
                        crate::template_dialog::font_folders(ui, &mut dialog.options.font_directories);
                        ui.checkbox(&mut dialog.options.paired,"Beide richtingen (Normaal / Reverse)");
                        if dialog.options.paired {ui.small("Kies na export in AutoCAD de gewenste richting met Versie.");}
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
                            ui.checkbox(&mut dialog.options.include_cad,
                                "Zichtbare CAD/DXF-lagen (ook geneste blokken)");
                        });
                        ui.label("Jobs gebruiken de volledige GeoTIFF. Virtuele/MarXact-sleuven kunnen zonder externe TIFF hun eigen transparante raster met meetpunten en grens gebruiken.");
                        ui.separator();
                        ui.heading("3. Proefsleufvakken en GeoTIFF-koppelingen");
                        let selected = dialog.options.choices.iter().filter(|s|s.include).count();
                        ui.label(format!("{selected} vakken geselecteerd; {} GeoTIFF's geladen",
                            rasters.len()));
                        if selected > template_project::MAX_SLOTS {
                            ui.colored_label(DARK, "Te veel proefsleufvakken");
                        }
                        ui.horizontal(|ui| {
                            if ui.small_button("Alle vakken selecteren").clicked() {
                                for c in &mut dialog.options.choices { c.include = true; }
                            }
                            if ui.small_button("Alles deselecteren").clicked() {
                                for c in &mut dialog.options.choices { c.include = false; }
                            }
                            if ui.small_button("Alleen gekoppelde TIFF's").clicked() {
                                for c in &mut dialog.options.choices { c.include = c.raster.is_some() || matches!(c.source,Source::Virtual(_)); }
                            }
                        });
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
                                        if idx>0 && ui.small_button("↑").on_hover_text("Eerder op het sjabloon").clicked() {
                                            reorder=Some((idx,idx-1));
                                        }
                                        if idx+1<dialog.automatic.len() && ui.small_button("↓").on_hover_text("Later op het sjabloon").clicked() {
                                            reorder=Some((idx,idx+1));
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("GeoTIFF:");
                                        let selected_name = slot.raster.and_then(|i| rasters.get(i))
                                            .and_then(|r|r.path.file_name())
                                            .map(|s|s.to_string_lossy().to_string())
                                            .unwrap_or_else(||match slot.source {
                                                Source::Virtual(_)=>"Eigen virtueel raster (geen TIFF nodig)".into(),
                                                Source::Job(_)=>"Ontbreekt — selecteer een TIFF".into(),
                                            });
                                        egui::ComboBox::from_id_salt("raster")
                                            .selected_text(selected_name)
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(&mut slot.raster, None,
                                                    if matches!(slot.source,Source::Virtual(_)){
                                                        "Eigen virtueel raster (zonder externe TIFF)"
                                                    }else{"Geen (niet exporteerbaar)"});
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
                                        ui.add_enabled(!dialog.options.paired,egui::Checkbox::new(&mut slot.reverse,"Alleen omgekeerde richting"));
                                    });
                                    if slot.include && slot.raster.is_none() && matches!(slot.source,Source::Job(_)) {
                                        ui.colored_label(DARK, "Koppel eerst een GeoTIFF voordat dit vak geëxporteerd kan worden.");
                                    }
                                    let suggested = dialog.automatic[idx];
                                    if let Some(automatic_idx)=suggested.raster {
                                        if slot.raster==Some(automatic_idx) {
                                            ui.small(match suggested.reason {
                                                sleufbase_rust::template_match::MatchKind::JobId =>
                                                    "Automatische TIFF-koppeling: unieke KickTheMap-job-ID",
                                                sleufbase_rust::template_match::MatchKind::Location =>
                                                    "Automatische TIFF-koppeling: unieke ligging in RD",
                                                _ => "TIFF handmatig beoordelen",
                                            });
                                        } else if ui.small_button("Herstel automatische TIFF-keuze").clicked() {
                                            slot.raster=Some(automatic_idx);
                                        }
                                    } else if suggested.reason == sleufbase_rust::template_match::MatchKind::Ambiguous {
                                        ui.small("Meerdere mogelijke TIFF's; handmatige keuze nodig.");
                                    }
                                    if let Some(raster_index)=slot.raster {
                                        if let Some(info)=rasters.get(raster_index) {
                                            ui.horizontal(|ui| {
                                                if let Some(Some(preview))=raster_previews.get(raster_index) {
                                                    ui.add(egui::Image::new((preview.id(),egui::vec2(190.,120.))));
                                                }
                                                ui.vertical(|ui| {
                                                    ui.small(format!("RD X {:.1}–{:.1}",info.bounds.min_x,info.bounds.max_x));
                                                    ui.small(format!("RD Y {:.1}–{:.1}",info.bounds.min_y,info.bounds.max_y));
                                                    ui.small(format!("CRS: {}",info.epsg.map_or("onbekend".into(),|id|format!("EPSG:{id}"))));
                                                });
                                            });
                                        }
                                    }
                                    ui.collapsing(format!("Werkfoto's voor dit vak ({}/{})",
                                        slot.photos.len(),
                                        sleufbase_rust::template_photo::MAX_PHOTOS_PER_SLOT), |ui| {
                                        ui.small("JPEG/PNG als eigen afbeeldingsbestand op de rechterkant van het gekozen kaart- of TIFF-vak. De oorspronkelijke rasterbron blijft bestaan.");
                                        if ui.add_enabled(slot.photos.len() <
                                            sleufbase_rust::template_photo::MAX_PHOTOS_PER_SLOT,
                                            egui::Button::new("Voeg werkfoto's toe…")).clicked() {
                                            if let Some(selected)=rfd::FileDialog::new()
                                                .add_filter("Foto's", &["jpg","jpeg","png"])
                                                .pick_files() {
                                                for path in selected {
                                                    if slot.photos.len()>=sleufbase_rust::template_photo::MAX_PHOTOS_PER_SLOT {break;}
                                                    if !slot.photos.iter().any(|p|p.path==path) {
                                                        slot.photos.push(sleufbase_rust::template_photo::Photo {
                                                            path,turns:0,
                                                            area:sleufbase_rust::template_photo::Area::Tiff
                                                        });
                                                    }
                                                }
                                            }
                                        }
                                        let mut remove=None;
                                        for (photo_index,photo) in slot.photos.iter_mut().enumerate() {
                                            ui.push_id(photo_index,|ui|{
                                                ui.horizontal(|ui|{
                                                    ui.label(photo.path.file_name()
                                                        .map(|s|s.to_string_lossy().to_string())
                                                        .unwrap_or_else(||"Werkfoto".into()));
                                                    egui::ComboBox::from_id_salt("rotate")
                                                        .selected_text(format!("{}°",u32::from(photo.turns)*90))
                                                        .show_ui(ui,|ui|{
                                                            for turn in 0..4 {
                                                                ui.selectable_value(&mut photo.turns,turn,format!("{}°",u32::from(turn)*90));
                                                            }
                                                        });
                                                    egui::ComboBox::from_id_salt("area")
                                                        .selected_text(match photo.area{
                                                            sleufbase_rust::template_photo::Area::Tiff=>"Op TIFF",
                                                            sleufbase_rust::template_photo::Area::Map=>"Op kaart",
                                                        }).show_ui(ui,|ui|{
                                                            ui.selectable_value(&mut photo.area,
                                                                sleufbase_rust::template_photo::Area::Tiff,"Op TIFF");
                                                            ui.selectable_value(&mut photo.area,
                                                                sleufbase_rust::template_photo::Area::Map,"Op kaart");
                                                        });
                                                    if ui.small_button("Verwijder").clicked(){remove=Some(photo_index);}
                                                });
                                                if !photo.path.is_file(){ui.colored_label(DARK,"Fotobestand ontbreekt");}
                                            });
                                        }
                                        if let Some(index)=remove {slot.photos.remove(index);}
                                    });
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
                            dialog.options.choices.iter().filter(|c|c.include)
                                .all(|c|c.raster.is_some()||matches!(c.source,Source::Virtual(_)));
                        if ui.add_enabled(ready, egui::Button::new("Volledige project-sjabloonexport starten")).clicked() {
                            launch = dialog.output.clone();
                        }
                    });
                });
            if let Some((a,b))=reorder {
                dialog.options.choices.swap(a,b);
                dialog.automatic.swap(a,b);
            }
        }
        if !open { self.project_template_dialog = None; }
        if let Some(output) = launch {
            let options = std::mem::take(&mut self.project_template_dialog.as_mut().unwrap().options);
            self.project_template_dialog = None;
            self.start_project_template_export(options, output);
        }
    }
}
