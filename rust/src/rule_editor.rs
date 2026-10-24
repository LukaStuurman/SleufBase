use eframe::egui;
use sleufbase_rust::{disciplines::Rule, rule_settings};

#[derive(Clone)]
struct Row {
    words: String,
    layer: String,
    color: u16,
    label: String,
}
pub(crate) struct Editor {
    rows: Vec<Row>,
    pub error: String,
}
impl Editor {
    pub fn new(rules: &[Rule]) -> Self {
        Self {
            rows: rules
                .iter()
                .map(|r| Row {
                    words: r
                        .keywords
                        .iter()
                        .map(|s| s.as_ref())
                        .collect::<Vec<_>>()
                        .join(","),
                    layer: r.target_layer.to_string(),
                    color: r.color_aci,
                    label: r.label.to_string(),
                })
                .collect(),
            error: String::new(),
        }
    }
    fn rules(&self) -> Result<Vec<Rule>, String> {
        let rules = self
            .rows
            .iter()
            .map(|r| {
                Rule::new(
                    r.words
                        .split([',', ';', '\n'])
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect(),
                    r.layer.trim().into(),
                    r.color,
                    r.label.trim().into(),
                )
            })
            .collect::<Vec<_>>();
        rule_settings::validate(&rules)?;
        Ok(rules)
    }
}
/// Return a candidate only; the caller publishes it after durable storage.
pub(crate) fn show(ctx: &egui::Context, editor: &mut Option<Editor>) -> Option<Vec<Rule>> {
    let state = editor.as_mut()?;
    let mut open = true;
    let mut candidate = None;
    let mut cancel = false;
    egui::Window::new("Objectregels en disciplinekleuren")
        .open(&mut open)
        .default_width(720.)
        .show(ctx, |ui| {
            ui.label("De eerste passende regel bepaalt de CAD-laag, kleur en discipline.");
            ui.label("Scheid coderingen met komma of puntkomma. * en ? ondersteunen patronen.");
            let mut action = None;
            egui::ScrollArea::vertical()
                .max_height(470.)
                .show(ui, |ui| {
                    let count = state.rows.len();
                    for (i, row) in state.rows.iter_mut().enumerate() {
                        ui.push_id(i, |ui| {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.strong(format!("Regel {}", i + 1));
                                    if ui.add_enabled(i > 0, egui::Button::new("Omhoog")).clicked()
                                    {
                                        action = Some((i, -1));
                                    }
                                    if ui
                                        .add_enabled(i + 1 < count, egui::Button::new("Omlaag"))
                                        .clicked()
                                    {
                                        action = Some((i, 1));
                                    }
                                    if ui.button("Verwijderen").clicked() {
                                        action = Some((i, 0));
                                    }
                                });
                                egui::Grid::new("fields").num_columns(2).show(ui, |ui| {
                                    ui.label("Coderingen");
                                    ui.text_edit_singleline(&mut row.words);
                                    ui.end_row();
                                    ui.label("CAD-laag");
                                    ui.text_edit_singleline(&mut row.layer);
                                    ui.end_row();
                                    ui.label("Disciplinenaam");
                                    ui.text_edit_singleline(&mut row.label);
                                    ui.end_row();
                                    ui.label("ACI-kleur");
                                    ui.add(egui::DragValue::new(&mut row.color).range(1..=256))
                                        .on_hover_text("1–255: CAD-kleur; 256: kleur van de laag");
                                    ui.end_row();
                                });
                            });
                        });
                    }
                });
            if let Some((index, direction)) = action {
                if direction == 0 {
                    state.rows.remove(index);
                } else {
                    state
                        .rows
                        .swap(index, (index as isize + direction) as usize);
                }
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(state.rows.len() < 256, egui::Button::new("Regel toevoegen"))
                    .clicked()
                {
                    state.rows.push(Row {
                        words: String::new(),
                        layer: "0".into(),
                        color: 256,
                        label: String::new(),
                    });
                }
                if ui.button("Standaardregels herstellen").clicked() {
                    *state = Editor::new(&rule_settings::defaults());
                }
            });
            if !state.error.is_empty() {
                ui.colored_label(egui::Color32::RED, &state.error);
            }
            ui.horizontal(|ui| {
                if ui.button("Opslaan en toepassen").clicked() {
                    match state.rules() {
                        Ok(rules) => candidate = Some(rules),
                        Err(error) => state.error = error,
                    }
                }
                if ui.button("Annuleren").clicked() {
                    cancel = true;
                }
            });
        });
    if !open || cancel {
        *editor = None;
        return None;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edits_are_isolated_until_a_valid_candidate_is_saved() {
        let original = rule_settings::defaults();
        let mut draft = Editor::new(&original);
        draft.rows[0].words = "data; *WAT?R*".into();
        draft.rows.swap(0, 1);
        let rules = draft.rules().unwrap();
        assert_eq!(rules[1].keywords[1], "*WAT?R*");
        assert_eq!(original[0].keywords[0], "water");
        draft.rows[0].layer = "bad\nlayer".into();
        assert!(draft.rules().is_err());
    }
}
