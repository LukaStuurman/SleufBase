//! Lightweight selection of loaded jobs; only virtual jobs need preparation.
use crate::{virtual_trench::VirtualTrench, Dataset};
#[derive(Clone, Debug)]
pub struct Selection {
    pub datasets: Vec<bool>,
    pub trenches: Vec<bool>,
    pub include_endpoints: bool,
}
impl Selection {
    pub fn all(datasets: usize, trenches: usize) -> Self {
        Self {
            datasets: vec![true; datasets],
            trenches: vec![true; trenches],
            include_endpoints: true,
        }
    }
    pub fn prepare<'a>(
        &self,
        datasets: &'a [Dataset],
        trenches: &[VirtualTrench],
    ) -> Result<Prepared<'a>, String> {
        self.prepare_with_cancel(datasets, trenches, &crate::input::NEVER_CANCEL)
    }
    pub(crate) fn prepare_with_cancel<'a>(
        &self,
        datasets: &'a [Dataset],
        trenches: &[VirtualTrench],
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<Prepared<'a>, String> {
        crate::input::check(cancel)?;
        if self.datasets.len() != datasets.len() || self.trenches.len() != trenches.len() {
            return Err("De bronnen zijn veranderd; vernieuw de exportselectie".into());
        }
        let regular = self
            .datasets
            .iter()
            .zip(datasets)
            .filter_map(|(&selected, job)| selected.then_some(job))
            .collect();
        let mut selected: Vec<_> = self
            .trenches
            .iter()
            .zip(trenches)
            .filter_map(|(&selected, trench)| selected.then_some(trench))
            .collect();
        selected.sort_by_cached_key(|t| {
            crate::python_values::casefold(
                t.source_path
                    .as_ref()
                    .and_then(|p| p.file_stem())
                    .and_then(|s| s.to_str())
                    .unwrap_or(&t.label),
            )
        });
        let mut virtual_jobs = Vec::new();
        let mut skipped = Vec::new();
        for trench in selected {
            let job = trench.to_dataset_with_cancel(0, self.include_endpoints, cancel)?;
            if job.features.is_empty() {
                skipped.push(trench.label.clone());
            } else {
                virtual_jobs.push(job);
            }
        }
        Ok(Prepared {
            regular,
            virtual_jobs,
            skipped,
        })
    }
}
pub struct Prepared<'a> {
    regular: Vec<&'a Dataset>,
    virtual_jobs: Vec<Dataset>,
    pub skipped: Vec<String>,
}
impl Prepared<'_> {
    pub fn datasets(&self) -> impl Iterator<Item = &Dataset> {
        self.regular.iter().copied().chain(self.virtual_jobs.iter())
    }
    pub fn is_empty(&self) -> bool {
        self.regular.is_empty() && self.virtual_jobs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_job_features, Geometry, Vertex};
    fn trench(label: &str) -> VirtualTrench {
        VirtualTrench::new(
            label.into(),
            Vertex {
                x: 0.,
                y: 0.,
                z: None,
            },
            Vertex {
                x: 10.,
                y: 0.,
                z: None,
            },
            1.,
        )
        .unwrap()
    }
    #[test]
    fn selected_loaded_jobs_are_borrowed_and_unselected_jobs_are_omitted() {
        let json = r#"{"features":[{"geometry":{"type":"Point","coordinates":[1,2]}}]}"#;
        let jobs = vec![
            parse_job_features(json, 1, "PS1").unwrap(),
            parse_job_features(json, 2, "PS2").unwrap(),
        ];
        let mut selection = Selection::all(2, 0);
        selection.datasets[0] = false;
        let prepared = selection.prepare(&jobs, &[]).unwrap();
        let exported: Vec<_> = prepared.datasets().collect();
        assert_eq!(exported.len(), 1);
        assert!(std::ptr::eq(exported[0], &jobs[1]));
        assert!(selection.prepare(&jobs[..1], &[]).is_err());
    }
    #[test]
    fn virtual_endpoints_are_explicit_and_empty_jobs_are_reported_in_original_path_order() {
        let trenches = vec![trench("PS2"), trench("PS1")];
        let mut selection = Selection::all(0, 2);
        let prepared = selection.prepare(&[], &trenches).unwrap();
        let jobs: Vec<_> = prepared.datasets().collect();
        assert_eq!(jobs[0].title, "PS1");
        assert_eq!(jobs[0].features.len(), 2);
        assert!(matches!(jobs[0].features[0].geometry, Geometry::Point(_)));
        selection.include_endpoints = false;
        let prepared = selection.prepare(&[], &trenches).unwrap();
        assert!(prepared.is_empty());
        assert_eq!(prepared.skipped, ["PS1", "PS2"]);
    }
}
