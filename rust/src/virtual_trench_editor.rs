//! Non-destructive editor operations for native and imported virtual trenches.
//! Measured MarXact boundaries remain immutable unless the operator explicitly
//! creates a separate, manually editable copy.
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
    let a=t.start;
    let dx=t.end.x-a.x;
    let dy=t.end.y-a.y;
    t.objects.sort_by(|a,b|{
        let chain=|p:Vertex|(p.x-t.start.x)*dx+(p.y-t.start.y)*dy;
        chain(a.position).total_cmp(&chain(b.position))
    });
}
pub fn reverse_axis(t:&mut VirtualTrench)->Result<(),String>{
    if is_measured(t) {
        return Err("MarXact-uitlijning omkeren vereist opnieuw berekenen van de originele meetas".into());
    }
    let mut next=t.clone();
    std::mem::swap(&mut next.start,&mut next.end);
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
/// Preserve measured 3D original; create a standalone editable derivative.
pub fn manual_copy(original:&VirtualTrench,suffix:&str)->Result<VirtualTrench,String> {
    let mut copy=original.clone();
    copy.label=format!("{} {}",original.label,suffix).trim().to_owned();
    copy.boundary_3d.clear();
    copy.alignment=None;
    copy.source_path=None;
    copy.job_id=0;
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
        TrenchPoint {
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
        assert!(edit_axis(&mut t,"".into(),t.start,t.end,0.6).is_err());
        assert_eq!(serde_json::to_vec(&t).unwrap(),original);
        assert!(replace_object(&mut t,None,TrenchPoint {
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
        assert!(edit_axis(&mut t,"Changed".into(),changed,t.end,0.8).is_err());
        assert_eq!(t.start,measured.start);
        assert_eq!(t.boundary_3d,measured.boundary_3d);
        assert!(reverse_axis(&mut t).is_err());
        assert!(translate(&mut t,2.,0.).is_err());
        let mut new=manual_copy(&t,"(handmatig)").unwrap();
        assert!(new.boundary_3d.is_empty());
        assert!(new.source_path.is_none());
        assert_eq!(new.job_id,0);
        edit_axis(&mut new,"Copy".into(),changed,new.end,0.8).unwrap();
        assert_eq!(t.label,"Test");
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
