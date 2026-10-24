//! Non-destructive editor operations for native and imported virtual trenches.
//! Axis/width edits protect measured MarXact geometry; structural edits use a
//! separate manual copy. Explicit whole-layer map moves translate all geometry
//! and the alignment baseline together without rewriting source files.
use crate::{
    model::Vertex,
    virtual_trench::{TrenchPoint, VirtualTrench},
};

pub const MAX_OBJECTS: usize = 100_000;
const MAX_TEXT: usize = 1024;

fn valid_text(text: &str, max: usize) -> bool {
    text.len() <= max && !text.chars().any(|c| c.is_control() && c != '\n')
}

pub fn check(trench: &VirtualTrench) -> Result<(), String> {
    trench.validate()?;
    if trench.label.trim().is_empty() || !valid_text(&trench.label, 256) {
        return Err("Geef een geldige proefsleufnaam van maximaal 256 bytes".into());
    }
    if trench.objects.len() > MAX_OBJECTS {
        return Err("Te veel meetobjecten in één proefsleuf".into());
    }
    for object in &trench.objects {
        if !valid_text(&object.role, MAX_TEXT)
            || !valid_text(&object.object_name, MAX_TEXT)
            || !valid_text(&object.source_name, MAX_TEXT)
            || object.attributes.iter().any(|a| !valid_text(a, MAX_TEXT))
            || object.position.z.is_some_and(|z| !z.is_finite())
        {
            return Err("Ongeldige objectnaam, attribuut of hoogte".into());
        }
    }
    Ok(())
}
pub fn is_measured(t: &VirtualTrench) -> bool {
    !t.boundary_3d.is_empty() || t.alignment.is_some()
}
/// A point on the axis, plus the signed perpendicular offset in RD meters.
/// Positive offset is to the left in the start→end direction.
pub fn position(t: &VirtualTrench, chainage: f64, offset: f64, z: Option<f64>)
    -> Result<Vertex, String>
{
    t.validate()?;
    if !chainage.is_finite() || !offset.is_finite()
        || z.is_some_and(|v| !v.is_finite())
        || chainage.abs()>1_000_000. || offset.abs()>1_000_000.
    {return Err("Ongeldige kettingmaat, dwarsafstand of Z-hoogte".into());}
    let length=t.length();
    let dx=(t.end.x-t.start.x)/length;
    let dy=(t.end.y-t.start.y)/length;
    let result=Vertex {
        x:t.start.x + dx*chainage - dy*offset,
        y:t.start.y + dy*chainage + dx*offset,
        z,
    };
    if !result.x.is_finite() || !result.y.is_finite() {return Err("RD-coördinaten vallen buiten bereik".into());}
    Ok(result)
}
pub fn coordinates(t: &VirtualTrench, p: Vertex) -> (f64, f64) {
    let length=t.length();
    if length<=1e-9 {return (0., 0.);}
    let dx=(t.end.x-t.start.x)/length;
    let dy=(t.end.y-t.start.y)/length;
    let x=p.x-t.start.x;
    let y=p.y-t.start.y;
    (x*dx+y*dy, -x*dy+y*dx)
}
/// Work on a detached candidate and apply only after all validation succeeds.
pub fn edit_axis(t:&mut VirtualTrench,label:String,start:Vertex,end:Vertex,width:f64)
    -> Result<(), String>
{
    if is_measured(t) && (t.start!=start || t.end!=end || t.width_meters!=width) {
        return Err("MarXact-grens is gemeten: maak eerst een handmatige kopie om de meetas te veranderen".into());
    }
    let mut candidate=t.clone();
    candidate.label=label;
    candidate.start=start;
    candidate.end=end;
    candidate.width_meters=width;
    sort_objects(&mut candidate);
    check(&candidate)?;
    *t=candidate;
    Ok(())
}
pub fn replace_object(t:&mut VirtualTrench,index:Option<usize>,object:TrenchPoint)
    -> Result<(), String>
{
    check(t)?;
    if index.is_none() && t.objects.len() >= MAX_OBJECTS {
        return Err("Maximumaantal meetobjecten bereikt".into());
    }
    let mut next=t.clone();
    if let Some(i)=index {
        let target=next.objects.get_mut(i).ok_or("Meetobject bestaat niet meer")?;
        *target=object;
    } else {
        next.objects.push(object);
    }
    sort_objects(&mut next);
    check(&next)?;
    *t=next;
    Ok(())
}
pub fn remove_object(t:&mut VirtualTrench,index:usize) -> Result<(), String> {
    if index>=t.objects.len() {return Err("Meetobject bestaat niet".into());}
    t.objects.remove(index);
    Ok(())
}
pub fn sort_objects(t:&mut VirtualTrench) {
    let start=t.start;
    let dx=t.end.x-start.x;
    let dy=t.end.y-start.y;
    t.objects.sort_by(|a,b|{
        let chain=|p:Vertex|(p.x-start.x)*dx+(p.y-start.y)*dy;
        chain(a.position).total_cmp(&chain(b.position))
    });
}
pub fn reverse_axis(t:&mut VirtualTrench)->Result<(),String>{
    if is_measured(t) {
        return Err("MarXact-uitlijning omkeren vereist opnieuw berekenen van de originele meetas".into());
    }
    let mut next=t.clone();
    std::mem::swap(&mut next.start,&mut next.end);
    std::mem::swap(&mut next.start_metadata,&mut next.end_metadata);
    let length=next.length();
    for row in &mut next.dekband_rows {
        std::mem::swap(&mut row.start_depth,&mut row.end_depth);
        let old_start=row.start_chainage;
        row.start_chainage=length-row.end_chainage;
        row.end_chainage=length-old_start;
    }
    next.dekband_rows.sort_by(|a,b|a.start_chainage.total_cmp(&b.start_chainage));
    sort_objects(&mut next);
    check(&next)?;
    *t=next;
    Ok(())
}
/// Translate entire manually drawn trench including its objects; preserving
/// an imported measured 3D boundary is never optional or implicit.
pub fn translate(t:&mut VirtualTrench,dx:f64,dy:f64)->Result<(),String> {
    if is_measured(t) {
        return Err("Oorspronkelijke MarXact-grens kan niet handmatig worden verschoven".into());
    }
    if !dx.is_finite() || !dy.is_finite() || dx.abs()>1e6 || dy.abs()>1e6 {
        return Err("Ongeldige verplaatsing".into());
    }
    let mut next=t.clone();
    let move_p=|p:&mut Vertex|{p.x+=dx;p.y+=dy;};
    move_p(&mut next.start);
    move_p(&mut next.end);
    for object in &mut next.objects {move_p(&mut object.position);}
    check(&next)?;
    *t=next;
    Ok(())
}
/// Paste from a spreadsheet in the order:
/// role, object, source, chainage, signed offset, optional Z, attr1..attr3.
/// Comma decimals are supported; column separators must be tab characters.
/// All rows are validated before a single atomic mutation is performed.
pub fn import_tabular(t:&mut VirtualTrench,source:&str)->Result<usize,String> {
    if source.len()>1_000_000 {return Err("Excel-invoer is groter dan 1 MB".into());}
    check(t)?;
    let mut new_objects=Vec::new();
    for (line_no,line) in source.lines().enumerate() {
        if line.trim().is_empty(){continue;}
        if new_objects.is_empty() && line_no==0 &&
            (line.to_lowercase().starts_with("rol\t") ||
            line.to_lowercase().starts_with("role\t")) {continue;}
        if new_objects.len()>=10_000 {return Err("Maximaal 10.000 Excel-regels per import".into());}
        let cols=line.split('\t').map(str::trim).collect::<Vec<_>>();
        if !(5..=9).contains(&cols.len()) {
            return Err(format!("Regel {}: verwacht 5 t/m 9 tab-gescheiden kolommen",line_no+1));
        }
        let number=|index:usize|->Result<f64,String>{
            cols[index].replace(',',".").parse::<f64>()
                .map_err(|_|format!("Regel {}: ongeldige meterwaarde in kolom {}",line_no+1,index+1))
        };
        let chainage=number(3)?;
        let offset=number(4)?;
        let z=if cols.get(5).is_some_and(|v|!v.is_empty()) {Some(number(5)?)}else{None};
        let mut attributes:[String;3]=Default::default();
        for (i,value) in cols.iter().skip(6).enumerate() {
            if i<3 {attributes[i]=(*value).to_owned();}
        }
        new_objects.push(TrenchPoint {metadata:Default::default(),
            role:cols[0].to_owned(),object_name:cols[1].to_owned(),
            source_name:cols[2].to_owned(),
            position:position(t,chainage,offset,z)?,
            attributes,
        });
    }
    if new_objects.is_empty() {return Err("Er zijn geen geldige meetobjectregels gevonden".into());}
    if t.objects.len().saturating_add(new_objects.len())>MAX_OBJECTS {
        return Err("Te veel meetobjecten na import".into());
    }
    let n=new_objects.len();
    let mut next=t.clone();
    next.objects.extend(new_objects);
    sort_objects(&mut next);
    check(&next)?;
    *t=next;
    Ok(n)
}
pub fn export_tabular(t:&VirtualTrench)->Result<String,String> {
    check(t)?;
    let mut out=String::from("Rol\tObject\tBron\tKettingmaat\tDwarsafstand\tZ\tAttribuut1\tAttribuut2\tAttribuut3\n");
    for object in &t.objects {
        let (chainage,offset)=coordinates(t,object.position);
        if !chainage.is_finite() || !offset.is_finite(){return Err("Ongeldige meetobjectpositie".into());}
        // Tabs/newlines cannot be faithfully represented in TSV fields.
        let cell=|value:&str| value.replace('\t'," ").replace('\n'," ").replace('\r'," ");
        out.push_str(&format!("{}\t{}\t{}\t{:.4}\t{:.4}\t{}\t{}\t{}\t{}\n",
            cell(&object.role),cell(&object.object_name),cell(&object.source_name),
            chainage,offset,object.position.z.map_or(String::new(),|z|format!("{z:.4}")),
            cell(&object.attributes[0]),cell(&object.attributes[1]),cell(&object.attributes[2])));
    }
    if out.len()>10_000_000 {return Err("Excel-export overschrijdt het maximum".into());}
    Ok(out)
}
/// Preserve measured 3D original; create a standalone editable derivative.
pub fn manual_copy(original:&VirtualTrench,suffix:&str)->Result<VirtualTrench,String> {
    let reference=crate::virtual_map::reference(original)?;
    let mut copy=original.clone();
    copy.label=format!("{} {}",original.label,suffix).trim().to_owned();
    copy.boundary_3d.clear();
    copy.alignment=None;
    copy.source_path=None;
    copy.job_id=0;
    if let Some(metadata)=copy.metadata.as_object_mut(){
        for key in ["rotation_degrees","base_transform_matrix","kickthemap_job_id","kickthemap_job_title","kickthemap_job_features_path","_sleufbase_python_tiff_index"]{metadata.remove(key);}
        if let Some([x,y])=reference{metadata.insert(crate::raster_points::REFERENCE.into(),serde_json::json!({"x":x,"y":y}));}
    }
    check(&copy)?;
    Ok(copy)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn trench()->VirtualTrench {
        VirtualTrench::new("Test".into(),
            Vertex{x:100.,y:200.,z:None},
            Vertex{x:110.,y:200.,z:None},0.6).unwrap()
    }
    fn sample_object(chain:f64,offset:f64)->TrenchPoint {
        let p=position(&trench(),chain,offset,Some(-1.25)).unwrap();
        TrenchPoint {metadata:Default::default(),
            role:"leiding".into(),object_name:"LS".into(),
            source_name:"A".into(),position:p,attributes:Default::default()
        }
    }
    #[test]
    fn chainage_offset_roundtrip_and_reverse_axis() {
        let mut t=trench();
        for (c,o) in [(0.,0.),(3.5,-0.4),(8.2,0.9),(12.,-1.0)] {
            let point=position(&t,c,o,Some(-2.)).unwrap();
            let (got_c,got_o)=coordinates(&t,point);
            assert!((got_c-c).abs()<1e-9);
            assert!((got_o-o).abs()<1e-9);
        }
        let p=position(&t,2.,0.4,None).unwrap();
        reverse_axis(&mut t).unwrap();
        let (c,o)=coordinates(&t,p);
        assert!((c-8.).abs()<1e-9);
        assert!((o+0.4).abs()<1e-9);
    }
    #[test]
    fn failed_edits_preserve_original_bytes_and_names() {
        let mut t=trench();
        let original=serde_json::to_vec(&t).unwrap();
        let start=t.start;
        let end=t.end;
        assert!(edit_axis(&mut t,"".into(),start,end,0.6).is_err());
        assert_eq!(serde_json::to_vec(&t).unwrap(),original);
        assert!(replace_object(&mut t,None,TrenchPoint {metadata:Default::default(),
            object_name:"A".into(),source_name:"B".into(),role:"C".into(),
            position:Vertex{x:f64::NAN,y:0.,z:None},
            attributes:Default::default(),
        }).is_err());
        assert_eq!(serde_json::to_vec(&t).unwrap(),original);
    }
    #[test]
    fn measured_boundary_is_immutable_and_copy_is_editable() {
        let mut t=trench();
        t.boundary_3d=vec![
            Vertex{x:100.,y:200.,z:Some(-1.)},
            Vertex{x:110.,y:200.,z:Some(-1.)},
            Vertex{x:110.,y:202.,z:Some(-1.)},
        ];
        t.source_path=Some("source.dxf".into());
        t.job_id=443;
        let measured=t.clone();
        let changed=Vertex{x:99.,y:200.,z:None};
        let end=t.end;
        assert!(edit_axis(&mut t,"Changed".into(),changed,end,0.8).is_err());
        assert_eq!(t.start,measured.start);
        assert_eq!(t.boundary_3d,measured.boundary_3d);
        assert!(reverse_axis(&mut t).is_err());
        assert!(translate(&mut t,2.,0.).is_err());
        let mut new=manual_copy(&t,"(handmatig)").unwrap();
        assert!(new.boundary_3d.is_empty());
        assert!(new.source_path.is_none());
        assert_eq!(new.job_id,0);
        let new_end=new.end;
        edit_axis(&mut new,"Copy".into(),changed,new_end,0.8).unwrap();
        assert_eq!(t.label,"Test");
    }
    #[test]
    fn paste_from_excel_is_atomic_and_roundtrips_to_tsv() {
        let mut t=trench();
        let good="Rol\tObject\tBron\tKettingmaat\tDwarsafstand\tZ\tA\tB\tC\nLS\tKabel\tIngemeten\t2,5\t-0,4\t-1,2\t1\t2\t3\nGAS\tBuis\tGB\t7.0\t0.2\t\t\t\t";
        assert_eq!(import_tabular(&mut t,good).unwrap(),2);
        assert_eq!(t.objects[0].position,Vertex{x:102.5,y:199.6,z:Some(-1.2)});
        let original=serde_json::to_vec(&t).unwrap();
        assert!(import_tabular(&mut t,"LS\tA\tB\t8\t0\nGAS\tC\tD\tinvalid\t1").is_err());
        assert_eq!(serde_json::to_vec(&t).unwrap(),original);
        let tsv=export_tabular(&t).unwrap();
        let mut target=trench();
        assert_eq!(import_tabular(&mut target,&tsv).unwrap(),2);
        assert_eq!(target.objects.len(),t.objects.len());
        assert_eq!(target.objects[0].source_name,t.objects[0].source_name);
    }
    #[test]
    fn reverse_axis_preserves_geographic_dekband_segment_and_old_projects() {
        let mut t=trench();
        t.start_metadata=serde_json::json!({"surface_mode":"auto","surface_z":4.,"point_id":"start"});
        t.end_metadata=serde_json::json!({"surface_mode":"manual","surface_z":5.,"point_id":"end"});
        let original=serde_json::to_value(&t).unwrap();
        t.dekband_rows.push(crate::virtual_trench::Dekband{start_depth:0.2,end_depth:0.7,
            start_chainage:1.,end_chainage:3.,
            source_name:"LS dekband".into(),display_rgb:Some([12,34,56]),
        });
        let json=serde_json::to_value(&t).unwrap();
        let restored:VirtualTrench=serde_json::from_value(json.clone()).unwrap();
        assert_eq!(restored.dekband_rows.len(),1);
        let mut older=json;
        older.as_object_mut().unwrap().remove("dekband_rows");
        let old:VirtualTrench=serde_json::from_value(older).unwrap();
        assert!(old.dekband_rows.is_empty());
        reverse_axis(&mut t).unwrap();
        assert_eq!(t.dekband_rows[0].start_chainage,7.);
        assert_eq!(t.dekband_rows[0].end_chainage,9.);
        assert_eq!(t.dekband_rows[0].display_rgb,Some([12,34,56]));
        assert_eq!((t.dekband_rows[0].start_depth,t.dekband_rows[0].end_depth),(0.7,0.2));
        assert_eq!(t.start_metadata,original["end_metadata"]);
        assert_eq!(t.end_metadata,original["start_metadata"]);
    }
    #[test]
    fn add_edit_delete_sort_and_shift_objects() {
        let mut t=trench();
        replace_object(&mut t,None,sample_object(8.,-0.2)).unwrap();
        replace_object(&mut t,None,sample_object(2.,0.3)).unwrap();
        assert_eq!(t.objects[0].position.x,102.);
        let mut changed=t.objects[0].clone();
        changed.position=position(&t,10.,-0.1,Some(-0.5)).unwrap();
        replace_object(&mut t,Some(0),changed).unwrap();
        assert_eq!(t.objects[1].position.x,110.);
        translate(&mut t,25.,-10.).unwrap();
        assert_eq!(t.objects[1].position.x,135.);
        remove_object(&mut t,0).unwrap();
        assert_eq!(t.objects.len(),1);
        let data=t.to_dataset(0,true);
        assert_eq!(data.features.len(),3);
    }
}
