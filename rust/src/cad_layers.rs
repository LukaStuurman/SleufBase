//! Resolve CAD symbol-table layer names once, retaining original entity spelling.
use crate::{cad::Layer, dxf_read::DxfOverlay};
use std::{collections::BTreeMap, sync::Arc};

impl DxfOverlay {
    pub fn canonical_layer_name<'a>(&'a self, name: &'a str) -> &'a str {
        self.layer_aliases.get(name).map_or(name, AsRef::as_ref)
    }

    pub fn resolve_layer(&self, name: &str) -> Option<&Layer> {
        self.layers.get(name).or_else(|| {
            self.layer_aliases
                .get(name)
                .and_then(|canonical| self.layers.get(canonical.as_ref()))
        })
    }

    pub(crate) fn rebuild_layer_aliases(&mut self) -> Vec<String> {
        let mut canonical = BTreeMap::<String, Arc<str>>::new();
        let mut warnings = Vec::new();
        for name in self.layers.keys() {
            let key = name.to_lowercase();
            if let Some(previous) = canonical.get(&key) {
                // Older previews could store spurious case variants, including
                // different user visibility choices. Keep them; do not guess
                // which was the source table definition or erase either choice.
                if warnings.len() < 100 {
                    warnings.push(format!("Opgeslagen CAD-laagvarianten {previous} / {name}; herlaad de DXF-bron om de oorspronkelijke laagdefinitie te herstellen"));
                }
            } else {
                canonical.insert(key, Arc::from(name.as_str()));
            }
        }
        let mut aliases = BTreeMap::new();
        for source in self
            .features
            .iter()
            .chain(self.blocks.values().flat_map(|b| b.features.iter()))
            .flat_map(|f| std::iter::once(f).chain(f.linked_attributes()))
        {
            if self.layers.contains_key(&source.layer) || aliases.contains_key(&source.layer) {
                continue;
            }
            let key = source.layer.to_lowercase();
            if let Some(name) = canonical.get(&key) {
                aliases.insert(source.layer.clone(), name.clone());
            } else {
                self.layers.insert(source.layer.clone(), Layer::default());
                canonical.insert(key, Arc::from(source.layer.as_str()));
            }
        }
        self.layer_aliases = Arc::new(aliases);
        warnings
    }
}
