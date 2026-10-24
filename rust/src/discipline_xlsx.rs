//! Native discipline report, independently compared with the original workbook.
use crate::{
    disciplines::{counts_grouped_cancel, rule_groups, Rule},
    Dataset,
};
use rust_xlsxwriter::{Color, Format, FormatAlign, Workbook};
use std::path::Path;
use std::{
    borrow::Cow,
    collections::{hash_map::Entry, HashMap},
};

/// A report keeps the template's visible names and ordering, even when a slot
/// has no measurement dataset. Only small summary counts are allocated.
pub struct ReportRow<'a> {
    pub label: Option<&'a str>,
    pub dataset: Option<&'a Dataset>,
}
pub fn write_rows<'a>(
    path: &Path,
    rows: impl Iterator<Item = ReportRow<'a>>,
    rules: &[Rule],
) -> Result<(), String> {
    write_rows_inner(path, rows, rules, false, None)
}
pub(crate) fn replace_rows<'a>(
    path: &Path,
    rows: impl Iterator<Item = ReportRow<'a>>,
    rules: &[Rule],
) -> Result<(), String> {
    write_rows_inner(path, rows, rules, true, None)
}

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
    write_rows_inner(
        path,
        datasets.map(|dataset| ReportRow {
            label: None,
            dataset: Some(dataset),
        }),
        rules,
        false,
        control,
    )
}
fn write_rows_inner<'a>(
    path: &Path,
    rows: impl Iterator<Item = ReportRow<'a>>,
    rules: &[Rule],
    overwrite: bool,
    control: Option<&crate::export_control::Control>,
) -> Result<(), String> {
    if let Some(c) = control {
        c.check()?;
        c.set_stage(crate::export_control::Stage::Planning);
    }
    let cancel = control.map_or(&crate::input::NEVER_CANCEL, |c| c.flag());
    let mut rows = rows.peekable();
    if rows.peek().is_none() {
        return Err("Geen ingeladen jobs".into());
    }
    if path.exists() && !overwrite {
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
    // Repeated PS variants share the immutable source, not a copied dataset or
    // another complete classification pass. Bound retained count buffers.
    let mut cache = HashMap::<*const Dataset, Vec<u32>>::new();
    let mut cache_bytes = 0usize;
    const CACHE_BYTES: usize = 2 * 1024 * 1024;
    let mut row = 1u32;
    for report in rows {
        crate::input::check(cancel)?;
        if row >= 1_048_575 {
            return Err("Te veel proefsleuven voor Excel".into());
        }
        let can_cache = cache.len() < 4096;
        let counts: Cow<'_, [u32]> = if let Some(dataset) = report.dataset {
            match cache.entry(dataset as *const Dataset) {
                Entry::Occupied(e) => Cow::Borrowed(e.into_mut()),
                Entry::Vacant(e) => {
                    let counts = counts_grouped_cancel(dataset, rules, &groups, cancel)?;
                    let bytes = std::mem::size_of_val(counts.as_slice());
                    if can_cache && cache_bytes + bytes <= CACHE_BYTES {
                        cache_bytes += bytes;
                        Cow::Borrowed(e.insert(counts))
                    } else {
                        Cow::Owned(counts)
                    }
                }
            }
        } else {
            Cow::Owned(vec![0; rules.len()])
        };
        let label = report.label.unwrap_or_else(|| {
            report
                .dataset
                .map_or("Onbekend", |d| crate::python_values::trim(&d.title))
        });
        sheet
            .write_string_with_format(row, 0, label, &body)
            .map_err(|e| e.to_string())?;
        sheet
            .write_number_with_format(
                row,
                1,
                counts.iter().filter(|&&n| n > 0).count() as f64,
                &body,
            )
            .map_err(|e| e.to_string())?;
        sheet
            .write_number_with_format(
                row,
                2,
                counts.iter().map(|&n| u64::from(n)).sum::<u64>() as f64,
                &body,
            )
            .map_err(|e| e.to_string())?;
        for (col, &index) in columns.iter().enumerate() {
            let count = counts[index];
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
            overwrite,
            |file| {
                workbook
                    .save_to_writer(crate::export_control::CheckedWriter { out: file, control })
                    .map_err(|e| e.to_string())
            },
            || control.begin_publish(),
        )
    } else {
        crate::storage::atomic_write(path, overwrite, |file| {
            workbook.save_to_writer(file).map_err(|e| e.to_string())
        })
    }
}
