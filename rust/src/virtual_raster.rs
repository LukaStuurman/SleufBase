//! Synthetic high-resolution raster for virtual/MarXact trenches without an
//! external KickTheMap GeoTIFF. Follows Python virtual_trench_render_geometry:
//! RD bounds with 1.5 m margin, 220..1600 pixels/axis, transparent background,
//! outlines and transverse object markers. No fake satellite imagery is used.
use crate::{model::Bounds, virtual_trench::VirtualTrench, input};
use image::{Rgba,RgbaImage};
use serde::Deserialize;
use std::sync::atomic::AtomicBool;

#[derive(Clone,Debug,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {pub trench:VirtualTrench,#[serde(default="default_quality")]pub quality:f64}
fn default_quality()->f64{1.}
#[derive(Clone,Copy,Debug)]
pub struct Layout {pub bounds:Bounds,pub width:u32,pub height:u32}
pub fn layout(t:&VirtualTrench,quality:f64)->Result<Layout,String>{
    t.validate()?;
    if !quality.is_finite() || !(0.25..=4.).contains(&quality) {
        return Err("Virtuele rasterkwaliteit moet tussen 0,25 en 4 liggen".into());
    }
    if t.objects.len()>100_000 || t.boundary_3d.len()>100_000 {
        return Err("Te veel proefsleuf- of MarXact-punten".into());
    }
    let mut bounds=Bounds{
        min_x:t.start.x.min(t.end.x),min_y:t.start.y.min(t.end.y),
        max_x:t.start.x.max(t.end.x),max_y:t.start.y.max(t.end.y),
    };
    for (x,y) in t.measured_boundary(){
        bounds.min_x=bounds.min_x.min(x);bounds.min_y=bounds.min_y.min(y);
        bounds.max_x=bounds.max_x.max(x);bounds.max_y=bounds.max_y.max(y);
    }
    for object in &t.objects {
        let p=object.position;
        bounds.min_x=bounds.min_x.min(p.x);bounds.min_y=bounds.min_y.min(p.y);
        bounds.max_x=bounds.max_x.max(p.x);bounds.max_y=bounds.max_y.max(p.y);
    }
    let pad=(t.width_meters*3.).max(1.5);
    bounds.min_x-=pad;bounds.max_x+=pad;
    bounds.min_y-=pad;bounds.max_y+=pad;
    let dx=bounds.max_x-bounds.min_x;
    let dy=bounds.max_y-bounds.min_y;
    if !dx.is_finite()||!dy.is_finite()||dx<=0.||dy<=0.||dx.max(dy)>100_000. {
        return Err("Virtuele proefsleuf heeft ongeldige of zeer grote RD-grenzen".into());
    }
    let span=dx.max(dy).max(1.);
    let ppm=(span/(900.*quality)).clamp(0.02/quality,0.15/quality);
    let min=(220.*quality).round().max(2.) as u32;
    let max=(1600.*quality).round().max(min as f64) as u32;
    let w=(dx/ppm).ceil().clamp(min as f64,max as f64) as u32;
    let h=(dy/ppm).ceil().clamp(min as f64,max as f64) as u32;
    if u64::from(w)*u64::from(h)>20_000_000{
        return Err("Virtueel raster overschrijdt pixelbudget".into());
    }
    Ok(Layout{bounds,width:w,height:h})
}
fn pixel(info:Layout,x:f64,y:f64)->[f64;2] {
    [((x-info.bounds.min_x)/(info.bounds.max_x-info.bounds.min_x))*(info.width-1) as f64,
     ((info.bounds.max_y-y)/(info.bounds.max_y-info.bounds.min_y))*(info.height-1) as f64]
}
fn circle(image:&mut RgbaImage,x:i32,y:i32,r:i32,color:Rgba<u8>){
    let w=image.width() as i32;let h=image.height() as i32;
    for yy in (y-r).max(0)..=(y+r).min(h-1) {
        for xx in (x-r).max(0)..=(x+r).min(w-1) {
            if (xx-x)*(xx-x)+(yy-y)*(yy-y)<=r*r {
                image.put_pixel(xx as u32,yy as u32,color);
            }
        }
    }
}
fn stroke(image:&mut RgbaImage,a:[f64;2],b:[f64;2],thickness:i32,color:Rgba<u8>){
    if a.into_iter().chain(b).any(|v|!v.is_finite()){return;}
    let dx=b[0]-a[0];let dy=b[1]-a[1];
    let steps=dx.abs().max(dy.abs()).ceil() as usize;
    if steps>20_000 {return;}
    let radius=(thickness/2).clamp(1,16);
    for i in 0..=steps.max(1) {
        let ratio=i as f64/steps.max(1) as f64;
        let x=(a[0]+ratio*dx).round() as i32;
        let y=(a[1]+ratio*dy).round() as i32;
        circle(image,x,y,radius,color);
    }
}
pub fn render(source:&Source,cancel:&AtomicBool)->Result<RgbaImage,String>{
    input::check(cancel)?;
    let t=&source.trench;
    let info=layout(t,source.quality)?;
    let mut result=RgbaImage::new(info.width,info.height);
    let length=t.length();
    let nx=-(t.end.y-t.start.y)/length;
    let ny=(t.end.x-t.start.x)/length;
    let point_width=(t.width_meters*0.5).max(0.04);
    let border=t.measured_boundary();
    // Python-compatible dekband strokes run along the measured axis; even
    // bounds reversed by imports are clamped to the physical trench length.
    for (index,row) in t.dekband_rows.iter().enumerate() {
        if index%256==0 {input::check(cancel)?;}
        let a=row.start_chainage.clamp(0.,length);
        let b=row.end_chainage.clamp(0.,length);
        if b-a<=1e-6 {continue;}
        let point=|distance:f64|{
            let f=distance/length;
            pixel(info,t.start.x+f*(t.end.x-t.start.x),
                t.start.y+f*(t.end.y-t.start.y))
        };
        let color=row.display_rgb.unwrap_or([17,24,39]);
        stroke(&mut result,point(a),point(b),
            (4.*source.quality).round().max(3.) as i32,
            Rgba([color[0],color[1],color[2],235]));
    }
    // Keep markers and endpoints visibly separate from the source contour.
    for (i,object) in t.objects.iter().enumerate() {
        if i%256==0 {input::check(cancel)?;}
        let (_,_,chain,_) = t.project(object.position);
        let along=(chain/length).clamp(0.,1.);
        let cx=t.start.x+along*(t.end.x-t.start.x);
        let cy=t.start.y+along*(t.end.y-t.start.y);
        let a=pixel(info,cx+nx*point_width,cy+ny*point_width);
        let b=pixel(info,cx-nx*point_width,cy-ny*point_width);
        let name=object.role.to_lowercase();
        let color=if name.contains("gas"){Rgba([243,183,25,235])}
            else if name.contains("water"){Rgba([39,123,229,235])}
            else if name.contains("dekband"){Rgba([17,24,39,235])}
            else if name.contains("data")||name.contains("telecom"){Rgba([128,71,190,235])}
            else{Rgba([37,99,235,235])};
        stroke(&mut result,a,b,(2.5*source.quality).round().max(2.) as i32,color);
    }
    for (i,edge) in border.windows(2).enumerate(){
        if i%256==0 {input::check(cancel)?;}
        stroke(&mut result,pixel(info,edge[0].0,edge[0].1),
            pixel(info,edge[1].0,edge[1].1),
            (2.*source.quality).round().max(2.) as i32,Rgba([0,0,0,255]));
    }
    input::check(cancel)?;
    Ok(result)
}
#[cfg(test)]
mod tests{
    use super::*;
    use crate::model::Vertex;
    use crate::virtual_trench::TrenchPoint;
    fn trench()->VirtualTrench{
        VirtualTrench::new("Virtuele PS".into(),
            Vertex{x:155000.,y:463000.,z:Some(-1.)},
            Vertex{x:155010.,y:463000.,z:Some(-1.2)},0.6).unwrap()
    }
    #[test]
    fn python_geometry_size_and_rd_bounds(){
        let t=trench();let l=layout(&t,1.).unwrap();
        assert_eq!(l.width,650);
        assert_eq!(l.height,220);
        assert!((l.bounds.min_x-154998.5).abs()<1e-9);
        assert!((l.bounds.max_x-155011.5).abs()<1e-9);
        assert!((l.bounds.min_y-462998.2).abs()<1e-9);
    }
    #[test]
    fn render_preserves_transparency_and_marker_layers(){
        let mut t=trench();
        t.objects.push(TrenchPoint{role:"water".into(),object_name:"Water".into(),
            source_name:"Water".into(),position:Vertex{x:155005.,y:463000.1,z:Some(-1.2)},
            attributes:Default::default()});
        let img=render(&Source{trench:t,quality:1.},&AtomicBool::new(false)).unwrap();
        assert_eq!(img.get_pixel(0,0)[3],0);
        assert!(img.pixels().any(|p|p[0]==0&&p[3]==255));
        assert!(img.pixels().any(|p|p[2]==229&&p[3]==235));
    }
    #[test]
    fn dekband_segments_are_drawn_with_custom_color() {
        let mut t=trench();
        t.dekband_rows.push(crate::virtual_trench::Dekband {
            start_chainage:2.,end_chainage:7.,
            source_name:"Waarschuwingsband".into(),
            display_rgb:Some([239,33,44]),
        });
        let img=render(&Source{trench:t,quality:1.},&AtomicBool::new(false)).unwrap();
        assert!(img.pixels().any(|p|p.0==[239,33,44,235]));
    }
    #[test]
    fn cancel_and_invalid_scale_are_rejected(){
        let t=trench();
        assert!(layout(&t,0.).is_err());
        assert!(render(&Source{trench:t,quality:1.},&AtomicBool::new(true)).is_err());
    }
}
