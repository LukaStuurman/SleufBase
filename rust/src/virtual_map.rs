//! Virtual-layer map selection, whole-layer translation and raw RD references.
use crate::{model::Vertex, raster_points, virtual_trench::VirtualTrench};
use serde_json::json;

/// Python's case-insensitive original name pattern and Unicode-14 decimal
/// suffixes, without a fixed-width integer overflow or newer Unicode digits.
pub fn next_name(names: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    let mut maximum = "0".to_owned();
    for name in names {
        let mut chars = crate::python_values::trim(name.as_ref()).chars();
        let matches = "virtuele proefsleuf".chars().all(|expected| {
            let actual = chars.next().map(|c| match c {
                'İ' | 'ı' => 'i',
                'ſ' => 's',
                c => c.to_ascii_lowercase(),
            });
            actual == Some(expected)
        });
        if !matches {
            continue;
        }
        let tail = chars.collect::<String>();
        let number = if tail.is_empty() {
            Some("1".into())
        } else {
            tail.strip_prefix(' ')
                .filter(|s| !s.is_empty())
                .and_then(|s| {
                    s.chars()
                        .map(crate::object_layout::decimal)
                        .collect::<Option<Vec<_>>>()
                })
                .map(|digits| {
                    digits
                        .into_iter()
                        .map(|d| char::from(b'0' + d as u8))
                        .collect::<String>()
                })
        };
        let Some(number) = number else { continue };
        let digits = number.trim_start_matches('0');
        let digits = if digits.is_empty() { "0" } else { digits };
        if digits.len() > maximum.len()
            || digits.len() == maximum.len() && digits > maximum.as_str()
        {
            maximum = digits.into();
        }
    }
    let mut digits = maximum.into_bytes();
    let mut carry = true;
    for d in digits.iter_mut().rev() {
        if *d == b'9' {
            *d = b'0';
        } else {
            *d += 1;
            carry = false;
            break;
        }
    }
    if carry {
        digits.insert(0, b'1');
    }
    format!("Virtuele proefsleuf {}", String::from_utf8(digits).unwrap())
}

