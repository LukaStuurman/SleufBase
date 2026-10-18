//! Native discipline report, independently compared with the original workbook.
use crate::{
    disciplines::{rule_groups, summarize_grouped_cancel, Rule},
    Dataset,
};
use rust_xlsxwriter::{Color, Format, FormatAlign, Workbook};
use std::path::Path;

pub fn write<'a>(
    path: &Path,
    datasets: impl Iterator<Item = &'a Dataset>,
    rules: &[Rule],
) -> Result<(), String> {
    write_inner(path, datasets, rules, None)
}
pub(crate) fn write_controlled<'a>(
    path: &Path,
    datasets: impl Iterator<Item = &'a Dataset>,
    rules: &[Rule],
    control: &crate::export_control::Control,
) -> Result<(), String> {
    write_inner(path, datasets, rules, Some(control))
}
fn write_inner<'a>(
    path: &Path,
    datasets: impl Iterator<Item = &'a Dataset>,
    rules: &[Rule],
    control: Option<&crate::export_control::Control>,
) -> Result<(), String> {
    if let Some(c) = control {
        c.check()?;
        c.set_stage(crate::export_control::Stage::Planning);
    }
    let cancel = control.map_or(&crate::input::NEVER_CANCEL, |c| c.flag());
    let mut datasets = datasets.peekable();
    if datasets.peek().is_none() {
        return Err("Geen ingeladen jobs".into());
    }
    if path.exists() {
        return Err(format!(
            "Bestand bestaat al: {}. Kies een nieuwe naam.",
            path.display()
        ));
    }
    let groups = rule_groups(rules);
    let columns: Vec<_> = groups
        .iter()
        .enumerate()
        .filter_map(|(i, &g)| (i == g).then_some(i))
        .collect();
    if columns.len() > 16_381 {
        return Err("Te veel disciplines voor Excel".into());
    }
    let headings: Vec<_> = std::iter::once("Proefsleuf")
        .chain([
            "Aantal verschillende disciplines",
            "Totaal aantal kabels/leidingen",
        ])
        .chain(columns.iter().map(|&i| rules[i].display_label()))
        .collect();
    let header = Format::new()
        .set_bold()
        .set_font_color(Color::White)
        .set_background_color(Color::RGB(0xC2410C))
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter);
    let body = Format::new().set_background_color(Color::RGB(0xFFF7ED));
    let total_format = body.clone().set_bold();
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("Disciplines").map_err(|e| e.to_string())?;
    sheet.set_freeze_panes(1, 0).map_err(|e| e.to_string())?;
    sheet.set_row_height(0, 24.).map_err(|e| e.to_string())?;
    for (col, name) in headings.iter().enumerate() {
        let width = match col {
            0 => 24,
            1 => 31,
            2 => 29,
            _ => (name.chars().count() + 3).clamp(12, 28),
        };
        // The original XML stores raw character widths. Pixel conversion avoids
        // adding Excel's five-pixel padding a second time.
        sheet
            .set_column_width_pixels(col as u16, (width * 7) as u16)
            .map_err(|e| e.to_string())?;
        sheet
            .write_string_with_format(0, col as u16, *name, &header)
            .map_err(|e| e.to_string())?;
    }
    let mut totals = vec![0u64; columns.len()];
    let mut row = 1u32;
    for dataset in datasets {
        if row >= 1_048_575 {
            return Err("Te veel proefsleuven voor Excel".into());
        }
        let summary = summarize_grouped_cancel(dataset, rules, &groups, cancel)?;
        sheet
            .write_string_with_format(row, 0, &summary.proefsleuf, &body)
            .map_err(|e| e.to_string())?;
        sheet
            .write_number_with_format(row, 1, summary.distinct_count() as f64, &body)
            .map_err(|e| e.to_string())?;
        sheet
            .write_number_with_format(row, 2, summary.total_count() as f64, &body)
            .map_err(|e| e.to_string())?;
        for (col, &index) in columns.iter().enumerate() {
            let count = summary.counts[index];
            totals[col] += u64::from(count);
            sheet
                .write_number_with_format(row, (col + 3) as u16, f64::from(count), &body)
                .map_err(|e| e.to_string())?;
        }
        row += 1;
    }
    sheet
        .autofilter(0, 0, row - 1, (headings.len() - 1) as u16)
        .map_err(|e| e.to_string())?;
    sheet.set_row_height(row, 22.).map_err(|e| e.to_string())?;
    sheet
        .write_string_with_format(row, 0, "Totaal", &total_format)
        .map_err(|e| e.to_string())?;
    sheet
        .write_number_with_format(
            row,
            1,
            totals.iter().filter(|&&v| v > 0).count() as f64,
            &total_format,
        )
        .map_err(|e| e.to_string())?;
    sheet
        .write_number_with_format(row, 2, totals.iter().sum::<u64>() as f64, &total_format)
        .map_err(|e| e.to_string())?;
    for (col, &total) in totals.iter().enumerate() {
        sheet
            .write_number_with_format(row, (col + 3) as u16, total as f64, &total_format)
            .map_err(|e| e.to_string())?;
    }
    if let Some(control) = control {
        control.check()?;
        control.set_stage(crate::export_control::Stage::Writing);
        crate::storage::atomic_write_checked(
            path,
            false,
            |file| {
                workbook
                    .save_to_writer(crate::export_control::CheckedWriter { out: file, control })
                    .map_err(|e| e.to_string())
            },
            || control.begin_publish(),
        )
    } else {
        crate::storage::atomic_write(path, false, |file| {
            workbook.save_to_writer(file).map_err(|e| e.to_string())
        })
    }
}
