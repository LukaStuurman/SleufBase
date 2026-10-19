//! Regenerate single-line attributes from ATTDEFs with full 3D affine text
//! metrics, signed/nonuniform scales, rotation and old/new OCS transforms.
use crate::{
    cad_geometry::{self as g, Affine, Placement, V3},
    dxf_document::{Document, RecordView},
    input,
    template_copy::{self as copy, Copies, Tags},
};
use std::{collections::HashMap, sync::atomic::AtomicBool};
fn scoped(t: &Tags, name: &str) -> Tags {
    let mut result = Vec::new();
    let mut active = false;
    let mut depth = 0;
    for (c, v) in t {
        if *c == 102 {
            if v.starts_with('{') {
                depth += 1;
            } else if v == "}" {
                depth -= 1;
            }
            continue;
        }
        if depth != 0 {
            continue;
        }
        if *c == 100 {
            active = v == name;
            continue;
        }
        if *c == 101 || *c >= 1000 {
            break;
        }
        if active {
            result.push((*c, v.clone()));
        }
    }
    result
}
fn has_embedded_object(t: &Tags) -> bool {
    let mut depth = 0;
    for (c, v) in t {
        if *c == 102 {
            if v.starts_with('{') {
                depth += 1;
            } else if v == "}" {
                depth -= 1;
            }
        } else if depth == 0 {
            if *c >= 1000 {
                break;
            }
            if *c == 101 {
                return true;
            }
        }
    }
    false
}
fn number(t: &Tags, code: i32, default: f64) -> Result<f64, String> {
    let n = copy::first(t, code).map_or(Ok(default), |v| {
        v.trim()
            .parse::<f64>()
            .map_err(|_| "Ongeldige attribuuttransformatie".to_string())
    })?;
    if !n.is_finite() {
        return Err("Niet-eindige attribuuttransformatie".into());
    }
    Ok(n)
}
fn point(t: &Tags, code: i32, default: V3) -> Result<V3, String> {
    Ok([
        number(t, code, default[0])?,
        number(t, code + 10, default[1])?,
        number(t, code + 20, default[2])?,
    ])
}
fn unit(v: V3) -> Result<V3, String> {
    let n = g::length(v);
    if n == 0. || !n.is_finite() {
        return Err("Ongeldige attribuutrichting".into());
    }
    g::finite(g::scale(v, 1. / n))
}
fn inverse(ocs: Affine, v: V3) -> V3 {
    std::array::from_fn(|i| (0..3).map(|j| ocs.0[j][i] * v[j]).sum())
}
fn put(t: &mut Tags, code: i32, v: f64) -> Result<(), String> {
    if !v.is_finite() {
        return Err("Attribuutmaat buiten bereik".into());
    }
    t.push((code, v.to_string()));
    Ok(())
}
fn vector(t: &mut Tags, code: i32, p: V3) -> Result<(), String> {
    for (i, v) in p.into_iter().enumerate() {
        put(t, code + i as i32 * 10, v)?;
    }
    Ok(())
}
fn attribute(
    def: &Tags,
    value: Option<&str>,
    matrix: Affine,
    handle: u64,
    owner: u64,
    r2018: bool,
) -> Result<Option<Tags>, String> {
    if has_embedded_object(def) {
        return Err("Multiline ATTDEF-regeneratie wordt nog niet ondersteund".into());
    }
    let entity = scoped(def, "AcDbEntity");
    let text = scoped(def, "AcDbText");
    let fields = scoped(def, "AcDbAttributeDefinition");
    let Some(tag) = copy::first(&fields, 2) else {
        return Ok(None);
    };
    if copy::first(&text, 10).is_none() {
        return Ok(None);
    }
    let old = Affine::ocs(point(&text, 210, [0., 0., 1.])?)?;
    let x = matrix.vector(old.vector([1., 0., 0.]));
    let y = matrix.vector(old.vector([0., 1., 0.]));
    let normal = unit(g::cross(x, y))?;
    let new = Affine::ocs(normal)?;
    let transform =
        |p: V3| -> Result<V3, String> { g::finite(inverse(new, matrix.point(old.vector(p)))) };
    let direction = |p: V3| -> V3 { inverse(new, matrix.vector(old.vector(p))) };
    let original = point(&text, 10, [0.; 3])?;
    let at = transform(original)?;
    let align = transform(point(&text, 11, original)?)?;
    let rotation = number(&text, 50, 0.)?;
    let angle = rotation.to_radians();
    let (s, c) = angle.sin_cos();
    let rotated = direction([c, s, 0.]);
    let new_rotation = rotated[1].atan2(rotated[0]).to_degrees();
    let x_scale = g::length(matrix.vector(old.vector([c, s, 0.])));
    // Match the original independent angle evaluation, including its numeric
    // uniform-plane threshold; do not infer a rotated axis by swapping components.
    let vertical = (rotation + 90.).to_radians();
    let (vs, vc) = vertical.sin_cos();
    let mut y_scale = g::length(matrix.vector(old.vector([vc, vs, 0.])));
    let xx = g::dot(x, x);
    let yy = g::dot(y, y);
    let uniform = (xx - yy).abs() <= 1e-9f64.max(1e-9 * xx.abs().max(yy.abs()));
    let mut oblique = number(&text, 51, 0.)?;
    if !uniform {
        let angle = (rotation + 90. - oblique).to_radians();
        let (s, c) = angle.sin_cos();
        let v = direction([c, s, 0.]);
        oblique = new_rotation + 90. - v[1].atan2(v[0]).to_degrees();
        y_scale *= oblique.to_radians().cos();
    }
    let height = number(&text, 40, 2.5)? * y_scale;
    let width = number(&text, 41, 1.)? * x_scale / y_scale;
    if height <= 0. || width <= 0. || !height.is_finite() || !width.is_finite() {
        return Err("Ongeldige getransformeerde attribuutmaat".into());
    }
    let mut t = vec![
        (0, "ATTRIB".into()),
        (5, format!("{handle:X}")),
        (330, format!("{owner:X}")),
        (100, "AcDbEntity".into()),
        (67, "1".into()),
    ];
    t.push((8, copy::first(&entity, 8).unwrap_or("0").into()));
    for (c, v) in &entity {
        if matches!(
            *c,
            6 | 62 | 48 | 60 | 370 | 420 | 430 | 440 | 347 | 390 | 284
        ) {
            t.push((*c, v.clone()));
        }
    }
    t.push((100, "AcDbText".into()));
    vector(&mut t, 10, at)?;
    put(&mut t, 40, height)?;
    t.push((
        1,
        value.unwrap_or(copy::first(&text, 1).unwrap_or("")).into(),
    ));
    if copy::first(&text, 39).is_some() {
        put(&mut t, 39, direction([0., 0., number(&text, 39, 0.)?])[2])?;
    }
    put(&mut t, 50, new_rotation)?;
    put(&mut t, 41, width)?;
    if !uniform || copy::first(&text, 51).is_some() {
        put(&mut t, 51, oblique)?;
    }
    for code in [7, 71, 72] {
        if let Some(v) = copy::first(&text, code) {
            t.push((code, v.into()));
        }
    }
    vector(&mut t, 11, align)?;
    vector(&mut t, 210, normal)?;
    t.push((100, "AcDbAttribute".into()));
    t.push((2, tag.into()));
    for code in [70, 73, 74, 280] {
        if let Some(v) = fields
            .iter()
            .rev()
            .find(|(c, _)| *c == code)
            .map(|(_, v)| v)
        {
            t.push((code, v.clone()));
        }
    }
    // The source R2018 exporter writes these supplementary values even for
    // ordinary single-line attributes; both alignment locations agree.
    if r2018 {
        t.push((71, "1".into()));
        t.push((72, "0".into()));
        vector(&mut t, 11, align)?;
    }
    Ok(Some(t))
}
/// The legend helper deliberately recreates an INSERT using the production
/// subset of source fields, then regenerates attributes from its block's ATTDEFs.
pub(crate) struct Blueprint(Vec<Tags>);
impl Blueprint {
    /// Geometry/text metrics are shared between pages; only identities and
    /// owners differ. Never expose the unbound zero-handle blueprint as DXF.
    pub fn instantiate(&self, owner: u64, copies: &mut Copies<'_>) -> Result<Vec<Tags>, String> {
        let root = copies.allocate()?;
        let mut result = Vec::with_capacity(self.0.len());
        for (i, source) in self.0.iter().enumerate() {
            let id = if i == 0 { root } else { copies.allocate()? };
            let mut tags = source.clone();
            copy::replace(&mut tags, 5, format!("{id:X}"));
            copy::replace(
                &mut tags,
                330,
                format!("{:X}", if i == 0 { owner } else { root }),
            );
            result.push(tags);
        }
        Ok(result)
    }
}
pub(crate) fn prepare_legend<'a>(
    doc: &'a Document,
    source: RecordView<'a>,
    children: &[RecordView<'a>],
    cancel: &AtomicBool,
) -> Result<Blueprint, String> {
    input::check(cancel)?;
    let t = copy::tags(source);
    let name = copy::first(&t, 2).ok_or("Legenda mist bloknaam")?;
    let wanted = crate::dxf_symbols::key(name);
    let mut base = [0.; 3];
    let mut found = false;
    let mut active = false;
    let mut definitions = Vec::new();
    for r in doc.records().filter(|r| r.section() == Some("BLOCKS")) {
        if r.index().is_multiple_of(1024) {
            input::check(cancel)?;
        }
        if r.kind() == "BLOCK" {
            active = r
                .first(2)
                .is_some_and(|n| crate::dxf_symbols::key(n) == wanted);
            if active {
                if found {
                    return Err("Dubbele legendablokdefinitie".into());
                }
                found = true;
                base = point(&copy::tags(r), 10, [0.; 3])?;
            }
        } else if r.kind() == "ENDBLK" {
            active = false;
        } else if active && r.kind() == "ATTDEF" {
            if definitions.len() >= 10_000 {
                return Err("Te veel legenda-attributen".into());
            }
            definitions.push(copy::tags(r));
        }
    }
    if !found {
        return Err(format!("Legendablok {name} ontbreekt"));
    }
    let values = children
        .iter()
        .filter(|r| r.kind() == "ATTRIB")
        .filter_map(|r| {
            r.first(2)
                .map(|tag| (tag.to_string(), r.first(1).unwrap_or("").to_string()))
        })
        .collect::<HashMap<_, _>>();
    let insert = point(&t, 10, [0.; 3])?;
    let scales = [
        number(&t, 41, 1.)?,
        number(&t, 42, 1.)?,
        number(&t, 43, 1.)?,
    ];
    let rotation = number(&t, 50, 0.)?;
    let placement = Placement {
        insert,
        scale: scales,
        rotation,
        normal: [0., 0., 1.],
        columns: 1,
        rows: 1,
        column_spacing: 0.,
        row_spacing: 0.,
    };
    let matrix = placement.matrix(base, 0, 0)?;
    let mut variable = false;
    let mut r2018 = false;
    for r in doc.records().filter(|r| r.section() == Some("HEADER")) {
        for (c, v) in r.tags() {
            if c == 9 {
                variable = v.trim() == "$ACADVER";
            } else if variable && c == 1 {
                r2018 = v.trim() == "AC1032";
            }
        }
    }
    let mut root = vec![
        (0, "INSERT".into()),
        (5, "0".into()),
        (330, "0".into()),
        (100, "AcDbEntity".into()),
        (67, "1".into()),
        (8, copy::first(&t, 8).unwrap_or("0").into()),
        (100, "AcDbBlockReference".into()),
        (2, name.into()),
    ];
    vector(&mut root, 10, insert)?;
    for (c, v) in [41, 42, 43].into_iter().zip(scales) {
        put(&mut root, c, v)?;
    }
    put(&mut root, 50, rotation)?;
    let mut attributes = Vec::new();
    for def in definitions {
        input::check(cancel)?;
        let field = scoped(&def, "AcDbAttributeDefinition");
        let value = copy::first(&field, 2)
            .and_then(|tag| values.get(tag))
            .map(String::as_str);
        if let Some(t) = attribute(&def, value, matrix, 0, 0, r2018)? {
            attributes.push(t);
        }
    }
    let mut result = Vec::new();
    if !attributes.is_empty() {
        root.push((66, "1".into()));
    }
    result.push(root);
    result.extend(attributes);
    if result.len() > 1 {
        result.push(vec![
            (0, "SEQEND".into()),
            (5, "0".into()),
            (330, "0".into()),
            (100, "AcDbEntity".into()),
            (67, "1".into()),
            (8, copy::first(&t, 8).unwrap_or("0").into()),
        ]);
    }
    Ok(Blueprint(result))
}
