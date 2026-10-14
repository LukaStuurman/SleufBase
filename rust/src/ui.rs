use eframe::egui::{self, Color32, Pos2, Stroke};
use sleufbase_rust::{load_job_features, write_dxf, write_xlsx, Bounds, Dataset, Geometry, Vertex, RULES};
use std::path::PathBuf;

const ORANGE: Color32 = Color32::from_rgb(249, 115, 22);
const DARK: Color32 = Color32::from_rgb(194, 65, 12);
const INK: Color32 = Color32::from_rgb(17, 24, 39);

#[derive(Default)]
pub struct SleufBaseApp {
    datasets: Vec<Dataset>,
    selected: Option<usize>,
    query: String,
    message: String,
}

impl SleufBaseApp {
    fn load_files(&mut self, paths: Vec<PathBuf>) {
        let mut added = 0;
        let mut failures = Vec::new();
        for path in paths {
            if self.datasets.iter().any(|ds| ds.source_path == path) { continue; }
            match load_job_features(&path) {
                Ok(dataset) => {
                    self.datasets.push(dataset);
                    self.selected = Some(self.datasets.len() - 1);
                    added += 1;
                }
                Err(err) => failures.push(format!("{}: {err}", path.display())),
            }
        }
        self.message = if failures.is_empty() {
            format!("{added} job(s) geladen uit lokale JSON-bestanden")
        } else {
            format!("{added} geladen, {} fouten: {}", failures.len(), failures.join("; "))
        };
    }

    fn save_excel(&mut self) {
        if self.datasets.is_empty() { self.message = "Laad eerst een jobFeatures.json".into(); return; }
        let path = rfd::FileDialog::new()
            .add_filter("Excel", &["xlsx"])
            .set_file_name("SleufBase-Disciplines-Rust.xlsx")
            .save_file();
        if let Some(path) = path {
            self.message = match write_xlsx(&path, &self.datasets, RULES) {
                Ok(()) => format!("Excel opgeslagen: {}", path.display()),
                Err(e) => format!("Excel-export mislukt: {e}"),
            };
        }
    }

    fn save_dxf(&mut self) {
        if self.datasets.is_empty() { self.message = "Laad eerst een jobFeatures.json".into(); return; }
        let path = rfd::FileDialog::new()
            .add_filter("DXF", &["dxf"])
            .set_file_name("SleufBase-Objecten-Rust.dxf")
            .save_file();
        if let Some(path) = path {
            self.message = match write_dxf(&path, &self.datasets, RULES) {
                Ok(()) => format!("DXF opgeslagen: {}", path.display()),
                Err(e) => format!("DXF-export mislukt: {e}"),
            };
        }
    }
}

fn canvas(ui: &mut egui::Ui, datasets: &[Dataset]) {
    let desired = ui.available_size();
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 5.0, Color32::from_rgb(250, 251, 253));
    let Some(bounds) = Bounds::from_datasets(datasets) else {
        painter.text(rect.center(), egui::Align2::CENTER_CENTER,
            "Open een jobFeatures.json om objecten te bekijken",
            egui::FontId::proportional(17.0), INK);
        return;
    };
    let pad = 28.0f32;
    let world_w = (bounds.max_x - bounds.min_x).max(1.0);
    let world_h = (bounds.max_y - bounds.min_y).max(1.0);
    let screen_w = (rect.width() - pad * 2.0).max(1.0) as f64;
    let screen_h = (rect.height() - pad * 2.0).max(1.0) as f64;
    let scale = (screen_w / world_w).min(screen_h / world_h);
    let offset_x = rect.center().x as f64 - (bounds.min_x + bounds.max_x) * 0.5 * scale;
    let offset_y = rect.center().y as f64 + (bounds.min_y + bounds.max_y) * 0.5 * scale;
    let project = |v: &Vertex| -> Pos2 {
        Pos2::new((offset_x + v.x * scale) as f32, (offset_y - v.y * scale) as f32)
    };
    for dataset in datasets {
        for feature in &dataset.features {
            let color = match sleufbase_rust::matching_rule(&feature.source_name, RULES) {
                Some(rule) if rule.label == "Waterleiding" => Color32::from_rgb(40, 105, 235),
                Some(rule) if rule.label == "Laagspanning" => Color32::from_rgb(236, 174, 20),
                Some(rule) if rule.label == "Middenspanning" => Color32::from_rgb(235, 40, 45),
                Some(_) => ORANGE,
                None => Color32::GRAY,
            };
            match &feature.geometry {
                Geometry::Point(p) => {
                    let pos = project(p);
                    painter.circle_filled(pos, 3.5, color);
                }
                Geometry::Polyline(vertices) => {
                    for pair in vertices.windows(2) {
                        painter.line_segment([project(&pair[0]), project(&pair[1])],
                            Stroke::new(1.8, color));
                    }
                }
            }
        }
    }
    painter.text(rect.left_bottom() + egui::vec2(12.0, -12.0),
        egui::Align2::LEFT_BOTTOM,
        format!("X: {:.2} - {:.2}   Y: {:.2} - {:.2} (broncoordinaten)",
            bounds.min_x, bounds.max_x, bounds.min_y, bounds.max_y),
        egui::FontId::monospace(11.0), Color32::DARK_GRAY);
}

