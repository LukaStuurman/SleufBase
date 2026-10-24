//! Unframed Gemma map input, preserving selected TIFF/virtual pixels and CAD.
//! Map bounds and 1400x1000 pixels are independent of A4 export settings.
use crate::{ai_maaiveld::{self,Point},map_export::{Raster,Stroke},Bounds};
use image::{RgbaImage,RgbImage,Rgba};
use std::sync::atomic::AtomicBool;
/// Original Gemma uses the configured map-export provider, independently of
/// the interactive background checkbox. The removed legacy provider migrates
/// to PDOK, as it does throughout the native application.
pub fn background_provider(setting:&str)->crate::project_view::BackgroundProvider{
    use crate::project_view::BackgroundProvider as P;
    match P::from_label(setting){P::OpenStreetMap=>P::OpenStreetMap,P::Cadastral=>P::Cadastral,_=>P::PdokAerial}
}
pub fn fetch_background(setting:&str,bounds:Bounds,size:[u32;2],cancel:&AtomicBool)->Result<RgbaImage,String>{
    let setting=setting.to_owned();
    crate::cadastral::cancellable_request(cancel,move|stop|fetch_background_blocking(&setting,bounds,size,&stop))?
}
fn fetch_background_blocking(setting:&str,bounds:Bounds,size:[u32;2],cancel:&AtomicBool)->Result<RgbaImage,String>{
    use crate::project_view::BackgroundProvider as P;
    crate::input::check(cancel)?;
    match background_provider(setting){
        P::OpenStreetMap=>{let cache=crate::project::app_data_directory().join("osm-tiles");let raster=crate::osm_tiles::request_export_map_with_cancel(bounds,size[0],size[1],&cache,cancel)?;RgbaImage::from_raw(raster.width,raster.height,raster.rgba).ok_or("Ongeldige OSM-achtergrondpixels".into())},
        P::Cadastral=>crate::cadastral_tiles::Client::new(crate::project::app_data_directory().join("cadastral-tiles"))?.request(bounds,size,cancel),
        _=>crate::pdok_wms::request_tiled(bounds,size[0],size[1],cancel),
    }
}
#[allow(clippy::too_many_arguments)]
pub fn render(layer_bounds:Bounds,points:&[Point],size:[u32;2],raster:Option<Raster<'_>>,overlays:&[Stroke],background:Option<&RgbaImage>,opacity:f64,cancel:&AtomicBool)->Result<RgbImage,String>{
    let bounds=ai_maaiveld::numbered_layout(layer_bounds,points,size)?;
    let base=render_base(bounds,size,raster,overlays,background,opacity,cancel)?;
    ai_maaiveld::build_numbered_image(&base,bounds,points,cancel)
}
#[allow(clippy::too_many_arguments)]
pub fn render_base(bounds:Bounds,size:[u32;2],raster:Option<Raster<'_>>,overlays:&[Stroke],background:Option<&RgbaImage>,opacity:f64,cancel:&AtomicBool)->Result<RgbaImage,String>{
    crate::input::check(cancel)?;
    if size.contains(&0)||u64::from(size[0])*u64::from(size[1])>20_000_000{return Err("Ongeldig Gemma-kaartformaat".into());}
    if !opacity.is_finite()||!(0.0..=1.).contains(&opacity)||!bounds.min_x.is_finite()||!bounds.min_y.is_finite()||!bounds.max_x.is_finite()||!bounds.max_y.is_finite()||bounds.max_x<=bounds.min_x||bounds.max_y<=bounds.min_y{return Err("Ongeldige Gemma-kaartuitsnede".into());}
    let layout=crate::map_page::Layout{page_size:size,map_size:size,origin:[0.,0.],bounds,meters_per_pixel:(bounds.max_x-bounds.min_x)/size[0]as f64};
    let mut image=if let Some(background)=background{if background.dimensions()!=(size[0],size[1]){return Err("Gemma-achtergrond past niet bij de kaartuitsnede".into());}background.clone()}else{RgbaImage::from_pixel(size[0],size[1],Rgba([255,255,255,255]))};
    if let Some(raster)=raster{
        let(pixels,transform)=match raster{
            Raster::Tiff(source)=>{let raw=crate::raster::decode_full_with_cancel(&source.path,cancel)?;let image=RgbaImage::from_raw(raw.width,raw.height,raw.rgba).ok_or("Ongeldige Gemma TIFF-pixels")?;(image,source.transform)},
            Raster::Virtual(source)=>{let info=crate::virtual_raster::layout(&source.trench,source.quality)?;(crate::virtual_raster::render(source,cancel)?,info.transform())},
        };
        crate::map_raster::paint_scene(&mut image,pixels,transform,&layout,opacity,cancel)?;
    }
    if overlays.len()>200_000||overlays.iter().map(|s|s.points.len()).sum::<usize>()>2_000_000{return Err("Te veel Gemma-kaartpunten".into());}
    let width=if layout.meters_per_pixel<0.05{4}else if layout.meters_per_pixel<0.2{3}else{2};let mut budget=200_000_000;
    for stroke in overlays{
        crate::input::check(cancel)?;
        if stroke.points.iter().flatten().any(|v|!v.is_finite()||v.abs()>1e12){return Err("Ongeldige Gemma CAD-coördinaten".into());}
        let points=stroke.points.iter().map(|p|layout.screen(*p)).collect::<Vec<_>>();let color=[stroke.color[0],stroke.color[1],stroke.color[2],230];
        crate::map_canvas::line(&mut image,&points,width,color,cancel,&mut budget)?;
    }
    Ok(image)
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn gemma_uses_export_provider_and_cancellation_never_fetches(){
        use crate::project_view::BackgroundProvider as P;
        assert_eq!(background_provider("osm"),P::OpenStreetMap);assert_eq!(background_provider("OpenStreetMap"),P::OpenStreetMap);assert_eq!(background_provider("cadastral"),P::Cadastral);assert_eq!(background_provider("PDOK kadastrale kaart"),P::Cadastral);assert_eq!(background_provider("pdok"),P::PdokAerial);assert_eq!(background_provider("unknown legacy label"),P::PdokAerial);
        for provider in ["osm","cadastral","pdok"]{assert!(fetch_background(provider,Bounds{min_x:0.,min_y:0.,max_x:1.,max_y:1.},[1400,1000],&AtomicBool::new(true)).is_err());}
    }
    #[test]fn unframed_scene_preserves_white_pixels_and_real_cad_coordinates(){
        let cancel=AtomicBool::new(false);let bounds=Bounds{min_x:0.,min_y:0.,max_x:10.,max_y:5.};
        let layout=crate::map_page::Layout{page_size:[100,50],map_size:[100,50],origin:[0.,0.],bounds,meters_per_pixel:0.1};
        let transform=crate::geo::GeoTransform{a:0.1,b:0.,c:0.,d:0.,e:-0.1,f:5.};
        let white=RgbaImage::from_pixel(100,50,Rgba([255;4]));let mut original=RgbaImage::from_pixel(100,50,Rgba([20,30,40,255]));let mut masked=original.clone();
        crate::map_raster::paint_scene(&mut original,white.clone(),transform,&layout,1.,&cancel).unwrap();crate::map_raster::paint(&mut masked,white,transform,&layout,&cancel).unwrap();
        assert_eq!(original.get_pixel(50,25).0,[255;4]);assert_eq!(masked.get_pixel(50,25).0,[20,30,40,255]);
        let stroke=Stroke{points:vec![[1.,2.5],[9.,2.5]],color:[201,11,51],selected:false,highlighted:false};
        let output=render_base(bounds,[100,50],None,&[stroke],Some(&masked),1.,&cancel).unwrap();assert_eq!(output.dimensions(),(100,50));assert_eq!(output.get_pixel(50,25).0,[201,11,51,230]);assert_eq!(output.get_pixel(50,0).0,[20,30,40,255]);
        assert!(render_base(bounds,[100,50],None,&[],Some(&masked),1.,&AtomicBool::new(true)).is_err());
    }
}
