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
    ModelAttribute(usize, usize),
    BlockAttribute(Arc<Block>, usize, usize),
}
impl Source {
    fn get<'a>(&'a self, overlay: &'a DxfOverlay) -> &'a DxfFeature {
        match self {
            Self::Model(i) => &overlay.features[*i],
            Self::Block(block, i) => &block.features[*i],
            Self::ModelAttribute(i, a) => &overlay.features[*i].linked_attributes()[*a],
            Self::BlockAttribute(block, i, a) => &block.features[*i].linked_attributes()[*a],
        }
    }
    fn attribute(&self, index: usize) -> Result<Self, String> {
        match self {
            Self::Model(i) => Ok(Self::ModelAttribute(*i, index)),
            Self::Block(block, i) => Ok(Self::BlockAttribute(block.clone(), *i, index)),
            _ => Err("Een ATTRIB kan geen eigen ATTRIB bezitten".into()),
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
    path_breaks: Box<[usize]>,
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
    pub path_breaks: &'a [usize],
    pub layer: &'a str,
    pub rgb: [u8; 3],
    pub visible: bool,
}
impl<'a> DrawRef<'a> {
    pub fn paths(&self) -> impl Iterator<Item = &'a [crate::model::Vertex]> + '_ {
        let mut start = 0;
        self.path_breaks
            .iter()
            .copied()
            .chain(std::iter::once(self.points.len()))
            .map(move |end| {
                let path = self.points.get(start..end).unwrap_or_default();
                start = end;
                path
            })
    }
}
struct Builder<'a> {
    overlay: &'a DxfOverlay,
    scene: Scene,
    points: usize,
    attempts: usize,
    stack: Vec<String>,
    layers: BTreeMap<String, Arc<str>>,
    cancel: &'a std::sync::atomic::AtomicBool,
    warnings: BTreeSet<String>,
}
impl Builder<'_> {
    fn layer(&mut self, name: &str) -> Arc<str> {
        let name = self.overlay.canonical_layer_name(name);
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
        crate::input::check(self.cancel)?;
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
            let (points, path_breaks) =
                if let Some(text) = source.cad.as_deref().and_then(|p| p.text.as_deref()) {
                    let output = text.contours.flatten(
                        parent,
                        0.005,
                        MAX_POINTS.saturating_sub(self.points),
                        self.cancel,
                    )?;
                    (output.points, output.breaks.into_boxed_slice())
                } else if let Some(spline) = source.cad.as_deref().and_then(|p| p.spline.as_deref())
                {
                    let output =
                        spline.flatten(parent, 0.25, MAX_POINTS.saturating_sub(self.points))?;
                    (output.points, output.breaks.into_boxed_slice())
                } else if let Some(curve) = source.cad.as_deref().and_then(|p| p.curve.as_deref()) {
                    (
                        curve.flatten(parent, 0.25, MAX_POINTS.saturating_sub(self.points))?,
                        Box::default(),
                    )
                } else {
                    (
                        source
                            .points
                            .iter()
                            .map(|p| parent.checked_point(g::xyz(*p)).map(g::vertex))
                            .collect::<Result<Vec<_>, _>>()?,
                        source.cad.as_ref().map_or_else(Box::default, |p| {
                            p.path_breaks.clone().into_boxed_slice()
                        }),
                    )
                };
            return self.append(RenderFeature {
                source: reference,
                points,
                path_breaks,
                layer,
                rgb,
                instance,
            });
        }
        let key = source.block_name.to_lowercase();
        let missing = !self.overlay.blocks.contains_key(&key);
        let block = if let Some(block) = self.overlay.blocks.get(&key) {
            block.clone()
        } else {
            let err = format!("Blokdefinitie ontbreekt: {}", source.block_name);
            if source.linked_attributes().is_empty() {
                return Err(err);
            }
            if self.warnings.len() < 100 {
                self.warnings.insert(err);
            }
            Arc::new(Block {
                name: source.block_name.clone(),
                base: [0.; 3],
                features: Vec::new(),
            })
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
        let invisible =
            source.color_aci < 0 || source.cad.as_deref().is_some_and(|p| p.is_hidden());
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
                if missing {
                    let point = g::vertex(matrix.checked_point([0.; 3])?);
                    self.append(RenderFeature {
                        source: reference.clone(),
                        points: vec![point, point],
                        path_breaks: Box::default(),
                        layer: layer.clone(),
                        rgb,
                        instance: Some(at),
                    })?;
                }
                // ATTRIB geometry already lives outside this INSERT's local
                // block space. Only the outer parent and the rotated, unscaled
                // MINSERT grid offset apply. Translate the entire contour,
                // including both FIT/ALIGNED endpoints (ezdxf 1.4.4 moves only
                // insert and leaves align_point behind).
                let (s, c) = placement.rotation.rem_euclid(360.).to_radians().sin_cos();
                let x = f64::from(column) * placement.column_spacing;
                let y = f64::from(row) * placement.row_spacing;
                let offset = [c * x - s * y, s * x + c * y, 0.];
                for (index, attribute) in source.linked_attributes().iter().enumerate() {
                    let normal = attribute
                        .cad
                        .as_deref()
                        .and_then(|p| p.text.as_deref())
                        .map_or([0., 0., 1.], |t| t.layout.normal);
                    let delta = Affine::ocs(normal)?.vector(offset);
                    let mut shift = Affine::IDENTITY;
                    for (i, value) in delta.into_iter().enumerate() {
                        shift.0[i][3] = value;
                    }
                    self.visit(
                        reference.attribute(index)?,
                        parent.compose(shift)?,
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
        self.rebuild_block_cache_with_cancel(&crate::input::NEVER_CANCEL)
    }
    pub fn rebuild_block_cache_with_cancel(
        &mut self,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<(), String> {
        crate::input::check(cancel)?;
        for feature in self.source_features() {
            if !feature.linked_attributes().is_empty() && feature.entity_type != "INSERT" {
                return Err("Alleen INSERT mag gekoppelde ATTRIB bevatten".into());
            }
            if let Some(cad) = feature.cad.as_deref() {
                cad.validate_geometry(feature.points.len())?;
            }
        }
        let controls = self
            .source_features()
            .map(|f| {
                f.cad
                    .as_deref()
                    .and_then(|p| p.text.as_deref())
                    .map_or(0, |t| t.contours.points.len())
            })
            .try_fold(0usize, |used, n| {
                used.checked_add(n)
                    .filter(|&n| n <= crate::cad_text_path::MAX_CONTROLS)
            });
        if controls.is_none() {
            return Err("CAD-bronnen overschrijden tekstbudget".into());
        }
        let layer_warnings = self.rebuild_layer_aliases();
        for block in self.blocks.values() {
            g::finite(block.base)?;
        }
        let points = self
            .source_features()
            .map(|f| {
                f.points.len().saturating_add(
                    f.cad
                        .as_deref()
                        .and_then(|p| p.mesh.as_ref())
                        .map_or(0, |m| m.vertices.len()),
                )
            })
            .try_fold(0usize, |count, extra| {
                count.checked_add(extra).filter(|n| *n <= MAX_POINTS)
            })
            .ok_or("CAD-bronpunten overschrijden het weergavebudget")?;
        let mut builder = Builder {
            overlay: self,
            scene: Scene::default(),
            points,
            attempts: 0,
            stack: Vec::new(),
            layers: BTreeMap::new(),
            cancel,
            warnings: BTreeSet::new(),
        };
        let mut warnings: BTreeSet<_> = layer_warnings.into_iter().collect();
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
                crate::input::check(cancel)?;
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
                    path_breaks: Box::default(),
                    layer,
                    rgb,
                    instance: None,
                })?;
            }
        }
        warnings.extend(builder.warnings);
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
            && !source.cad.as_deref().is_some_and(|p| p.is_hidden())
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
            path_breaks: &feature.path_breaks,
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
                path_breaks: source
                    .cad
                    .as_ref()
                    .map_or(&[], |p| p.path_breaks.as_slice()),
                layer: self.canonical_layer_name(&source.layer),
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
                path_breaks: source
                    .cad
                    .as_ref()
                    .map_or(&[], |p| p.path_breaks.as_slice()),
                layer: self.canonical_layer_name(&source.layer),
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