impl eframe::App for SleufBaseApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::light());
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.colored_label(DARK, egui::RichText::new("TECHBASE").strong().size(19.0));
                ui.separator();
                ui.label(egui::RichText::new("SleufBase Rust").strong().size(18.0));
                ui.label(egui::RichText::new("MVP / lokale data").color(Color32::GRAY));
                ui.separator();
                if ui.button("Open jobFeatures.json").clicked() {
                    if let Some(files) = rfd::FileDialog::new()
                        .add_filter("KickTheMap JSON", &["json"])
                        .pick_files() { self.load_files(files); }
                }
                if ui.button("Excel disciplines").clicked() { self.save_excel(); }
                if ui.button("DXF objecten").clicked() { self.save_dxf(); }
            });
            ui.add_space(6.0);
            if !self.message.is_empty() {
                ui.label(egui::RichText::new(&self.message).small().color(DARK));
            }
        });

        egui::SidePanel::left("jobs").default_width(300.0).min_width(230.0).show(ctx, |ui| {
            ui.heading("Ingeladen proefsleuven");
            ui.label("Alleen lokaal geladen jobs worden getoond.");
            ui.add_space(6.0);
            ui.text_edit_singleline(&mut self.query);
            ui.label(egui::RichText::new("Zoek op naam of job-ID").small().color(Color32::GRAY));
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (index, dataset) in self.datasets.iter().enumerate() {
                    let search = self.query.trim().to_lowercase();
                    if !search.is_empty()
                        && !dataset.title.to_lowercase().contains(&search)
                        && !dataset.job_id.to_string().contains(&search) { continue; }
                    let label = format!("{}  ({} objecten)", dataset.title, dataset.features.len());
                    if ui.selectable_label(self.selected == Some(index), label).clicked() {
                        self.selected = Some(index);
                    }
                }
            });
            ui.separator();
            if let Some(index) = self.selected {
                if let Some(ds) = self.datasets.get(index) {
                    ui.strong(&ds.title);
                    ui.label(format!("Job ID: {}", if ds.job_id == 0 { "onbekend".into() } else { ds.job_id.to_string() }));
                    ui.label(format!("{} objecten", ds.features.len()));
                    let summary = sleufbase_rust::summarize(ds, RULES);
                    ui.label(format!("{} disciplines, {} kabels/leidingen",
                        summary.distinct_count(), summary.total_count()));
                    for (rule, count) in RULES.iter().zip(summary.counts.iter()) {
                        if *count > 0 { ui.label(format!("{}: {}", rule.label, count)); }
                    }
                }
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Kaartvoorbeeld van lokale objectgegevens");
            ui.label("Schematische weergave zonder ondergrond. RD-/GPS-projectie wordt nog niet geconverteerd.");
            let selected: Vec<Dataset> = self.selected.and_then(|i| self.datasets.get(i))
                .into_iter().cloned().collect();
            canvas(ui, &selected);
        });
    }
}