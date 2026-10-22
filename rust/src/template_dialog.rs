//! Draft edits commit only on Exporteren. Stable source paths and project
//! generation protect a dialog from reordered jobs or a replaced project.
use eframe::egui;
use sleufbase_rust::{
    geotiff::GeoTiffInfo,
    template_order::{self, Row, State},
    Dataset,
};
pub(crate) struct Dialog {
    pub state: State,
    pub error: String,
    selected: Vec<bool>,
    generation: u64,
    anchor: usize,
    number: String,
    variant: String,
    scale: String,
}
/// Shared persistent CAD support folders for both template export routes.
pub(crate) fn font_folders(ui: &mut egui::Ui, folders: &mut Vec<std::path::PathBuf>) {
    egui::CollapsingHeader::new("Extra CAD-lettertypen").show(ui, |ui| {
        ui.label("Voeg een map met SHX-, SHP-, LFF- of TrueType-lettertypen toe. Deze keuze wordt bij het project bewaard.");
        let mut remove = None;
        for (i, folder) in folders.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(folder.display().to_string());
                if ui.small_button("Verwijderen").clicked() { remove = Some(i); }
            });
        }
        if let Some(i) = remove { folders.remove(i); }
        if ui.add_enabled(folders.len() < 64, egui::Button::new("Lettertypemap toevoegen…")).clicked() {
            if let Some(folder) = rfd::FileDialog::new().set_title("Map met CAD-lettertypen").pick_folder() {
                if !folders.contains(&folder) { folders.push(folder); }
            }
        }
    });
}
impl Dialog {
    pub fn new(mut state: State, rasters: &[GeoTiffInfo], generation: u64) -> Self {
        let error = state.sync(rasters).err().unwrap_or_default();
        let mut selected = vec![false; state.rows.len()];
        if let Some(s) = selected.first_mut() {
            *s = true;
        }
        let mut d = Self {
            state,
            error,
            selected,
            generation,
            anchor: 0,
            number: String::new(),
            variant: String::new(),
            scale: String::new(),
        };
        d.sync_controls();
        d
    }
    fn sync_controls(&mut self) {
        let rows = self
            .state
            .rows
            .iter()
            .zip(&self.selected)
            .filter(|(r, s)| **s && r.raster.is_some())
            .map(|(r, _)| r)
            .collect::<Vec<_>>();
        fn common(values: impl Iterator<Item = String>) -> String {
            let mut values = values;
            let Some(first) = values.next() else {
                return String::new();
            };
            if values.any(|s| s != first) {
                "Gemengd".into()
            } else {
                first
            }
        }
        self.number = common(rows.iter().map(|r| r.number.clone()));
        self.variant = common(rows.iter().map(|r| r.variant.clone()));
        self.scale = common(
            rows.iter()
                .map(|r| r.scale.map_or_else(|| "Auto".into(), |s| format!("1:{s}"))),
        );
    }
    fn select(&mut self, index: usize, modifiers: egui::Modifiers) {
        if modifiers.shift {
            if !modifiers.command && !modifiers.ctrl {
                self.selected.fill(false);
            }
            for i in self.anchor.min(index)..=self.anchor.max(index) {
                if let Some(s) = self.selected.get_mut(i) {
                    *s = true;
                }
            }
        } else {
            if !modifiers.command && !modifiers.ctrl {
                self.selected.fill(false);
            }
            self.selected[index] = !self.selected[index];
            self.anchor = index;
        }
        self.sync_controls();
    }
    fn apply_label(&mut self) -> Result<(), String> {
        let n = if self.number.to_lowercase() == "gemengd" {
            None
        } else {
            Some(
                template_order::number(&self.number)
                    .ok_or("Vul een positief of nul PS-nummer in, bijvoorbeeld 5 of PS5")?,
            )
        };
        let v = if self.variant.to_lowercase() == "gemengd" {
            None
        } else {
            Some(template_order::variant(&serde_json::Value::String(
                self.variant.clone(),
            )))
        };
        for (r, s) in self.state.rows.iter_mut().zip(&self.selected) {
            if *s && r.raster.is_some() && r.included {
                if let Some(n) = &n {
                    r.number = n.clone();
                }
                if let Some(v) = &v {
                    r.variant = v.clone();
                }
            }
        }
        self.sync_controls();
        Ok(())
    }
    fn apply_scale(&mut self) -> Result<(), String> {
        let s = self.scale.trim();
        if s.eq_ignore_ascii_case("gemengd") {
            return Ok(());
        }
        let scale = if s.is_empty() || s.eq_ignore_ascii_case("auto") {
            None
        } else {
            let value = s
                .strip_prefix("1:")
                .unwrap_or(s)
                .trim()
                .parse::<i64>()
                .map_err(|_| "Gebruik Auto of een schaal zoals 1:200")?;
            if !(1..=1_000_000).contains(&value) {
                return Err("De profielschaal moet positief zijn".into());
            }
            Some(value)
        };
        for (r, s) in self.state.rows.iter_mut().zip(&self.selected) {
            if *s && r.raster.is_some() {
                r.scale = scale;
            }
        }
        self.sync_controls();
        Ok(())
    }
}
pub(crate) fn show(
    ctx: &egui::Context,
    dialog: &mut Option<Dialog>,
    rasters: &[GeoTiffInfo],
    datasets: &[Dataset],
    generation: u64,
) -> Option<State> {
    let d = dialog.as_mut()?;
    if d.generation != generation {
        *dialog = None;
        return None;
    }
    let old = d
        .state
        .rows
        .iter()
        .map(|r| r.raster.clone())
        .collect::<Vec<_>>();
    let sync = d.state.sync(rasters);
    if let Err(e) = &sync {
        d.error = e.clone();
    }
    if old
        != d.state
            .rows
            .iter()
            .map(|r| r.raster.clone())
            .collect::<Vec<_>>()
    {
        d.selected = vec![false; d.state.rows.len()];
        for r in &mut d.state.rows {
            if r.raster.is_some() && !old.contains(&r.raster) {
                r.included = false;
            }
        }
        d.anchor = 0;
        d.sync_controls();
    }
    let labels = Some(d.state.preview_labels().0);
    let mut open = true;
    let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let mut result = None;
    let contents = |ui: &mut egui::Ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label("Projectnaam");
            ui.add(
                egui::TextEdit::singleline(&mut d.state.project)
                    .desired_width((ctx.screen_rect().width() - 300.).clamp(160., 490.)),
            );
            ui.label("Tekenaar");
            ui.add(egui::TextEdit::singleline(&mut d.state.drawn_by).desired_width(120.));
        });

        ui.label("De exportdatum wordt automatisch op ieder gebruikt blad ingevuld (dd-mm-jjjj).");
        font_folders(ui, &mut d.state.font_directories);

        ui.horizontal_wrapped(|ui| {
            ui.label(d.state.template.as_ref().map_or_else(
                || "Ingebouwd SleufBase-sjabloon".into(),
                |p| p.display().to_string(),
            ));
            if ui.button("Ander sjabloon…").clicked() {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("DXF-sjabloon", &["dxf"])
                    .pick_file()
                {
                    d.state.template = Some(p);
                }
            }
            if d.state.template.is_some() && ui.button("Ingebouwd sjabloon").clicked() {
                d.state.template = None;
            }
        });

        ui.separator();
        ui.label("Kies de volgorde en het PS-nummer. Ctrl/Shift selecteert meerdere regels; dubbele PS-nummers mogen.");

        egui::ScrollArea::vertical()
            .id_salt("template-order")
            .max_height((ctx.screen_rect().height() - 520.).clamp(80., 320.))
            .show_rows(ui, 26., d.state.rows.len(), |ui, range| {
                for i in range {
                    let r = &d.state.rows[i];
                    let name = r.raster.as_ref().map_or_else(
                        || "[Leeg vak]".into(),
                        |p| {
                            p.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned()
                        },
                    );
                    let ps =
                        labels
                            .as_ref()
                            .and_then(|l| l[i].as_deref())
                            .unwrap_or(if r.included {
                                "Ongeldig PS-nummer"
                            } else {
                                "Niet meenemen"
                            });
                    let text = if r.raster.is_none() {
                        format!("{:02}. {name}", i + 1)
                    } else {
                        format!(
                            "{:02}. [{ps}] [{}] {name}",
                            i + 1,
                            r.scale.map_or_else(|| "Auto".into(), |s| format!("1:{s}"))
                        )
                    };

                    if ui.selectable_label(d.selected[i], text).clicked() {
                        d.select(i, ui.input(|i| i.modifiers));
                    }
                }
            });

        ui.horizontal_wrapped(|ui| {
            if ui.button("Omhoog").clicked() {
                d.state.move_selected(&mut d.selected, false);
                d.sync_controls();
            }

            if ui.button("Omlaag").clicked() {
                d.state.move_selected(&mut d.selected, true);
                d.sync_controls();
            }

            for (text, included) in [("Wel meenemen", true), ("Niet meenemen", false)] {
                if ui.button(text).clicked() {
                    for (r, s) in d.state.rows.iter_mut().zip(&d.selected) {
                        if *s && r.raster.is_some() {
                            r.included = included;
                        }
                    }
                }
            }

            if ui
                .add_enabled(
                    d.state.rows.len() < 5000,
                    egui::Button::new("Leeg vak invoegen"),
                )
                .clicked()
            {
                let index = d
                    .selected
                    .iter()
                    .position(|s| *s)
                    .unwrap_or(d.state.rows.len());
                d.state.rows.insert(index, Row::default());
                d.selected = vec![false; d.state.rows.len()];
                d.selected[index] = true;
                d.anchor = index;
                d.sync_controls();
            }

            if ui.button("Lege vakken verwijderen").clicked() {
                let mut index = 0;
                d.state.rows.retain(|r| {
                    let keep = r.raster.is_some() || !d.selected[index];
                    index += 1;
                    keep
                });
                d.selected = vec![false; d.state.rows.len()];
                d.anchor = 0;
                d.sync_controls();
            }
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("PS-nummer");
            ui.add(egui::TextEdit::singleline(&mut d.number).desired_width(80.));
            ui.label("Variant");
            ui.add(egui::TextEdit::singleline(&mut d.variant).desired_width(90.));
            ui.label("Leeg, AUTO, B/C/…");
            if ui.button("Nummer/variant toepassen").clicked() {
                d.error = d.apply_label().err().unwrap_or_default();
            }
            ui.label("Profielschaal");
            ui.add(egui::TextEdit::singleline(&mut d.scale).desired_width(85.));
            if ui.button("Schaal toepassen").clicked() {
                d.error = d.apply_scale().err().unwrap_or_default();
            }
        });

        let selected = d
            .selected
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.then_some(i))
            .collect::<Vec<_>>();

        if selected.len() == 1 {
            let row = &mut d.state.rows[selected[0]];
            if row.raster.is_some() {
                ui.horizontal_wrapped(|ui| {
                    let current = if row.without_profile {
                        "Geen profiel".into()
                    } else {
                        row.dataset
                            .as_ref()
                            .and_then(|p| datasets.iter().find(|d| d.source_path == *p))
                            .map_or_else(|| "Automatisch via jobnummer".into(), |d| d.title.clone())
                    };
                    egui::ComboBox::from_id_salt("template-profile-job")
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    !row.without_profile && row.dataset.is_none(),
                                    "Automatisch via jobnummer",
                                )
                                .clicked()
                            {
                                row.without_profile = false;
                                row.dataset = None;
                            }

                            if ui
                                .selectable_label(row.without_profile, "Geen profiel")
                                .clicked()
                            {
                                row.without_profile = true;
                                row.dataset = None;
                            }

                            for dataset in datasets {
                                if ui
                                    .selectable_label(
                                        row.dataset.as_ref() == Some(&dataset.source_path),
                                        format!(
                                            "{} — {}",
                                            dataset.title,
                                            dataset.source_path.display()
                                        ),
                                    )
                                    .clicked()
                                {
                                    row.dataset = Some(dataset.source_path.clone());
                                    row.without_profile = false;
                                }
                            }
                        });
                    ui.label("Adres");
                    let mut address = row.address.clone().unwrap_or_default();
                    if ui.text_edit_singleline(&mut address).changed() {
                        row.address = (!address.is_empty()).then_some(address);
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    ui.label("Opmerkingen");
                    ui.add(egui::TextEdit::singleline(&mut row.comments).desired_width(690.));
                });
            }
        }

        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut d.state.profiles, "Dwarsprofielen");
            ui.checkbox(&mut d.state.paired, "Beide richtingen (Normaal / Reverse)");
            ui.add_enabled(
                !d.state.paired,
                egui::Checkbox::new(&mut d.state.reverse, "Alleen omgekeerde richting"),
            );
            ui.checkbox(&mut d.state.show_reference, "Referentiepunt");
            ui.checkbox(&mut d.state.avoid_collisions, "Labels uit elkaar plaatsen");
        });

        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut d.state.aerial, "PDOK-luchtfoto op kaartbladen");
            ui.checkbox(&mut d.state.auto_bgt, "BGT-ondergrond automatisch invullen");
            ui.label("Markerdiameter (m)");
            ui.add(
                egui::DragValue::new(&mut d.state.marker_diameter)
                    .range(0.005..=1.)
                    .speed(0.005),
            );
        });

        let included = d
            .state
            .rows
            .iter()
            .filter(|r| r.raster.is_some() && r.included)
            .count();
        let empty = d.state.rows.iter().filter(|r| r.raster.is_none()).count();
        ui.label(format!(
            "Meenemen: {included} | Lege vakken: {empty} | Bladen: {}",
            (included + empty).div_ceil(8)
        ));

        if !d.error.is_empty() {
            ui.colored_label(egui::Color32::RED, &d.error);
        }

        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    sync.is_ok() && included > 0,
                    egui::Button::new("Exporteren…"),
                )
                .clicked()
            {
                match d.state.confirmed() {
                    Ok(state) => result = Some(state),
                    Err(e) => d.error = e,
                }
            }

            if ui.button("Annuleren").clicked() {
                cancel = true;
            }
        });
    };
    egui::Window::new("DXF-sjabloon exporteren")
        .open(&mut open)
        .default_width(900.)
        .max_width((ctx.screen_rect().width() - 48.).max(280.))
        .max_height((ctx.screen_rect().height() - 48.).max(200.))
        .vscroll(true)
        .resizable(true)
        .show(ctx, contents);

    if !open || cancel {
        *dialog = None;
        return None;
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    fn raster() -> GeoTiffInfo {
        let transform = sleufbase_rust::geo::GeoTransform {
            a: 1.,
            b: 0.,
            c: 0.,
            d: 0.,
            e: -1.,
            f: 10.,
        };
        GeoTiffInfo {
            path: "PS01B.tiff".into(),
            width: 10,
            height: 10,
            transform,
            bounds: transform.bounds(10, 10).unwrap(),
            epsg: Some(28992),
            kickthemap_job_id: None,
            kickthemap_job_title: String::new(),
            kickthemap_job_sidecar: None,metadata:Default::default(),
        }
    }
    fn frame(
        ctx: &egui::Context,
        d: &mut Option<Dialog>,
        rasters: &[GeoTiffInfo],
        events: Vec<egui::Event>,
    ) -> (Option<State>, egui::FullOutput) {
        let mut result = None;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1024., 768.),
                )),
                events,
                ..Default::default()
            },
            |ctx| result = show(ctx, d, rasters, &[], 7),
        );
        (result, output)
    }
    #[test]
    fn actual_window_exports_project_value_and_escape_discards_draft_edits() {
        let rasters = vec![raster()];
        let original = State {
            project: "Bestaand Ø".into(),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut d = Some(Dialog::new(original.clone(), &rasters, 7));
        d.as_mut().unwrap().state.project = "  Nieuw project Ø  ".into();
        frame(&ctx, &mut d, &rasters, vec![]);
        let (_, output) = frame(&ctx, &mut d, &rasters, vec![]);
        assert!(output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.job.text.contains("automatisch op ieder gebruikt blad"))));
        let pos = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == "Exporteren…" => {
                    Some(t.pos + t.galley.rect.size() / 2.)
                }
                _ => None,
            })
            .expect("visible export button");
        assert!(pos.y < 768.);
        let (exported, _) = frame(
            &ctx,
            &mut d,
            &rasters,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ],
        );
        assert_eq!(exported.unwrap().project, "Nieuw project Ø");
        let (cancelled, _) = frame(
            &ctx,
            &mut d,
            &rasters,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert!(cancelled.is_none());
        assert!(d.is_none());
        assert_eq!(original.project, "Bestaand Ø");
    }
    #[test]
    fn project_replacement_closes_and_appended_sources_start_excluded() {
        let mut rasters = vec![raster()];
        let ctx = egui::Context::default();
        let mut d = Some(Dialog::new(State::default(), &rasters, 7));
        let mut other = raster();
        other.path = "PS2.tiff".into();
        rasters.push(other);
        frame(&ctx, &mut d, &rasters, vec![]);
        assert!(!d.as_ref().unwrap().state.rows[1].included);
        assert!(show(&ctx, &mut d, &rasters, &[], 8).is_none());
        assert!(d.is_none());
    }
}
