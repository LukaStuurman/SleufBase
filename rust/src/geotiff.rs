//! First native GeoTIFF reader: spatial metadata, EPSG and KickTheMap sidecars.
//! Pixel rendering is a separate step; never label this full GeoTIFF parity.

use crate::geo::GeoTransform;
use crate::model::Bounds;
use serde_json::Value;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use tiff::decoder::Decoder;
use tiff::tags::Tag;

pub const MAX_GEOTIFF_PIXELS: u64 = 150_000_000;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct GeoTiffInfo {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub transform: GeoTransform,
    pub bounds: Bounds,
    pub epsg: Option<u32>,
    pub kickthemap_job_id: Option<u64>,
    pub kickthemap_job_title: String,
    pub kickthemap_job_sidecar: Option<PathBuf>,
}

pub fn from_tags(
    scale: &[f64],
    tiepoints: &[f64],
    transformation: &[f64]
) -> Result<GeoTransform, String> {
    // Transformation takes precedence, as in the Python GeoTIFF reader.
    if transformation.len() >= 16 {
        let m = transformation;
        let tx = GeoTransform {a:m[0],b:m[1],c:m[3],d:m[4],e:m[5],f:m[7]};
        if [tx.a,tx.b,tx.c,tx.d,tx.e,tx.f].iter().all(|x| x.is_finite()) {
            return Ok(tx);
        }
        return Err("GeoTIFF-transformatie bevat ongeldige getallen".into());
    }
    if scale.len() >= 2 && tiepoints.len() >= 6 {
        let (sx,sy)=(scale[0],scale[1]);
        let (px,py)=(tiepoints[0],tiepoints[1]);
        let (wx,wy)=(tiepoints[3],tiepoints[4]);
        let tx = GeoTransform {
            a:sx,b:0.0,c:wx-px*sx,
            d:0.0,e:-sy,f:wy+py*sy
        };
        if [tx.a,tx.c,tx.e,tx.f].iter().all(|x| x.is_finite()) {
            return Ok(tx);
        }
        return Err("GeoTIFF pixel scale/tiepoint is ongeldig".into());
    }
    Err("Geen GeoTIFF-transformatie gevonden".into())
}

pub fn parse_epsg(directory: &[u16], ascii: &str) -> Option<u32> {
    if directory.len()<4 { return None; }
    let count=usize::from(directory[3]);
    for key in directory[4..].chunks_exact(4).take(count) {
        let (kind,location,n,offset)=(key[0],key[1],usize::from(key[2]),usize::from(key[3]));
        if kind!=2048 && kind!=3072 { continue; }
        if location==0 { return (offset>0).then_some(offset as u32); }
        if location==34737 {
            let bytes=ascii.as_bytes();
            let end=offset.checked_add(n)?;
            let segment=bytes.get(offset..end)?;
            let text=std::str::from_utf8(segment).ok()?.trim_end_matches(['|','\0']);
            if let Ok(epsg)=text.parse::<u32>() { return Some(epsg); }
        }
    }
    None
}

pub fn job_sidecar(path: &Path) -> Option<(u64, String, PathBuf)> {
    let original_extension = path.extension()?.to_string_lossy();
    let sidecar_path=path.with_extension(format!("{original_extension}.job.json"));
    if fs::metadata(&sidecar_path).ok()?.len()>1_048_576 { return None; }
    let contents=fs::read_to_string(&sidecar_path).ok()?;
    let record:Value=serde_json::from_str(&contents).ok()?;
    let job_id = record.get("job_id").and_then(Value::as_u64)
        .or_else(|| record.get("job_id").and_then(Value::as_str)?.parse::<u64>().ok())?;
    if job_id==0 { return None; }
    let title=record.get("title").and_then(Value::as_str)
        .unwrap_or("").trim().to_owned();
    Some((job_id,title,sidecar_path))
}

pub fn read_geotiff_metadata(path: &Path) -> Result<GeoTiffInfo, String> {
    let file=File::open(path).map_err(|e|format!("TIFF niet toegankelijk: {e}"))?;
    let mut decoder=Decoder::new(BufReader::new(file))
        .map_err(|e|format!("TIFF ongeldig: {e}"))?;
    let (width,height)=decoder.dimensions().map_err(|e|e.to_string())?;
    let orientation=decoder.find_tag_unsigned::<u32>(Tag::Orientation).map_err(|e|e.to_string())?.unwrap_or(1);
    let (width,height)=crate::raster::oriented_dimensions(width,height,orientation)?;
    let pixels = u64::from(width)*u64::from(height);
    if pixels==0 || pixels>MAX_GEOTIFF_PIXELS {
        return Err(format!("TIFF te groot of leeg ({pixels} pixels)"));
    }
    let scale=decoder.get_tag_f64_vec(Tag::ModelPixelScaleTag).unwrap_or_default();
    let ties=decoder.get_tag_f64_vec(Tag::ModelTiepointTag).unwrap_or_default();
    let transform_matrix=decoder.get_tag_f64_vec(Tag::ModelTransformationTag).unwrap_or_default();
    let transform=from_tags(&scale,&ties,&transform_matrix)?;
    let bounds=transform.bounds(width,height)?;
    let keys=decoder.get_tag_u16_vec(Tag::GeoKeyDirectoryTag).unwrap_or_default();
    let ascii=decoder.get_tag_ascii_string(Tag::GeoAsciiParamsTag).unwrap_or_default();
    let epsg=parse_epsg(&keys,&ascii);
    let sidecar=job_sidecar(path);
    Ok(GeoTiffInfo {
        path:path.to_owned(),
        width,height,transform,bounds,epsg,
        kickthemap_job_id:sidecar.as_ref().map(|p|p.0),
        kickthemap_job_title:sidecar.as_ref().map(|p|p.1.clone()).unwrap_or_default(),
        kickthemap_job_sidecar:sidecar.map(|p|p.2),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_pixel_scale_and_tiepoint() {
        let tx=from_tags(&[0.05,0.1,0.0], &[100.0,20.0,0.0,168000.0,401000.0,0.0], &[]).unwrap();
        assert!((tx.c-167995.0).abs()<1e-8);
        assert!((tx.f-401002.0).abs()<1e-8);
    }
    #[test]
    fn transformation_matrix_precedence() {
        let matrix=[2.,0.,0.,100.,0.,-3.,0.,200.,0.,0.,1.,0.,0.,0.,0.,1.];
        let tx=from_tags(&[1.,1.], &[0.,0.,0.,0.,0.,0.], &matrix).unwrap();
        assert_eq!(tx.pixel_to_world(2.,3.), (104.,191.));
    }
    #[test]
    fn epsg_geokey_projected() {
        assert_eq!(parse_epsg(&[1,1,0,1,3072,0,1,28992], ""), Some(28992));
        assert_eq!(parse_epsg(&[1,1,0,1,2048,0,1,4326], ""), Some(4326));
        assert_eq!(parse_epsg(&[], ""), None);
    }
    #[test]
    fn sidecar_positive_id_only() {
        let dir=std::env::temp_dir().join(format!("sleufbase-geotiff-test-{}",std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path=dir.join("voorbeeld.tiff");
        let sidecar=dir.join("voorbeeld.tiff.job.json");
        fs::write(&sidecar,r#"{"job_id":"12345","title":"Example trench"}"#).unwrap();
        let read=job_sidecar(&path).unwrap();
        assert_eq!(read.0,12345);
        assert_eq!(read.1,"Example trench");
        fs::write(&sidecar,r#"{"job_id":"-1"}"#).unwrap();
        assert!(job_sidecar(&path).is_none());
        fs::remove_file(sidecar).unwrap();
    }
}