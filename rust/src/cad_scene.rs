//! Shared block sources and cached WCS geometry; no per-instance metadata copies.
use crate::{
    cad_geometry::{self as g, Affine, Placement, V3},
    dxf_read::{DxfFeature, DxfOverlay},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
const MAX_POINTS: usize = 10_000_000;
const MAX_INSTANCES: usize = 2_000_000;
const MAX_DEPTH: usize = 32;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub name: String,
    pub base: V3,
    pub features: Vec<DxfFeature>,
}
#[derive(Clone, Debug)]
enum Source {
    Model(usize),
    Block(Arc<Block>, usize),
}
impl Source {
    fn get<'a>(&'a self, overlay: &'a DxfOverlay) -> &'a DxfFeature {
        match self {
            Self::Model(i) => &overlay.features[*i],
            Self::Block(block, i) => &block.features[*i],
        }
    }
}
#[derive(Debug)]
struct Instance {
    source: Source,
    block: Arc<Block>,
    parent: Option<usize>,
    layer: Arc<str>,
    invisible: bool,
    row: u32,
    column: u32,
}
#[derive(Debug)]
pub struct RenderFeature {
    source: Source,
    pub points: Vec<crate::model::Vertex>,
    layer: Arc<str>,
    rgb: [u8; 3],
    instance: Option<usize>,
}
#[derive(Debug, Default)]
pub struct Scene {
    pub features: Vec<RenderFeature>,
    instances: Vec<Instance>,
}
pub struct DrawRef<'a> {
    pub source: &'a DxfFeature,
    pub points: &'a [crate::model::Vertex],
    pub layer: &'a str,
    pub rgb: [u8; 3],
    pub visible: bool,
}
struct Builder<'a> {
    overlay: &'a DxfOverlay,
    scene: Scene,
    points: usize,
    attempts: usize,
    stack: Vec<String>,
    layers: BTreeMap<String, Arc<str>>,
}
impl Builder<'_> {
    fn layer(&mut self, name: &str) -> Arc<str> {
        if let Some(layer) = self.layers.get(name) {
            return layer.clone();
        }
        let layer: Arc<str> = Arc::from(name);
        self.layers.insert(name.to_owned(), layer.clone());
        layer
    }

    fn rgb(&self, source: &DxfFeature, layer: &str, parent: Option<[u8; 3]>) -> [u8; 3] {
        if source.color_aci == 0 && source.cad.as_deref().and_then(|p| p.true_color).is_none() {
            if let Some(rgb) = parent {
                return rgb;
            }
        }
        crate::cad::display_rgb(
            source.color_aci,
            source.cad.as_deref(),
            self.overlay.layers.get(layer),
        )
    }
    fn append(&mut self, feature: RenderFeature) -> Result<(), String> {
        let points = self
            .points
            .checked_add(feature.points.len())
            .ok_or("Te veel CAD-blokpunten")?;
        if points > MAX_POINTS || self.scene.features.len() >= MAX_INSTANCES {
            return Err("CAD-blokken overschrijden het weergavebudget".into());
        }
        for point in &feature.points {
            g::finite(g::xyz(*point))?;
        }
        self.points = points;
        self.scene.features.push(feature);
        Ok(())
    }
    fn visit(
        &mut self,
        reference: Source,
        parent: Affine,
        instance: Option<usize>,
        parent_layer: Option<&str>,
        parent_rgb: Option<[u8; 3]>,
    ) -> Result<(), String> {
        let source = reference.get(self.overlay);
        let effective = if source.layer == "0" {
            parent_layer.unwrap_or(&source.layer)
        } else {
            &source.layer
        };
        // Intern potentially large layer names once, not once for every leaf.
        let layer = self.layer(effective);
        let rgb = self.rgb(source, &layer, parent_rgb);
        if source.entity_type != "INSERT" {
            let points = if let Some(curve) = source.cad.as_deref().and_then(|p| p.curve.as_deref())
            {
                curve.flatten(parent, 0.25, MAX_POINTS.saturating_sub(self.points))?
            } else {
                source
                    .points
                    .iter()
                    .map(|p| parent.checked_point(g::xyz(*p)).map(g::vertex))
                    .collect::<Result<Vec<_>, _>>()?
            };
            return self.append(RenderFeature {
                source: reference,
                points,
                layer,
                rgb,
                instance,
            });
        }
        let key = source.block_name.to_lowercase();
        let Some(block) = self.overlay.blocks.get(&key) else {
            return Err(format!("Blokdefinitie ontbreekt: {}", source.block_name));
        };
        if self.stack.len() >= MAX_DEPTH || self.stack.contains(&key) {
            return Err(format!("Recursief of te diep CAD-blok: {}", block.name));
        }
        let fallback = Placement {
            insert: source.points.first().map_or([0.; 3], |p| g::xyz(*p)),
            scale: [1.; 3],
            normal: [0., 0., 1.],
            rotation: 0.,
            rows: 1,
            columns: 1,
            row_spacing: 0.,
            column_spacing: 0.,
        };
        let placement = source
            .cad
            .as_deref()
            .and_then(|p| p.placement.as_deref())
            .unwrap_or(&fallback);
        placement.validate()?;
        let count =
            usize::try_from(placement.grid_size()).map_err(|_| "Te veel CAD-blokinstanties")?;
        self.attempts = self
            .attempts
            .checked_add(count)
            .ok_or("Te veel CAD-blokinstanties")?;
        if self.attempts > MAX_INSTANCES {
            return Err("Te veel CAD-blokinstanties".into());
        }
        let invisible = source.color_aci < 0 || source.cad.as_deref().is_some_and(|p| p.invisible);
        self.stack.push(key);
        let rows = if placement.row_spacing == 0. {
            1
        } else {
            placement.rows
        };
        let columns = if placement.column_spacing == 0. {
            1
        } else {
            placement.columns
        };
        for row in 0..rows {
            for column in 0..columns {
                let matrix = parent.compose(placement.matrix(block.base, row, column)?)?;
                let at = self.scene.instances.len();
                self.scene.instances.push(Instance {
                    source: reference.clone(),
                    block: block.clone(),
                    parent: instance,
                    layer: layer.clone(),
                    invisible,
                    row,
                    column,
                });
                for index in 0..block.features.len() {
                    self.visit(
                        Source::Block(block.clone(), index),
                        matrix,
                        Some(at),
                        Some(&layer),
                        Some(rgb),
                    )?;
                }
            }
        }
        self.stack.pop();
        Ok(())
    }
}
impl DxfOverlay {
    pub fn rebuild_block_cache(&mut self) -> Result<(), String> {
        for block in self.blocks.values() {
            g::finite(block.base)?;
        }
        let points = self
            .features
            .iter()
            .chain(self.blocks.values().flat_map(|b| b.features.iter()))
            .map(|f| f.points.len())
            .sum();
        let mut builder = Builder {
            overlay: self,
            scene: Scene::default(),
            points,
            attempts: 0,
            stack: Vec::new(),
            layers: BTreeMap::new(),
        };
        let mut warnings = BTreeSet::new();
        for (index, source) in self.features.iter().enumerate() {
            if source.entity_type != "INSERT" {
                continue;
            }
            let before = builder.scene.features.len();
            let before_instances = builder.scene.instances.len();
            let points = builder.points;
            if let Err(err) =
                builder.visit(Source::Model(index), Affine::IDENTITY, None, None, None)
            {
                builder.scene.features.truncate(before);
                builder.scene.instances.truncate(before_instances);
                builder.points = points;
                builder.stack.clear();
                if warnings.len() < 100 {
                    warnings.insert(err);
                }
                let layer = builder.layer(&source.layer);
                let rgb = self.feature_rgb(source);
                builder.append(RenderFeature {
                    source: Source::Model(index),
                    points: source.points.clone(),
                    layer,
                    rgb,
                    instance: None,
                })?;
            }
        }
        self.expanded = Arc::new(builder.scene);
        self.block_warnings = warnings.into_iter().collect();
        self.refresh_block_visibility();
        Ok(())
    }
    /// Recompute only on a layer change, never on pan/zoom or a normal frame.
    pub fn refresh_block_visibility(&mut self) {
        let mut visible: Vec<bool> = Vec::with_capacity(self.expanded.instances.len());
        for instance in &self.expanded.instances {
            visible.push(
                !instance.invisible
                    && instance.parent.is_none_or(|p| visible[p])
                    && self
                        .layers
                        .get(instance.layer.as_ref())
                        .is_none_or(|l| l.visible),
            );
        }
        self.block_visibility = Arc::new(visible);
        self.cached_bounds = self.calculate_bounds();
        self.bounds_ready = true;
    }
    fn drawn<'a>(&'a self, feature: &'a RenderFeature) -> DrawRef<'a> {
        let source = feature.source.get(self);
        let visible = source.color_aci >= 0
            && !source.cad.as_deref().is_some_and(|p| p.invisible)
            && self
                .layers
                .get(feature.layer.as_ref())
                .is_none_or(|l| l.visible)
            && feature
                .instance
                .is_none_or(|i| self.block_visibility.get(i) == Some(&true));
        DrawRef {
            source,
            points: &feature.points,
            layer: &feature.layer,
            rgb: feature.rgb,
            visible,
        }
    }
    pub fn rendered_features(&self) -> impl Iterator<Item = DrawRef<'_>> {
        self.features
            .iter()
            .filter(|f| f.entity_type != "INSERT")
            .map(|source| DrawRef {
                source,
                points: &source.points,
                layer: &source.layer,
                rgb: self.feature_rgb(source),
                visible: self.feature_visible(source),
            })
            .chain(self.expanded.features.iter().map(|f| self.drawn(f)))
    }
    pub fn inspect_feature(&self, index: usize) -> Option<DrawRef<'_>> {
        if index < self.features.len() {
            let source = &self.features[index];
            Some(DrawRef {
                source,
                points: &source.points,
                layer: &source.layer,
                rgb: self.feature_rgb(source),
                visible: self.feature_visible(source),
            })
        } else {
            self.expanded
                .features
                .get(index - self.features.len())
                .map(|f| self.drawn(f))
        }
    }
    pub fn inspect_details(&self, index: usize) -> Option<BTreeMap<String, String>> {
        let drawing = self.inspect_feature(index)?;
        let mut details = drawing.source.details(&self.path);
        details.insert("Laag".into(), drawing.layer.to_owned());
        details.insert("Punten".into(), drawing.points.len().to_string());
        if drawing.source.layer != drawing.layer {
            details.insert("Bronlaag".into(), drawing.source.layer.clone());
        }
        if index >= self.features.len() {
            let mut instance = self.expanded.features[index - self.features.len()].instance;
            let mut path = Vec::new();
            while let Some(i) = instance {
                let entry = &self.expanded.instances[i];
                let source = entry.source.get(self);
                path.push(format!(
                    "{}:{}[{},{}]",
                    source.handle, entry.block.name, entry.row, entry.column
                ));
                instance = entry.parent;
            }
            path.reverse();
            if !path.is_empty() {
                details.insert("Blokpad".into(), path.join(" / "));
            }
        }
        Some(details)
    }
}
