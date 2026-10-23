//! Pillow 11.3 Draw.c wide-line / polygon scan conversion for virtual rasters.
//! Adapted from Fredrik Lundh / Secret Labs AB's polygon_generic and
//! ImagingDrawWideLine; see third_party/Pillow-LICENSE.txt. Keep f32 edge
//! arithmetic and the distinct half-tie rounding rules: border masks depend on
//! their exact inclusive pixels. No per-object image/mask allocations.
use image::{Rgba,RgbaImage};
use std::sync::atomic::AtomicBool;
#[derive(Clone,Copy)]
struct Edge{x:i32,y:i32,xmin:i32,xmax:i32,ymin:i32,ymax:i32,dx:f32}
impl Edge{
    fn new(a:[i32;2],b:[i32;2])->Self{Self{x:a[0],y:a[1],xmin:a[0].min(b[0]),xmax:a[0].max(b[0]),ymin:a[1].min(b[1]),ymax:a[1].max(b[1]),dx:if a[1]==b[1]{0.}else{(b[0]-a[0])as f32/(b[1]-a[1])as f32}}}
    fn at(self,y:i32)->f32{(y-self.y)as f32*self.dx+self.x as f32}
}
fn up(v:f32)->i32{if v>=0.{(v+0.5).floor()as i32}else{-(-v+0.5).floor()as i32}}
fn down(v:f32)->i32{if v>=0.{(v-0.5).ceil()as i32}else{-(-v-0.5).ceil()as i32}}
fn up64(v:f64)->i32{if v>=0.{(v+0.5).floor()as i32}else{-(-v+0.5).floor()as i32}}
fn down64(v:f64)->i32{if v>=0.{(v-0.5).ceil()as i32}else{-(-v-0.5).ceil()as i32}}
fn debit(budget:&mut u64,cost:u64)->Result<(),String>{*budget=budget.checked_sub(cost).ok_or("Virtueel raster overschrijdt het tekenbudget")?;Ok(())}
pub(super) fn polygon(points:&[[f64;2]],size:[u32;2],cancel:&AtomicBool,budget:&mut u64,mut span:impl FnMut(u32,u32,u32))->Result<(),String>{
    crate::input::check(cancel)?;
    if points.len()<2{return Ok(());}
    let p=points.iter().map(|p|[p[0]as i32,p[1]as i32]).collect::<Vec<_>>();
    let mut edges=Vec::with_capacity(p.len());
    for pair in p.windows(2){edges.push(Edge::new(pair[0],pair[1]));}
    if p.last()!=p.first(){edges.push(Edge::new(*p.last().unwrap(),p[0]));}
    let ymin=edges.iter().map(|e|e.ymin).min().unwrap_or(0).max(0);
    let ymax=edges.iter().map(|e|e.ymax).max().unwrap_or(0).min(size[1]as i32);
    let mut hline=|left:i32,y:i32,right:i32,budget:&mut u64|->Result<(),String>{
        if y<0||y>=size[1]as i32{return Ok(());}
        let a=left.max(0);let b=right.min(size[0]as i32-1);
        if b>=a{debit(budget,(b-a+1)as u64)?;span(y as u32,a as u32,b as u32);}
        Ok(())
    };
    for e in &edges{if e.ymin==e.ymax{hline(e.xmin,e.ymin,e.xmax,budget)?;}}
    let edges=edges.into_iter().filter(|e|e.ymin!=e.ymax).collect::<Vec<_>>();
    let mut xx=Vec::<f32>::with_capacity(edges.len()*2);
    for y in ymin..=ymax{
        crate::input::check(cancel)?;debit(budget,edges.len()as u64)?;xx.clear();
        for (i,e) in edges.iter().enumerate(){
            if y<e.ymin||y>e.ymax{continue;}
            xx.push(e.at(y));
            if y==e.ymax&&y<ymax{xx.push(*xx.last().unwrap());}
            else if (y==e.ymin||y==e.ymax)&&e.dx!=0.{
                let index=xx.len()-1;
                for (j,other) in edges[..i].iter().enumerate(){
                    if j%1024==0{crate::input::check(cancel)?;}
                    debit(budget,1)?;
                    if (y!=other.ymin&&y!=other.ymax)||other.dx==0.{continue;}
                    if xx[index].round()==other.at(y).round(){
                        let offset=if y==e.ymax{-1}else{1};
                        let adjacent=e.at(y+offset);
                        if y+offset>=other.ymin&&y+offset<=other.ymax{
                            let other_adjacent=other.at(y+offset);
                            if xx[index]>adjacent+1.&&xx[index]>other_adjacent+1.{xx[index]=adjacent.max(other_adjacent).round()+1.;}
                            else if xx[index]<adjacent-1.&&xx[index]<other_adjacent-1.{xx[index]=adjacent.min(other_adjacent).round()-1.;}
                            break;
                        }
                    }
                }
            }
        }
        xx.sort_by(f32::total_cmp);
        for pair in xx.as_chunks::<2>().0{hline(up(pair[0]),y,down(pair[1]),budget)?;}
    }
    Ok(())
}
pub(super) fn line(image:&mut RgbaImage,points:&[[f64;2]],width:u32,color:[u8;4],cancel:&AtomicBool,budget:&mut u64)->Result<(),String>{
    for pair in points.windows(2){
        crate::input::check(cancel)?;
        let [x0,y0]=pair[0].map(|v|v as i32);let [x1,y1]=pair[1].map(|v|v as i32);
        let dx=x1-x0;let dy=y1-y0;
        if dx==0&&dy==0{if x0>=0&&y0>=0&&x0<image.width()as i32&&y0<image.height()as i32{debit(budget,1)?;image.put_pixel(x0 as u32,y0 as u32,Rgba(color));}continue;}
        let big=(dx as f64).hypot(dy as f64);let small=(width as f64-1.)/2.;
        let max=up64(small)as f64/big;let min=down64(small)as f64/big;
        let dxmin=down64(min*dy as f64);let dxmax=down64(max*dy as f64);
        let dymin=down64(min*dx as f64);let dymax=down64(max*dx as f64);
        let vertices=[[x0-dxmin,y0+dymax],[x1-dxmin,y1+dymax],[x1+dxmax,y1-dymin],[x0+dxmax,y0-dymin]].map(|p|p.map(|v|v as f64));
        polygon(&vertices,[image.width(),image.height()],cancel,budget,|y,a,b|{for x in a..=b{image.put_pixel(x,y,Rgba(color));}})?;
    }
    Ok(())
}