fn point(p: [f64; 2]) -> Result<(), String> {
    if p.iter().any(|v| !v.is_finite() || v.abs() > 1e12) {
        Err("Ongeldige RD-kaartcoördinaten".into())
    } else {
        Ok(())
    }
}
pub fn center(t: &VirtualTrench) -> [f64; 2] {
    [(t.start.x + t.end.x) * 0.5, (t.start.y + t.end.y) * 0.5]
}
/// Actual virtual image centre, also when a point lies outside the axis/outline.
pub fn raster_center(t: &VirtualTrench) -> [f64; 2] {
    let mut min = [t.start.x.min(t.end.x), t.start.y.min(t.end.y)];
    let mut max = [t.start.x.max(t.end.x), t.start.y.max(t.end.y)];
    let mut include = |x: f64, y: f64| {
        min[0] = min[0].min(x);
        min[1] = min[1].min(y);
        max[0] = max[0].max(x);
        max[1] = max[1].max(y);
    };
    if t.boundary_3d.len() >= 3 {
        for p in &t.boundary_3d {
            include(p.x, p.y);
        }
    } else {
        for (x, y) in t.polygon() {
            include(x, y);
        }
    }
    for p in &t.objects {
        include(p.position.x, p.position.y);
    }
    let pad = (t.width_meters * 3.).max(1.5);
    // Same padded-bounds arithmetic as the actual original image transform.
    [
        ((min[0] - pad) + (max[0] + pad)) * 0.5,
        ((min[1] - pad) + (max[1] + pad)) * 0.5,
    ]
}
pub fn reference(t: &VirtualTrench) -> Result<Option<[f64; 2]>, String> {
    let Some(raw) = t
        .metadata
        .get(raster_points::REFERENCE)
        .and_then(raster_points::point)
    else {
        return Ok(None);
    };
    crate::profile_reference::Reference {
        raw,
        chainage: 0.,
        description: String::new(),
    }
    .display(&t.metadata, raster_center(t))
    .map(Some)
}
pub fn set_reference(t: &mut VirtualTrench, displayed: [f64; 2]) -> Result<bool, String> {
    point(displayed)?;
    let bytes = raster_points::validate_metadata(&t.metadata)?;
    let angle = crate::python_values::number(t.metadata.get("rotation_degrees")).unwrap_or(0.);
    let raw = crate::profile_reference::Reference {
        raw: displayed,
        chainage: 0.,
        description: String::new(),
    }
    .display(&json!({"rotation_degrees":-angle}), raster_center(t))?;
    point(raw)?;
    let value = json!({"x":raw[0],"y":raw[1]});
    if t.metadata.get(raster_points::REFERENCE) == Some(&value) {
        return Ok(false);
    }
    if bytes + 128 > 2 * 1024 * 1024 {
        return Err("Geen ruimte voor referentiepunt in laagmetadata".into());
    }
    if t.metadata.is_null() {
        t.metadata = json!({});
    }
    t.metadata
        .as_object_mut()
        .unwrap()
        .insert(raster_points::REFERENCE.into(), value);
    Ok(true)
}
pub fn clear_reference(t: &mut VirtualTrench) -> bool {
    t.metadata
        .as_object_mut()
        .and_then(|m| m.remove(raster_points::REFERENCE))
        .is_some()
}
pub fn create(
    label: String,
    start: [f64; 2],
    end: [f64; 2],
    width: f64,
) -> Result<VirtualTrench, String> {
    point(start)?;
    point(end)?;
    if (end[0] - start[0]).hypot(end[1] - start[1]) < 0.5 {
        return Err("Kies een eindpunt op minstens 0,5 m van het beginpunt".into());
    }
    let t = VirtualTrench::new(
        label,
        Vertex {
            x: start[0],
            y: start[1],
            z: None,
        },
        Vertex {
            x: end[0],
            y: end[1],
            z: None,
        },
        width,
    )?;
    crate::virtual_trench_editor::check(&t)?;
    Ok(t)
}
/// Validate every result before mutating. Coordinates, boundaries and alignment
/// baselines move together; Z, reference metadata, source files and allocations
/// stay intact. The original app moved points but left a measured outline behind.
pub fn translate(t: &mut VirtualTrench, dx: f64, dy: f64) -> Result<bool, String> {
    crate::virtual_trench_editor::check(t)?;
    if !dx.is_finite() || !dy.is_finite() || dx.abs() > 1e6 || dy.abs() > 1e6 {
        return Err("Ongeldige verplaatsing".into());
    }
    if dx == 0. && dy == 0. {
        return Ok(false);
    }
    let shifted = |p: Vertex| Vertex {
        x: p.x + dx,
        y: p.y + dy,
        ..p
    };
    for p in std::iter::once(t.start)
        .chain(std::iter::once(t.end))
        .chain(t.objects.iter().map(|o| o.position))
        .chain(t.boundary_3d.iter().copied())
        .chain(t.alignment.iter().flat_map(|a| [a.auto_start, a.auto_end]))
    {
        let p = shifted(p);
        point([p.x, p.y])?;
    }
    let start = shifted(t.start);
    let end = shifted(t.end);
    if (end.x - start.x).hypot(end.y - start.y) <= 1e-9 {
        return Err("Verplaatsing zou de meetas laten samenvallen".into());
    }
    if let Some(a) = &t.alignment {
        let mut next = a.clone();
        next.auto_start = shifted(a.auto_start);
        next.auto_end = shifted(a.auto_end);
        next.validate()?;
    }
    t.start = start;
    t.end = end;
    for p in &mut t.objects {
        p.position = shifted(p.position);
    }
    for p in &mut t.boundary_3d {
        *p = shifted(*p);
    }
    if let Some(a) = &mut t.alignment {
        a.auto_start = shifted(a.auto_start);
        a.auto_end = shifted(a.auto_end);
    }
    Ok(true)
}
/// Original reversed-source nearest hit selection and inclusive width/tolerance.
pub fn pick(
    trenches: &[VirtualTrench],
    p: [f64; 2],
    mpp: f64,
    selection_tolerance: f64,
) -> Option<usize> {
    point(p).ok()?;
    if !mpp.is_finite() || mpp <= 0. || !selection_tolerance.is_finite() {
        return None;
    }
    let tolerance = (mpp * 14.).max(selection_tolerance).min(4.);
    let mut best = None;
    let mut distance = f64::INFINITY;
    for (i, t) in trenches.iter().enumerate().rev() {
        let (x, y, _, _) = t.project(Vertex {
            x: p[0],
            y: p[1],
            z: None,
        });
        let d = (x - p[0]).hypot(y - p[1]);
        if d <= t.width_meters * 0.5 + tolerance && d < distance {
            best = Some(i);
            distance = d;
        }
    }
    best
}

