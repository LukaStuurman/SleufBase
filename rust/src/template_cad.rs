//! Native CAD map overlays for template sheets.
//! Uses the same flattened INSERT / MINSERT scene and visibility rules as
//! the Rust canvas; clips line segments before allocating map stroke buffers.
use crate::{
    dxf_read::DxfOverlay,
    export_control::Control,
    map_export::Stroke,
    model::Bounds,
};
const MAX_CAD_STROKES: usize = 30_000;
const MAX_CAD_VERTICES: usize = 120_000;

fn valid(b: Bounds) -> bool {
    [b.min_x,b.min_y,b.max_x,b.max_y].iter().all(|x|x.is_finite())
        && b.min_x<b.max_x && b.min_y<b.max_y
}
/// Clip a segment without losing paths that cross a map with both endpoints
/// outside it (the old vertex-only test silently dropped those paths).
pub(crate) fn clip(a: [f64;2], b: [f64;2], bounds: Bounds) -> Option<([f64;2],[f64;2])> {
    if !valid(bounds) || !a.into_iter().chain(b).all(|x|x.is_finite()) {return None;}
    let d=[b[0]-a[0],b[1]-a[1]];
    let p=[-d[0],d[0],-d[1],d[1]];
    let q=[a[0]-bounds.min_x,bounds.max_x-a[0],a[1]-bounds.min_y,bounds.max_y-a[1]];
    let mut entry: f64=0.;
    let mut exit: f64=1.;
    for k in 0..4 {
        if p[k].abs()<=1e-14 {
            if q[k]<0. {return None;}
        } else {
            let t=q[k]/p[k];
            if p[k]<0. {entry=entry.max(t);} else {exit=exit.min(t);}
            if entry>exit {return None;}
        }
    }
    let clamp=|x:f64,lo:f64,hi:f64|x.max(lo).min(hi);
    let start=[clamp(a[0]+entry*d[0],bounds.min_x,bounds.max_x),
               clamp(a[1]+entry*d[1],bounds.min_y,bounds.max_y)];
    let end=[clamp(a[0]+exit*d[0],bounds.min_x,bounds.max_x),
             clamp(a[1]+exit*d[1],bounds.min_y,bounds.max_y)];
    (start.iter().chain(end.iter()).all(|x|x.is_finite()) && start!=end)
        .then_some((start,end))
}
pub fn append_visible(
    layers: &[DxfOverlay], bounds: Bounds, output: &mut Vec<Stroke>,
    control: &Control,
) -> Result<usize,String> {
    control.check()?;
    if !valid(bounds) {return Err("Ongeldige CAD-kaartuitsnede".into());}
    let original=output.len();
    let mut count=0usize;
    let mut scanned=0usize;
    for layer in layers.iter().filter(|l|l.visible) {
        for feature in layer.rendered_features() {
            if !feature.visible {continue;}
            for (path, rgb) in feature.painted_paths() {
                for edge in path.windows(2) {
                    scanned=scanned.checked_add(2).ok_or("CAD-puntenbudget overschreden")?;
                    if scanned%4096==0 {control.check()?;}
                    if scanned>5_000_000 {return Err("Te veel CAD-punten voor sjabloonkaarten".into());}
                    let a=[edge[0].x,edge[0].y];
                    let b=[edge[1].x,edge[1].y];
                    if !a.iter().chain(b.iter()).all(|v|v.is_finite()) {
                        return Err("CAD-kaart bevat niet-eindige coördinaten".into());
                    }
                    if let Some((start,end))=clip(a,b,bounds) {
                        count+=1;
                        if count>MAX_CAD_STROKES || count*2>MAX_CAD_VERTICES {
                            return Err("CAD-kaart bevat te veel zichtbare lijnsegmenten".into());
                        }
                        output.push(Stroke {
                            points:vec![start,end],
                            color:rgb,
                            selected:false,
                            highlighted:false,
                        });
                    }
                }
            }
        }
    }
    Ok(output.len()-original)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dxf_read::{DxfFeature, DxfOverlay};
    use crate::model::Vertex;
    use std::path::PathBuf;
    const B:Bounds=Bounds{min_x:0.,min_y:0.,max_x:10.,max_y:10.};
    #[test]
    fn segment_crossing_viewport_and_tangents_are_clipped() {
        assert_eq!(clip([-20.,5.],[20.,5.],B),Some(([0.,5.],[10.,5.])));
        assert_eq!(clip([20.,5.],[-20.,5.],B),Some(([10.,5.],[0.,5.])));
        assert!(clip([-20.,12.],[20.,12.],B).is_none());
        assert!(clip([f64::NAN,0.],[4.,4.],B).is_none());
    }
    #[test]
    fn flattening_respects_hidden_overlay_and_preserves_color() {
        let feature=DxfFeature {
            entity_type:"LINE".into(),handle:"FF".into(),layer:"Layer1".into(),
            color_aci:1,points:vec![
                Vertex{x:-5.,y:5.,z:None},
                Vertex{x:15.,y:5.,z:None},
            ],raw_text:String::new(),block_name:String::new(),closed:None,cad:None,
        };
        let mut overlay=DxfOverlay::default();
        overlay.path=PathBuf::from("layer.dxf");
        overlay.visible=true;
        overlay.features.push(feature);
        let mut out=Vec::new();
        assert_eq!(append_visible(&[overlay.clone()],B,&mut out,&Control::default()).unwrap(),1);
        assert_eq!(out[0].points,vec![[0.,5.],[10.,5.]]);
        assert_eq!(out[0].color,overlay.feature_rgb(&overlay.features[0]));
        overlay.visible=false;
        assert_eq!(append_visible(&[overlay],B,&mut out,&Control::default()).unwrap(),0);
        assert_eq!(out.len(),1);
    }
    #[test]
    fn cancellation_is_honored_before_processing() {
        let control=Control::default();
        control.cancel();
        let mut output=Vec::new();
        assert!(append_visible(&[],B,&mut output,&control).is_err());
    }
}
