//! Image attachments for native multi-slot template exports.
//! Never use original JPEG/PNG bytes as DXF IMAGEDEF: decode with explicit
//! memory bounds, normalize to bounded PNG and preserve the transactional
//! publication of the existing template exporter.
use crate::{export_control::Control,model::Bounds};
use image::{DynamicImage,ImageReader,imageops::FilterType};
use std::path::{Path,PathBuf};

pub const MAX_PHOTOS_PER_SLOT:usize=4;
const MAX_SOURCE_BYTES:u64=64*1024*1024;
const MAX_PIXELS:u64=24_000_000;

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq,serde::Serialize,serde::Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Area {#[default] Tiff,Map}
#[derive(Clone,Debug,serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Photo {
    pub path:PathBuf,
    #[serde(default)]
    pub turns:u8,
    #[serde(default)]
    pub area:Area,
}
pub fn validate(photos:&[Photo])->Result<(),String>{
    if photos.len()>MAX_PHOTOS_PER_SLOT {
        return Err(format!("Maximaal {MAX_PHOTOS_PER_SLOT} foto's per proefsleufvak"));
    }
    for photo in photos {
        if photo.turns>3 {return Err("Foto rotatie moet 0, 90, 180 of 270 graden zijn".into());}
        if !photo.path.is_file(){return Err(format!("Foto ontbreekt: {}",photo.path.display()));}
        let size=std::fs::metadata(&photo.path).map_err(|e|e.to_string())?.len();
        if size==0 || size>MAX_SOURCE_BYTES{
            return Err(format!("Foto is leeg of groter dan 64 MB: {}",photo.path.display()));
        }
        let ext=photo.path.extension().and_then(|e|e.to_str())
            .map(str::to_ascii_lowercase).unwrap_or_default();
        if !matches!(ext.as_str(),"png"|"jpg"|"jpeg"){
            return Err("Kies alleen JPEG- en PNG-foto's".into());
        }
    }
    Ok(())
}
/// Right-side photo strip inside the selected box. Keeps the original map
/// and TIFF assets present: photos are separate overlaid DXF IMAGE entities.
pub fn boxes(tiff:Bounds,map:Bounds,photos:&[Photo])->Result<Vec<Bounds>,String>{
    let valid=|b:Bounds|{
        [b.min_x,b.min_y,b.max_x,b.max_y].into_iter().all(f64::is_finite)
          && b.max_x>b.min_x && b.max_y>b.min_y
    };
    if !valid(tiff)||!valid(map){return Err("Ongeldige foto-/kaartgrenzen".into());}
    if photos.len()>MAX_PHOTOS_PER_SLOT{return Err("Te veel foto's in dit vak".into());}
    let mut result=Vec::with_capacity(photos.len());
    for (area,base) in [(Area::Tiff,tiff),(Area::Map,map)]{
        let indices=photos.iter().enumerate().filter(|(_,p)|p.area==area)
            .map(|(i,_)|i).collect::<Vec<_>>();
        let n=indices.len();
        if n==0 {continue;}
        let width=base.max_x-base.min_x;
        let height=base.max_y-base.min_y;
        // photos cover the right 36% with 2% safe margins
        let x1=base.max_x-width*0.38;
        let x2=base.max_x-width*0.02;
        let pad=height*0.025;
        let usable=height-2.*pad;
        let step=usable/n as f64;
        if x2<=x1 || step<=2.*pad {return Err("Kaartvak te klein voor foto's".into());}
        for (order,&i) in indices.iter().enumerate(){
            let top=base.max_y-pad-order as f64*step;
            let bottom=top-step+pad;
            result.push((i,Bounds{min_x:x1,min_y:bottom,max_x:x2,max_y:top}));
        }
    }
    result.sort_by_key(|(index,_)|*index);
    Ok(result.into_iter().map(|(_,b)|b).collect())
}
pub fn prepare(photo:&Photo,dest:&Path,control:&Control)->Result<u64,String>{
    control.check()?;
    validate(std::slice::from_ref(photo))?;
    let mut reader=ImageReader::open(&photo.path).map_err(|e|e.to_string())?
        .with_guessed_format().map_err(|e|e.to_string())?;
    if !matches!(reader.format(),Some(image::ImageFormat::Png|image::ImageFormat::Jpeg)){
        return Err("Fotobestand heeft geen geldige PNG/JPEG-structuur".into());
    }
    let (w,h)=image::image_dimensions(&photo.path).map_err(|e|e.to_string())?;
    if w==0 || h==0 || u64::from(w)*u64::from(h)>MAX_PIXELS{
        return Err("Foto heeft te veel pixels of ongeldige afmetingen".into());
    }
    let mut limits=image::Limits::default();
    limits.max_alloc=Some(128*1024*1024);
    reader.limits(limits);
    let mut decoded=reader.decode().map_err(|e|format!("Foto kon niet gelezen worden: {e}"))?;
    control.check()?;
    decoded=match photo.turns {
        1=>decoded.rotate90(),2=>decoded.rotate180(),3=>decoded.rotate270(),_=>decoded,
    };
    let longest=decoded.width().max(decoded.height());
    if longest>1800 {
        let scale=1800./longest as f64;
        decoded=decoded.resize(
            (decoded.width() as f64*scale).round().max(1.) as u32,
            (decoded.height() as f64*scale).round().max(1.) as u32,
            FilterType::Lanczos3
        );
    }
    control.check()?;
    let rgba=DynamicImage::ImageRgba8(decoded.into_rgba8()).into_rgba8();
    crate::template_raster::save_image(&rgba,dest,control.flag())?;
    let size=std::fs::metadata(dest).map_err(|e|e.to_string())?.len();
    if size>64*1024*1024{return Err("Genormaliseerde PNG is te groot".into());}
    control.check()?;
    Ok(size)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_does_not_escape_source_boxes_and_keeps_per_area_order() {
        let a=Bounds{min_x:0.,min_y:0.,max_x:10.,max_y:20.};
        let b=Bounds{min_x:20.,min_y:10.,max_x:40.,max_y:30.};
        let photos=[Photo{path:"x".into(),turns:0,area:Area::Map},
            Photo{path:"y".into(),turns:0,area:Area::Tiff},
            Photo{path:"z".into(),turns:0,area:Area::Map}];
        let result=boxes(a,b,&photos).unwrap();
        assert_eq!(result.len(),3);
        assert!(result[0].min_x>20. && result[0].max_x<40.);
        assert!(result[1].min_x>0. && result[1].max_x<10.);
        assert!(result[0].min_y>result[2].min_y);
    }
    #[test]
    fn jpeg_png_rotation_and_invalid_input_are_handled_without_publish() {
        let dir=tempfile::tempdir().unwrap();
        let source=dir.path().join("source.png");
        let mut img=image::RgbaImage::from_pixel(3,2,image::Rgba([7,8,9,255]));
        img.put_pixel(0,0,image::Rgba([200,0,0,255]));
        img.save(&source).unwrap();
        let photo=Photo{path:source,turns:1,area:Area::Tiff};
        let output=dir.path().join("photo.png");
        prepare(&photo,&output,&Control::default()).unwrap();
        let pixels=image::open(&output).unwrap().to_rgba8();
        assert_eq!(pixels.dimensions(),(2,3));
        assert_eq!(pixels.get_pixel(1,0).0,[200,0,0,255]);
        assert!(validate(&[Photo{turns:4,..photo.clone()}]).is_err());
        let invalid=Photo{path:dir.path().join("missing.jpeg"),..photo};
        assert!(prepare(&invalid,&dir.path().join("bad.png"),&Control::default()).is_err());
        assert!(!dir.path().join("bad.png").exists());
    }
}