/// Stable identity while a map gesture owns its source; objects keep source
/// order when only their position changes, avoiding index changes mid-gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapPoint { Start, End, Object(usize) }
pub fn position(t: &VirtualTrench, target: MapPoint) -> Option<Vertex> {
    match target { MapPoint::Start=>Some(t.start),MapPoint::End=>Some(t.end),MapPoint::Object(i)=>t.objects.get(i).map(|p|p.position) }
}
pub fn pick_point(t:&VirtualTrench,p:[f64;2],mpp:f64,tolerance:f64)->Option<MapPoint>{
    point(p).ok()?;
    if !mpp.is_finite() || mpp<=0. || !tolerance.is_finite(){return None;}
    let limit=(10.*mpp).max(tolerance).min(3.);
    let mut points=vec![MapPoint::Start,MapPoint::End];
    points.extend((0..t.objects.len()).map(MapPoint::Object));
    points.sort_by(|a,b|t.chainage(position(t,*a).unwrap()).partial_cmp(&t.chainage(position(t,*b).unwrap())).unwrap_or(std::cmp::Ordering::Equal));
    let mut nearest=None;let mut distance=f64::INFINITY;
    for target in points {
        let v=position(t,target)?;let d=(v.x-p[0]).hypot(v.y-p[1]);
        if d<=limit && d<distance {nearest=Some(target);distance=d;}
    }
    nearest
}
/// Original axis transform: normalized along-axis coordinates scale with the
/// new length, while signed perpendicular distances remain measured metres.
pub fn transform_axis(t:&mut VirtualTrench,start:[f64;2],end:[f64;2])->Result<bool,String>{
    crate::virtual_trench_editor::check(t)?;point(start)?;point(end)?;
    if crate::virtual_trench_editor::is_measured(t){return Err("Maak een handmatige kopie voordat u de gemeten meetas verandert".into());}
    let length=(end[0]-start[0]).hypot(end[1]-start[1]);
    if length<0.5 {return Err("Zet begin- en eindpunt minstens 0,5 m uit elkaar".into());}
    if start==[t.start.x,t.start.y] && end==[t.end.x,t.end.y]{return Ok(false);}
    let old_length=t.length();let ux=(end[0]-start[0])/length;let uy=(end[1]-start[1])/length;
    let result=|p:Vertex|->Result<Vertex,String>{
        let (along,offset)=crate::virtual_trench_editor::coordinates(t,p);
        let along=along/old_length*length;
        let p=Vertex{x:start[0]+ux*along-uy*offset,y:start[1]+uy*along+ux*offset,z:p.z};
        point([p.x,p.y])?;Ok(p)
    };
    // Validate all coordinates before applying; no partially changed layer on
    // a bad map point and no allocation of a full second source/metadata copy.
    let points=t.objects.iter().map(|p|result(p.position)).collect::<Result<Vec<_>,_>>()?;
    t.start.x=start[0];t.start.y=start[1];t.end.x=end[0];t.end.y=end[1];
    for (object,p) in t.objects.iter_mut().zip(points){object.position=p;}
    Ok(true)
}
pub fn move_point(t:&mut VirtualTrench,target:MapPoint,p:[f64;2])->Result<bool,String>{
    point(p)?;crate::virtual_trench_editor::check(t)?;
    match target {
        MapPoint::Start=>transform_axis(t,p,[t.end.x,t.end.y]),
        MapPoint::End=>transform_axis(t,[t.start.x,t.start.y],p),
        MapPoint::Object(i)=>{
            let (x,y,_,_)=t.project(Vertex{x:p[0],y:p[1],z:None});
            let object=t.objects.get_mut(i).ok_or("Meetpunt bestaat niet meer")?;
            let changed=object.position.x!=x || object.position.y!=y;
            object.position.x=x;object.position.y=y;Ok(changed)
        }
    }
}
pub fn copy_target_end(t:&VirtualTrench,start:[f64;2],direction:[f64;2])->Result<[f64;2],String>{
    point(start)?;point(direction)?;t.validate()?;
    let dx=direction[0]-start[0];let dy=direction[1]-start[1];let length=dx.hypot(dy);
    if length<=1e-9{return Err("Kies een richting vanaf het nieuwe beginpunt".into());}
    let scale=t.length()/length;let end=[start[0]+dx*scale,start[1]+dy*scale];point(end)?;Ok(end)
}
pub fn copy_in_direction(t:&VirtualTrench,label:String,start:[f64;2],direction:[f64;2])->Result<VirtualTrench,String>{
    let end=copy_target_end(t,start,direction)?;
    let mut copy=crate::virtual_trench_editor::manual_copy(t,"")?;
    copy.label=label;transform_axis(&mut copy,start,end)?;
    // This is a new map layer: old references and annotations describe the old
    // location, so preserve the full source but do not transplant those points.
    for key in [raster_points::REFERENCE,crate::map_comments::KEY]{
        if let Some(meta)=copy.metadata.as_object_mut(){meta.remove(key);}
    }
    crate::virtual_trench_editor::check(&copy)?;Ok(copy)
}
pub fn add_dekband(t:&mut VirtualTrench,a:[f64;2],b:[f64;2],source_name:String,rgb:Option<[u8;3]>)->Result<bool,String>{
    add_dekband_with_depth(t,a,b,source_name,rgb,0.,0.)
}
pub fn add_dekband_with_depth(t:&mut VirtualTrench,a:[f64;2],b:[f64;2],source_name:String,rgb:Option<[u8;3]>,a_depth:f64,b_depth:f64)->Result<bool,String>{
    point(a)?;point(b)?;crate::virtual_trench_editor::check(t)?;
    if t.dekband_rows.len()>=10_000{return Err("Maximumaantal dekbandsegmenten bereikt".into());}
    if source_name.len()>256 || source_name.chars().any(char::is_control){return Err("Ongeldige dekbandomschrijving".into());}
    if [a_depth,b_depth].iter().any(|d|!d.is_finite()||*d<0.||*d>1e6){return Err("Ongeldige dekbanddiepte".into());}
    let along=|p:[f64;2]| t.project(Vertex{x:p[0],y:p[1],z:None}).2;
    let (a,b)=(along(a),along(b));let start=a.min(b);let end=a.max(b);
    if end<=start+1e-6{return Err("Prik twee verschillende kettingmaten voor de dekband".into());}
    let(start_depth,end_depth)=if a<=b{(a_depth,b_depth)}else{(b_depth,a_depth)};
    t.dekband_rows.push(crate::virtual_trench::Dekband{start_chainage:start,end_chainage:end,start_depth,end_depth,source_name,display_rgb:rgb});
    Ok(true)
}

