//! Bounded Mapbox Vector Tile 1/2 protobuf and geometry command decoder.
//! Field numbers follow vector-tile-spec/2.1/vector_tile.proto. Geometry order
//! and default Y inversion follow mapbox-vector-tile 2.2's decoder.
use crate::input;
use serde_json::Value;
use std::sync::{atomic::AtomicBool, Arc};
pub(crate) const MAX_VERTICES: usize = 2_000_000;
const MAX_ITEMS: usize = 100_000;
const MAX_WORDS: usize = 6_000_000;
pub(crate) type XY = [i64; 2];
pub(crate) enum Field<'a> {
    Int(u64),
    Bits32(u32),
    Bits64(u64),
    Bytes(&'a [u8]),
    Group,
}
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    cancel: &'a AtomicBool,
}
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], cancel: &'a AtomicBool) -> Self {
        Self {
            bytes,
            at: 0,
            cancel,
        }
    }
    fn varint(&mut self) -> Result<u64, String> {
        let mut value = 0u64;
        for shift in (0..70).step_by(7) {
            let byte = *self.bytes.get(self.at).ok_or("Onvolledige MVT-varint")?;
            self.at += 1;
            if shift == 63 && byte > 1 {
                return Err("MVT-varint loopt over".into());
            }
            value |= ((byte & 127) as u64) << shift;
            if byte & 128 == 0 {
                return Ok(value);
            }
        }
        Err("Te lange MVT-varint".into())
    }
    fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(len).ok_or("MVT-lengte loopt over")?;
        let data = self.bytes.get(self.at..end).ok_or("Onvolledig MVT-veld")?;
        self.at = end;
        Ok(data)
    }
    fn next(&mut self) -> Result<Option<(u32, Field<'a>)>, String> {
        self.next_depth(0, None)
    }
    fn next_depth(
        &mut self,
        depth: usize,
        end: Option<u32>,
    ) -> Result<Option<(u32, Field<'a>)>, String> {
        input::check(self.cancel)?;
        if depth > 16 {
            return Err("MVT-groepen te diep genest".into());
        }
        if self.at == self.bytes.len() {
            return if end.is_some() {
                Err("MVT-groep niet afgesloten".into())
            } else {
                Ok(None)
            };
        }
        let tag = self.varint()?;
        if tag > u32::MAX as u64 || tag >> 3 == 0 {
            return Err("Ongeldige MVT-tag".into());
        }
        let number = (tag >> 3) as u32;
        let value = match tag & 7 {
            0 => Field::Int(self.varint()?),
            1 => Field::Bits64(u64::from_le_bytes(self.take(8)?.try_into().unwrap())),
            2 => {
                let len = usize::try_from(self.varint()?).map_err(|_| "MVT-veld te groot")?;
                Field::Bytes(self.take(len)?)
            }
            3 => {
                while self.next_depth(depth + 1, Some(number))?.is_some() {}
                Field::Group
            }
            4 => {
                return if end == Some(number) {
                    Ok(None)
                } else {
                    Err("Onjuist MVT-groepeinde".into())
                }
            }
            5 => Field::Bits32(u32::from_le_bytes(self.take(4)?.try_into().unwrap())),
            _ => return Err("Onbekend MVT-wiretype".into()),
        };
        Ok(Some((number, value)))
    }
}
impl<'a> Field<'a> {
    fn int(self) -> Result<u64, String> {
        if let Self::Int(v) = self {
            Ok(v)
        } else {
            Err("Onjuist MVT-geheel veld".into())
        }
    }
    fn bytes(self) -> Result<&'a [u8], String> {
        if let Self::Bytes(v) = self {
            Ok(v)
        } else {
            Err("Onjuist MVT-bytesveld".into())
        }
    }
    fn text(self) -> Result<&'a str, String> {
        let bytes = self.bytes()?;
        if bytes.len() > 1024 * 1024 {
            return Err("MVT-tekst te lang".into());
        }
        std::str::from_utf8(bytes).map_err(|_| "Ongeldige MVT-UTF-8".into())
    }
}
fn unsigned(v: u64) -> Result<u32, String> {
    u32::try_from(v).map_err(|_| "MVT-uint32 loopt over".into())
}
fn number(v: f64) -> Value {
    serde_json::Number::from_f64(v)
        .map(Value::Number)
        .unwrap_or_else(|| {
            Value::String(
                if v.is_nan() {
                    "nan"
                } else if v.is_sign_positive() {
                    "inf"
                } else {
                    "-inf"
                }
                .into(),
            )
        })
}
fn property(bytes: &[u8], cancel: &AtomicBool) -> Result<Option<Value>, String> {
    let mut reader = Reader::new(bytes, cancel);
    let mut values: [Option<Value>; 7] = Default::default();
    while let Some((code, field)) = reader.next()? {
        let value = match (code, field) {
            (1, f) => Value::String(f.text()?.into()),
            (2, Field::Bits32(v)) => number(f32::from_bits(v) as f64),
            (3, Field::Bits64(v)) => number(f64::from_bits(v)),
            (4, f) => Value::from(f.int()? as i64),
            (5, f) => Value::from(f.int()?),
            (6, f) => {
                let v = f.int()?;
                Value::from(((v >> 1) as i64) ^ -((v & 1) as i64))
            }
            (7, f) => Value::Bool(f.int()? != 0),
            _ => continue,
        };
        values[(code - 1) as usize] = Some(value);
    }
    // The protobuf message is not a oneof. Match Python's explicit precedence.
    Ok([6, 2, 1, 3, 5, 0, 4]
        .into_iter()
        .find_map(|i| values[i].take()))
}
enum Words<'a> {
    One(u32),
    Packed(&'a [u8]),
}
fn words(field: Field<'_>) -> Result<Words<'_>, String> {
    match field {
        Field::Int(v) => Ok(Words::One(unsigned(v)?)),
        Field::Bytes(b) => Ok(Words::Packed(b)),
        _ => Err("Ongeldig MVT-commando-/tagveld".into()),
    }
}
fn unpack(
    values: Vec<Words<'_>>,
    budget: &mut Budget,
    cancel: &AtomicBool,
) -> Result<Vec<u32>, String> {
    let mut output = Vec::new();
    for value in values {
        match value {
            Words::One(v) => {
                budget.word()?;
                output.push(v);
            }
            Words::Packed(bytes) => {
                let mut reader = Reader::new(bytes, cancel);
                while reader.at < bytes.len() {
                    if output.len() % 1024 == 0 {
                        input::check(cancel)?;
                    }
                    budget.word()?;
                    output.push(unsigned(reader.varint()?)?);
                }
            }
        }
    }
    Ok(output)
}
pub(crate) struct Budget {
    words: usize,
    vertices: usize,
    features: usize,
}
impl Budget {
    fn word(&mut self) -> Result<(), String> {
        self.words += 1;
        if self.words > MAX_WORDS {
            Err("Te veel MVT-woorden".into())
        } else {
            Ok(())
        }
    }
    fn vertex(&mut self) -> Result<(), String> {
        self.vertices += 1;
        if self.vertices > MAX_VERTICES {
            Err("Te veel MVT-vertices".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, serde::Serialize)]
pub(crate) enum Geometry {
    Point(Vec<XY>),
    Line(Vec<Vec<XY>>),
    Polygon(Vec<Vec<Vec<XY>>>),
}
impl Geometry {
    pub(crate) fn payload(&self) -> Value {
        let (kind, coordinates) = match self {
            Self::Point(p) if p.len() == 1 => ("Point", serde_json::json!(p[0])),
            Self::Point(p) => ("MultiPoint", serde_json::json!(p)),
            Self::Line(p) if p.len() == 1 => ("LineString", serde_json::json!(p[0])),
            Self::Line(p) => ("MultiLineString", serde_json::json!(p)),
            Self::Polygon(p) if p.len() == 1 => ("Polygon", serde_json::json!(p[0])),
            Self::Polygon(p) => ("MultiPolygon", serde_json::json!(p)),
        };
        serde_json::json!({"coordinates":coordinates,"type":kind})
    }
    pub(crate) fn vertices(&self) -> usize {
        match self {
            Self::Point(p) => p.len(),
            Self::Line(p) => p.iter().map(Vec::len).sum(),
            Self::Polygon(p) => p.iter().flatten().map(Vec::len).sum(),
        }
    }
}
fn close(coords: &mut Vec<XY>, budget: &mut Budget) -> Result<(), String> {
    if coords.first() != coords.last() {
        budget.vertex()?;
        coords.push(coords[0]);
    }
    Ok(())
}
fn geometry(
    commands: Vec<u32>,
    kind: u32,
    extent: u32,
    budget: &mut Budget,
    cancel: &AtomicBool,
) -> Result<Geometry, String> {
    let mut index = 0;
    let mut previous = [0i64, 0i64];
    let mut coords = Vec::new();
    let mut parts = Vec::new();
    while index < commands.len() {
        input::check(cancel)?;
        let command = commands[index];
        index += 1;
        let op = command & 7;
        let count = command >> 3;
        match op {
            7 if kind == 3 => {
                if count != 1 {
                    return Err("Ongeldig MVT-ClosePath-aantal".into());
                }
                close(&mut coords, budget)?;
                parts.push(std::mem::take(&mut coords));
            }
            1 | 2 => {
                if count == 0 || count as usize > MAX_VERTICES.saturating_sub(budget.vertices) {
                    return Err("Ongeldig of te groot MVT-commando".into());
                }
                if !coords.is_empty() && op == 1 && kind != 1 {
                    if kind == 3 {
                        close(&mut coords, budget)?;
                    }
                    parts.push(std::mem::take(&mut coords));
                }
                for i in 0..count {
                    if i % 1024 == 0 {
                        input::check(cancel)?;
                    }
                    let pair = commands
                        .get(index..index + 2)
                        .ok_or("Onvolledig MVT-coördinaatpaar")?;
                    index += 2;
                    budget.vertex()?;
                    for j in 0..2 {
                        let delta = ((pair[j] >> 1) as i64) ^ -((pair[j] & 1) as i64);
                        previous[j] = previous[j]
                            .checked_add(delta)
                            .ok_or("MVT-coördinaat loopt over")?;
                    }
                    coords.push([previous[0], extent as i64 - previous[1]]);
                }
            }
            _ => return Err("Onbekend of ongeldig MVT-geometriecommando".into()),
        }
    }
    match kind {
        1 => Ok(Geometry::Point(coords)),
        2 => {
            if !coords.is_empty() || parts.is_empty() {
                parts.push(coords);
            }
            Ok(Geometry::Line(parts))
        }
        3 => {
            if !coords.is_empty() {
                parts.push(coords);
            }
            let mut polygons = Vec::new();
            let mut polygon = Vec::new();
            let mut winding = 0;
            for ring in parts {
                let mut area = 0i128;
                for pair in ring.windows(2) {
                    area = area
                        .checked_add(
                            pair[0][0] as i128 * pair[1][1] as i128
                                - pair[1][0] as i128 * pair[0][1] as i128,
                        )
                        .ok_or("MVT-ringoppervlakte loopt over")?;
                }
                let sign = area.signum();
                if sign == 0 {
                    continue;
                }
                if winding == 0 {
                    winding = sign;
                }
                if sign == winding {
                    if !polygon.is_empty() {
                        polygons.push(std::mem::take(&mut polygon));
                    }
                    polygon.push(ring);
                } else {
                    polygon.push(ring);
                }
            }
            if !polygon.is_empty() {
                polygons.push(polygon);
            }
            Ok(Geometry::Polygon(polygons))
        }
        _ => Err("Onbekend MVT-geometrietype".into()),
    }
}
pub(crate) struct Feature {
    pub id: u64,
    pub local_id: Option<Arc<Value>>,
    pub external_id: Option<Arc<Value>>,
    pub appearance: Option<Arc<Value>>,
    pub geometry: Geometry,
}
pub(crate) struct Layer<'a> {
    pub name: &'a str,
    pub extent: u32,
    pub features: Vec<Feature>,
}
fn layer<'a>(
    bytes: &'a [u8],
    budget: &mut Budget,
    cancel: &'a AtomicBool,
) -> Result<Layer<'a>, String> {
    let mut name = "";
    let mut extent = 4096;
    let mut version = 1;
    let mut keys = Vec::new();
    let mut values = Vec::new();
    let mut features = Vec::new();
    let mut reader = Reader::new(bytes, cancel);
    while let Some((code, field)) = reader.next()? {
        match code {
            1 => {
                name = field.text()?;
                if name.len() > 1024 {
                    return Err("MVT-laagnaam te lang".into());
                }
            }
            2 => {
                if features.len() >= MAX_ITEMS {
                    return Err("Te veel MVT-features".into());
                }
                features.push(field.bytes()?);
            }
            3 => {
                if keys.len() >= MAX_ITEMS {
                    return Err("Te veel MVT-sleutels".into());
                }
                keys.push(field.text()?);
            }
            4 => {
                if values.len() >= MAX_ITEMS {
                    return Err("Te veel MVT-waarden".into());
                }
                values.push(property(field.bytes()?, cancel)?.map(Arc::new));
            }
            5 => extent = unsigned(field.int()?)?,
            15 => version = unsigned(field.int()?)?,
            _ => {}
        }
    }
    if !matches!(version, 1 | 2) {
        return Err("Onbekende MVT-versie".into());
    }
    let mut output = Vec::with_capacity(features.len());
    for bytes in features {
        budget.features += 1;
        if budget.features > MAX_ITEMS {
            return Err("Te veel MVT-features in tegel".into());
        }
        let mut id = 0;
        let mut kind = 0;
        let mut tags = Vec::new();
        let mut commands = Vec::new();
        let mut reader = Reader::new(bytes, cancel);
        while let Some((code, field)) = reader.next()? {
            match code {
                1 => id = field.int()?,
                2 => tags.push(words(field)?),
                3 => kind = unsigned(field.int()?)?,
                4 => commands.push(words(field)?),
                _ => {}
            }
        }
        let tags = unpack(tags, budget, cancel)?;
        if tags.len() % 2 != 0 {
            return Err("Oneven MVT-eigenschapstags".into());
        }
        let mut local_id = None;
        let mut external_id = None;
        let mut appearance = None;
        for pair in tags.as_chunks::<2>().0 {
            let key = *keys
                .get(pair[0] as usize)
                .ok_or("MVT-sleutelindex ontbreekt")?;
            let value = values
                .get(pair[1] as usize)
                .and_then(Option::as_ref)
                .ok_or("MVT-waardeindex ontbreekt of is leeg")?;
            match key {
                "lokaal_id" => local_id = Some(value.clone()),
                "external_fid" => external_id = Some(value.clone()),
                "fysiek_voorkomen" => appearance = Some(value.clone()),
                _ => {}
            }
        }
        let geometry = geometry(
            unpack(commands, budget, cancel)?,
            kind,
            extent,
            budget,
            cancel,
        )?;
        output.push(Feature {
            id,
            local_id,
            external_id,
            appearance,
            geometry,
        });
    }
    Ok(Layer {
        name,
        extent,
        features: output,
    })
}
pub(crate) fn decode<'a>(
    bytes: &'a [u8],
    cancel: &'a AtomicBool,
) -> Result<Vec<Layer<'a>>, String> {
    let mut budget = Budget {
        words: 0,
        vertices: 0,
        features: 0,
    };
    let mut layers = Vec::<Layer<'a>>::new();
    let mut reader = Reader::new(bytes, cancel);
    while let Some((code, field)) = reader.next()? {
        if code != 3 {
            continue;
        }
        if layers.len() >= 256 {
            return Err("Te veel MVT-lagen".into());
        }
        let layer = layer(field.bytes()?, &mut budget, cancel)?;
        if let Some(old) = layers.iter_mut().find(|old| old.name == layer.name) {
            *old = layer;
        } else {
            layers.push(layer);
        }
    }
    Ok(layers)
}
