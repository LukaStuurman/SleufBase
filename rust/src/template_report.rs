//! Post-publication companion report. Never damage the durable DXF or loaded
//! sources; atomic workbook replacement retains the prior report in its bundle.
use crate::{template_export::Request, template_transaction::Published, Dataset};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

pub(crate) fn input_paths<'a>(request: &'a Request, datasets: &'a [Dataset]) -> Vec<&'a Path> {
    let mut sources = vec![request.template.as_path()];
    sources.extend(request.settings.logos.techbase.as_deref());
    sources.extend(request.settings.logos.client.as_deref());
    sources.extend(
        request
            .settings
            .protected_sources
            .iter()
            .map(PathBuf::as_path),
    );
    sources.extend(datasets.iter().map(|d| d.source_path.as_path()));
    for slot in request.slots.iter().flatten() {
        sources.push(&slot.tiff_png);
        sources.push(&slot.map_png);
        if let Some(tiff) = &slot.tiff_source {
            sources.push(&tiff.path);
        }
        if let Some(v) = &slot.virtual_source {
            sources.extend(v.trench.source_path.as_deref());
        }
        if let Some(trench)=slot.profile.as_ref().and_then(|p|p.virtual_trench.as_ref()) {
            sources.extend(trench.source_path.as_deref());
        }
        sources.extend(slot.photos.iter().map(|p| p.path.as_path()));
        if let Some(map) = &slot.map_source {
            sources.extend(map.input_paths());
        }
    }
    sources
}
pub(crate) fn write(
    output: &Path,
    published: &Published,
    request: &Request,
    datasets: &[Dataset],
    font_paths: &[PathBuf],
) -> Result<(PathBuf, Vec<String>), String> {
    if request.rules.is_empty() {
        return Err("er zijn geen disciplines geconfigureerd".into());
    }
    let path = output.with_extension("xlsx");
    let target = fs::canonicalize(&path).ok();
    let mut sources = input_paths(request, datasets);
    sources.extend(font_paths.iter().map(PathBuf::as_path));
    sources.push(output);
    for source in sources.into_iter().filter(|p| !p.as_os_str().is_empty()) {
        if source == path
            || target
                .as_ref()
                .is_some_and(|p| fs::canonicalize(source).is_ok_and(|s| s == *p))
        {
            return Err("het Excel-pad verwijst naar de DXF of een geladen projectbron; die blijft behouden".into());
        }
    }
    let replacing = match fs::symlink_metadata(&path) {
        Ok(m) => {
            if !m.is_file() || m.file_type().is_symlink() {
                return Err("het Excel-pad is geen gewoon bestand".into());
            }
            const MAX_OLD: u64 = 128 * 1024 * 1024;
            if m.len() > MAX_OLD {
                return Err(
                    "het bestaande overzicht is te groot om veilig te bewaren (maximaal 128 MiB)"
                        .into(),
                );
            }
            let mut source = File::open(&path)
                .map_err(|e| e.to_string())?
                .take(MAX_OLD + 1);
            crate::storage::atomic_write(
                &published.assets.join("previous-disciplines.xlsx"),
                false,
                |file| {
                    let copied = std::io::copy(&mut source, file).map_err(|e| e.to_string())?;
                    if copied > MAX_OLD {
                        return Err("het bestaande overzicht groeide tijdens het bewaren".into());
                    }
                    Ok(())
                },
            )?;
            true
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(e.to_string()),
    };
    let mut warnings = Vec::new();
    let rows = request.slots.iter().flatten().map(|slot| {
        let label = slot.report_label.as_deref().unwrap_or(&slot.label);
        let index = slot
            .report_dataset
            .or_else(|| slot.profile.as_ref().map(|p| p.dataset));
        let dataset = index.and_then(|i| datasets.get(i));
        if dataset.is_none() {
            warnings.push(format!(
                "{label}: geen kabel/leidinggegevens beschikbaar; nulregel in Excel"
            ));
        }
        crate::discipline_xlsx::ReportRow {
            label: Some(label),
            dataset,
        }
    });
    if replacing {
        crate::discipline_xlsx::replace_rows(&path, rows, &request.rules)?;
    } else {
        crate::discipline_xlsx::write_rows(&path, rows, &request.rules)?;
    }
    if replacing {
        warnings.push(format!(
            "Vorige Excel-versie bewaard: {}",
            published.assets.join("previous-disciplines.xlsx").display()
        ));
    }
    Ok((path, warnings))
}