pub fn point_depth(metadata:&serde_json::Value,z:Option<f64>)->f64{
    crate::python_values::number(metadata.get("sleufbase_depth_meters"))
        .or_else(||crate::python_values::number(metadata.get("surface_z")).zip(z).map(|(surface,z)|surface-z))
        .filter(|d|d.is_finite()).unwrap_or(0.)
}
/// Python's original str(value or "").strip().lower() surface-mode contract.
pub fn point_surface_mode(metadata:&serde_json::Value)->String{
    crate::python_values::string(metadata.get("surface_mode").filter(|v|crate::python_values::truth(v))).unwrap_or_default().to_lowercase()
}
/// Apply an asynchronous original endpoint AHN response. Imported measured or
/// explicit manual points retain their elevations; only original auto endpoints
/// and objects explicitly attached to start ground level participate.
pub fn apply_surface_height(t:&mut VirtualTrench,target:MapPoint,height:Option<f64>)->Result<bool,String>{
    crate::virtual_trench_editor::check(t)?;
    if height.is_some_and(|v|!v.is_finite()||v.abs()>1e6){return Err("Ongeldige AHN-hoogte".into());}
    if crate::virtual_trench_editor::is_measured(t){return Ok(false);}
    let (p,old_meta)=match target{MapPoint::Start=>(t.start,&t.start_metadata),MapPoint::End=>(t.end,&t.end_metadata),MapPoint::Object(_)=>return Err("Meetobjecten volgen het beginmaaiveld; AHN hoort bij de eindpunten".into())};
    let mode=point_surface_mode(old_meta);
    if mode!="auto" && (!mode.is_empty() || p.z.is_some() || crate::virtual_trench_editor::is_measured(t)){return Ok(false);}
    let mut meta=old_meta.as_object().cloned().unwrap_or_default();
    // Recognize Python's normalized mode without rewriting its archived text.
    // Newly created endpoints without a mode still receive the canonical value.
    if mode.is_empty(){meta.insert("surface_mode".into(),json!("auto"));}
    meta.insert("surface_status".into(),json!(if height.is_some(){"resolved"}else{"unavailable"}));
    let depth=point_depth(old_meta,p.z);let next_z=height.map(|z|z-depth).or(p.z);
    if let Some(z)=height{meta.insert("surface_z".into(),json!(z));}
    let next_meta=serde_json::Value::Object(meta);
    let mut updates=Vec::new();let mut bytes=t.point_metadata_bytes()? as i64;
    let size=|meta:&serde_json::Value|crate::raster_points::validate_metadata(meta).map(|b|b as i64);
    bytes+=size(&next_meta)?-size(old_meta)?;
    if target==MapPoint::Start {
        if let Some(surface)=height {
            for (i,object) in t.objects.iter().enumerate(){
                if point_surface_mode(&object.metadata)!="start"{continue;}
                let depth=point_depth(&object.metadata,object.position.z);
                let mut meta=object.metadata.as_object().cloned().unwrap_or_default();
                meta.insert("surface_mode".into(),json!("start"));
                meta.insert("surface_z".into(),json!(surface));meta.insert("surface_status".into(),json!("start"));
                let meta=serde_json::Value::Object(meta);bytes+=size(&meta)?-size(&object.metadata)?;
                updates.push((i,surface-depth,meta));
            }
        }
    }
    if bytes>2*1024*1024{return Err("AHN-puntmetadata overschrijdt het laagbudget".into());}
    let changed=p.z!=next_z||*old_meta!=next_meta||updates.iter().any(|(i,z,m)|t.objects[*i].position.z!=Some(*z)||t.objects[*i].metadata!=*m);
    match target{MapPoint::Start=>{t.start.z=next_z;t.start_metadata=next_meta;},MapPoint::End=>{t.end.z=next_z;t.end_metadata=next_meta;},_=>unreachable!()}
    for (i,z,meta) in updates{t.objects[i].position.z=Some(z);t.objects[i].metadata=meta;}
    Ok(changed)
}
