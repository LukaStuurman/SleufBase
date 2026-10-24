//! Synthetic high-resolution raster for virtual/MarXact trenches without an
//! external KickTheMap GeoTIFF. Follows Python virtual_trench_render_geometry:
//! RD bounds with 1.5 m margin, 220..1600 pixels/axis, transparent background,
//! outlines and transverse object markers. No fake satellite imagery is used.
use crate::{model::Bounds, virtual_trench::VirtualTrench, input};
use image::RgbaImage;
use serde::Deserialize;
use std::sync::atomic::AtomicBool;
#[path="virtual_draw.rs"]
mod draw;

#[derive(Clone,Debug,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub trench:VirtualTrench,
    #[serde(default="default_quality")]
    pub quality:f64,
    #[serde(default)]
    pub reverse:bool,
}
fn default_quality()->f64{1.}
#[derive(Clone,Copy,Debug)]
pub struct Layout {pub bounds:Bounds,pub width:u32,pub height:u32}
impl Layout {
    pub fn transform(self)->crate::geo::GeoTransform {
        crate::geo::GeoTransform {
            a:(self.bounds.max_x-self.bounds.min_x)/self.width as f64,
            b:0.,c:self.bounds.min_x,
            d:0.,e:-(self.bounds.max_y-self.bounds.min_y)/self.height as f64,
            f:self.bounds.max_y,
        }
    }
}
pub fn layout(t:&VirtualTrench,quality:f64)->Result<Layout,String>{
    t.validate()?;
    if !quality.is_finite() || !(0.1..=6.).contains(&quality) {
        return Err("Virtuele rasterkwaliteit moet tussen 0,1 en 6 liggen".into());
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
    let min=(220.*quality).round_ties_even().max(2.) as u32;
    let max=(1600.*quality).round_ties_even().max(min as f64) as u32;
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
/// Match Python's virtual-template pixel alignment: render in RD space and
/// then rotate/crop through the same full-resolution engine as GeoTIFF sheets.
/// Reverse swaps measured orientation before rotating, so profile and image
/// follow the same start/end direction.
pub fn prepare_for_template(source:&Source,cancel:&AtomicBool)
    ->Result<crate::template_raster::Prepared,String>
{
    prepare_for_template_direction(source,source.reverse,cancel)
}
pub fn prepare_for_template_direction(source:&Source,reverse:bool,cancel:&AtomicBool)
    ->Result<crate::template_raster::Prepared,String>
{
    input::check(cancel)?;
    let info=layout(&source.trench,source.quality)?;
    let image=render(source,cancel)?;
    let transform=info.transform();
    let start=[source.trench.start.x,source.trench.start.y];
    let end=[source.trench.end.x,source.trench.end.y];
    crate::template_raster::prepare(
        image,
        &crate::template_raster::Options {
            transform,
            roads:&[],
            terrain:&[],
            axis:Some(if reverse {[end,start]}else{[start,end]}),
            reverse,
        },
        cancel,
    )
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
    let border_width=(2.*source.quality).round_ties_even().max(2.) as u32;
    let dekband_width=(4.*source.quality).round_ties_even().max((border_width+1) as f64) as u32;
    let point_width_px=(2.5*source.quality).round_ties_even().max(2.) as u32;
    let mut budget=300_000_000u64;
    // Python-compatible dekband strokes run along the measured axis; even
    // bounds reversed by imports are clamped to the physical trench length.
    let mut bands=t.dekband_rows.iter().collect::<Vec<_>>();
    // The raster normalizes/clips before its stable start/end sort, while
    // profile injection deliberately retains declaration order for labels.
    bands.sort_by(|a,b|a.start_chainage.clamp(0.,length).partial_cmp(&b.start_chainage.clamp(0.,length)).unwrap_or(std::cmp::Ordering::Equal)
        .then_with(||a.end_chainage.clamp(0.,length).partial_cmp(&b.end_chainage.clamp(0.,length)).unwrap_or(std::cmp::Ordering::Equal)));
    for (index,row) in bands.into_iter().enumerate() {
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
        draw::line(&mut result,&[point(a),point(b)],dekband_width,
            [color[0],color[1],color[2],235],cancel,&mut budget)?;
    }
    // The original stable chainage order determines overlapping marker colors.
    let mut objects=t.objects.iter().collect::<Vec<_>>();
    let chainage=|p:&crate::virtual_trench::TrenchPoint|((p.position.x-t.start.x)*(t.end.x-t.start.x)+(p.position.y-t.start.y)*(t.end.y-t.start.y))/length;
    objects.sort_by(|a,b|chainage(a).total_cmp(&chainage(b)));
    for (i,object) in objects.into_iter().enumerate() {
        if i%256==0 {input::check(cancel)?;}
        let (cx,cy,_,_) = t.project(object.position);
        if matches!(object.role.to_lowercase().as_str(),"start"|"end"){continue;}
        let preferred=(object.position.x-cx)*nx+(object.position.y-cy)*ny;
        let ((ax,ay),(bx,by))=crate::cross_section::local_segment(&border,(cx,cy),(nx,ny),preferred)
            .unwrap_or(((cx+nx*point_width,cy+ny*point_width),(cx-nx*point_width,cy-ny*point_width)));
        let a=pixel(info,ax,ay);let b=pixel(info,bx,by);
        let color=display_rgb(&object.metadata).unwrap_or([37,99,235]);
        draw::line(&mut result,&[a,b],point_width_px,[color[0],color[1],color[2],235],cancel,&mut budget)?;
    }
    let polygon=border.iter().map(|&(x,y)|pixel(info,x,y)).collect::<Vec<_>>();
    draw::line(&mut result,&polygon,border_width,[0,0,0,255],cancel,&mut budget)?;
    if polygon.len()>=3 {
        let mut mask=vec![0u8;info.width as usize*info.height as usize];
        draw::polygon(&polygon,[info.width,info.height],cancel,&mut budget,|y,a,b|{
            mask[y as usize*info.width as usize+a as usize..=y as usize*info.width as usize+b as usize].fill(255);
        })?;
        for (i,(p,m)) in result.pixels_mut().zip(mask).enumerate(){
            if i%65536==0{input::check(cancel)?;}
            p[3]=((u16::from(p[3])*u16::from(m))/255) as u8;
        }
        draw::line(&mut result,&polygon,border_width,[0,0,0,255],cancel,&mut budget)?;
    }
    input::check(cancel)?;
    Ok(result)
}
/// Original point color decoder: three int-convertible channels, clamped to
/// 0..255; malformed values fall back as one complete color.
pub fn display_rgb(metadata:&serde_json::Value)->Option<[u8;3]>{
    let values=metadata.get("display_rgb")?.as_array()?;
    if values.len()<3{return None;}
    let mut result=[0;3];
    for (out,value) in result.iter_mut().zip(values){
        let n=match value{
            serde_json::Value::Bool(v)=>if *v{1.}else{0.},
            serde_json::Value::Number(v)=>v.as_f64()?,
            serde_json::Value::String(v)=>{
                let v=crate::python_values::trim(v);
                let translated=v.chars().map(|c|crate::object_layout::decimal(c).map_or(c,|d|char::from(b'0'+d as u8))).collect::<String>();
                let mut digits=0usize;let mut previous_digit=false;
                for (i,c) in translated.chars().enumerate(){
                    if c.is_ascii_digit(){digits+=1;previous_digit=true;}
                    else if (c=='+'||c=='-')&&i==0{previous_digit=false;}
                    else if c=='_'&&previous_digit{previous_digit=false;}
                    else{return None;}
                }
                if digits==0||!previous_digit{return None;}
                // Python integers do not overflow; only their clamped result
                // matters. Huge finite digit strings therefore saturate too.
                let parsed=translated.replace('_',"").parse::<f64>().ok()?;
                if parsed.is_infinite(){if parsed.is_sign_negative(){0.}else{255.}}else{parsed}
            },
            _=>return None,
        };
        if !n.is_finite(){return None;}*out=n.clamp(0.,255.) as u8;
    }
    Some(result)
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
        assert_eq!(l.width,680);
        assert_eq!(l.height,220);
        assert!((l.bounds.min_x-154998.2).abs()<1e-9);
        assert!((l.bounds.max_x-155011.8).abs()<1e-9);
        assert!((l.bounds.min_y-462997.9).abs()<1e-9);
    }
    #[test]
    fn render_preserves_transparency_and_marker_layers(){
        let mut t=trench();
        t.objects.push(TrenchPoint{metadata:serde_json::json!({"display_rgb":[39,123,229]}),role:"water".into(),object_name:"Water".into(),
            source_name:"Water".into(),position:Vertex{x:155005.,y:463000.1,z:Some(-1.2)},
            attributes:Default::default()});
        let img=render(&Source{trench:t,quality:1.,reverse:false},&AtomicBool::new(false)).unwrap();
        assert_eq!(img.get_pixel(0,0)[3],0);
        assert!(img.pixels().any(|p|p[0]==0&&p[3]==255));
        assert!(img.pixels().any(|p|p[2]==229&&p[3]==235));
    }
    #[test]
    fn dekband_segments_are_drawn_with_custom_color() {
        let mut t=trench();
        t.dekband_rows.push(crate::virtual_trench::Dekband {start_depth:0.,end_depth:0.,
            start_chainage:2.,end_chainage:7.,
            source_name:"Waarschuwingsband".into(),
            display_rgb:Some([239,33,44]),
        });
        let img=render(&Source{trench:t,quality:1.,reverse:false},&AtomicBool::new(false)).unwrap();
        assert!(img.pixels().any(|p|p.0==[239,33,44,235]));
    }
    #[test]
    fn template_preparation_aligns_vertical_measurement_axis_and_reversal(){
        let mut t=trench();
        t.end.x=t.start.x;
        t.end.y=t.start.y+10.;
        let forward=prepare_for_template(
            &Source{trench:t.clone(),quality:1.,reverse:false},
            &AtomicBool::new(false)).unwrap();
        let reverse=prepare_for_template(
            &Source{trench:t,quality:1.,reverse:true},
            &AtomicBool::new(false)).unwrap();
        assert_eq!(forward.image.dimensions(),reverse.image.dimensions());
        assert!(forward.details.rotation.abs()>1.);
        assert!((forward.details.rotation-reverse.details.rotation).abs()>90.);
    }
    #[test]
    fn cancel_and_invalid_scale_are_rejected(){
        let t=trench();
        assert!(layout(&t,0.).is_err());
        assert!(render(&Source{trench:t,quality:1.,reverse:false},&AtomicBool::new(true)).is_err());
    }
}
