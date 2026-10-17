"""Independent ezdxf 1.4.4 placement/OCS and dense WCS curve oracles.

The reference uses ezdxf's matrix44(), multi_insert(), vertices(), OCS and
bulge_to_arc APIs. It never calls native geometry and never starts SleufBase.
"""
from pathlib import Path
import json
import math
import numpy as np
import ezdxf
from ezdxf.math import Vec3,bulge_to_arc
from ezdxf import colors

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/"rust/tests/fixtures/cad-scene"


def samples(entity):
    kind=entity.dxftype()
    if kind=="LINE":return [entity.dxf.start,entity.dxf.end],True
    if kind=="POINT":return [entity.dxf.location],True
    if kind in {"ARC","CIRCLE"}:
        if kind=="CIRCLE":a,b=0,360
        else:
            a=entity.dxf.start_angle%360;b=entity.dxf.end_angle%360
            if b<=a:b+=360
        return list(entity.vertices(np.linspace(a,b,2049))),False
    if kind=="ELLIPSE":
        a=entity.dxf.start_param;b=entity.dxf.end_param
        if b<=a:b+=math.tau
        return list(entity.vertices(np.linspace(a,b,2049))),False
    if kind=="POLYLINE" and entity.is_3d_polyline:
        points=list(entity.points())
        if entity.is_closed:points.append(points[0])
        return points,True
    if kind=="LWPOLYLINE":
        points=[(Vec3(x,y,entity.dxf.elevation),b) for x,y,b in entity.get_points("xyb")];closed=entity.closed
    elif kind=="POLYLINE" and entity.is_2d_polyline:
        points=[(Vec3(v.dxf.location.x,v.dxf.location.y,entity.dxf.elevation.z),v.dxf.bulge) for v in entity.vertices];closed=entity.is_closed
    else:return [],False
    ocs=entity.ocs();result=[];linear=True
    for index in range(len(points) if closed else len(points)-1):
        a,bulge=points[index];b=points[(index+1)%len(points)][0]
        if bulge:
            linear=False
            center,start,end,radius=bulge_to_arc(a,b,bulge)
            if end<=start:end+=math.tau
            curve=[Vec3(center.x+radius*math.cos(t),center.y+radius*math.sin(t),a.z) for t in np.linspace(start,end,2049)]
            if bulge<0:curve.reverse()
            result.extend(ocs.to_wcs(v) for v in curve)
        else:result.extend([ocs.to_wcs(a),ocs.to_wcs(b)])
    return result,linear


def main():
    OUT.mkdir(parents=True,exist_ok=True);ezdxf.options.write_fixed_meta_data_for_testing=True
    matrices=[]
    for normal in [(0,0,1),(0,0,-1),(0,1,0),(1,2,3),(.015624,0,1),(.015626,0,1),(-.015626,.015626,-1),(.3,-.4,.5)]:
        for scaling in [(1,1,1),(2,.5,-1),(-1.25,3,2)]:
            for rotation in [-90,37.5,720.25]:
                doc=ezdxf.new("R2018");base=(2,-3,1);doc.blocks.new("B",base_point=base)
                at=(168000,401000,9)
                ins=doc.modelspace().add_blockref("B",at,dxfattribs=dict(xscale=scaling[0],yscale=scaling[1],zscale=scaling[2],rotation=rotation,extrusion=normal,row_count=2,column_count=3,row_spacing=-7,column_spacing=4))
                local=[base,(0,0,0),(1,2,3),(-4,5,-6)]
                matrices.append(dict(placement=dict(insert=at,scale=scaling,rotation=rotation,normal=normal,rows=2,columns=3,row_spacing=-7,column_spacing=4),
                    base=base,local=local,expected=[[list(copy.matrix44().transform(p)) for p in local] for copy in ins.multi_insert()]))
    (OUT/"matrices.json").write_text(json.dumps(matrices,separators=(",",":"))+"\n",encoding="utf-8")
    doc=ezdxf.new("R2018");doc.appids.new("SOURCE_ID")
    doc.layers.new("ROOT",dxfattribs={"color":5});doc.layers.new("Fixed",dxfattribs={"color":3});doc.layers.new("Off",dxfattribs={"color":1}).off()
    def identify(entity,name):entity.set_xdata("SOURCE_ID",[(1000,name)]);return entity
    base=doc.blocks.new("Base",base_point=(2,-3,1))
    identify(base.add_line((2,-3,1),(5,1,4),dxfattribs={"color":0}),"block-line")
    identify(base.add_point((4,1,2),dxfattribs={"layer":"Fixed"}),"block-point")
    identify(base.add_lwpolyline([(0,0,.414213562373095),(4,0,-.7),(5,2,1.2),(0,3,0)],format="xyb",close=True,dxfattribs={"elevation":2,"extrusion":(.1,.3,.9),"color":0}),"bulged-lw")
    poly=base.add_polyline2d([(0,0),(3,1),(5,4)],close=True,dxfattribs={"elevation":(0,0,4.5),"extrusion":(0,0,-1)})
    poly.vertices[0].dxf.bulge=-.5;identify(poly,"bulged-poly2d")
    identify(base.add_polyline3d([(1,2,3),(4,5,6),(7,2,1)],close=True),"poly3d")
    identify(base.add_circle((1,2,3),2,dxfattribs={"extrusion":(1,2,3)}),"circle")
    identify(base.add_arc((2,3,1),4,350,35,dxfattribs={"extrusion":(0,0,-1)}),"arc")
    identify(base.add_ellipse((1,2,3),major_axis=(3,1,.5),ratio=.3,start_param=.25,end_param=5.75,dxfattribs={"extrusion":(1,-3,0)}),"ellipse")
    nested=doc.blocks.new("Nested",base_point=(1,1,2))
    identify(nested.add_blockref("bAsE",(3,7,0),dxfattribs={"xscale":2,"yscale":.5,"rotation":37,"color":0}),"nested-ref")
    identify(nested.add_line((0,0,0),(4,2,3),dxfattribs={"layer":"Fixed"}),"nested-line")
    msp=doc.modelspace()
    identify(msp.add_line((100,200,3),(103,204,8),dxfattribs={"extrusion":(0,0,-1)}),"world-line")
    identify(msp.add_lwpolyline([(0,0),(2,3),(5,6)],dxfattribs={"elevation":3,"extrusion":(.2,.5,.8)}),"world-lw")
    identify(msp.add_blockref("Nested",(168000,401000,7),dxfattribs={"layer":"ROOT","color":1}),"root-a")
    identify(msp.add_blockref("Nested",(168020,401010,9),dxfattribs={"layer":"ROOT","xscale":-1.5,"yscale":2,"zscale":1.25,"rotation":-41,"extrusion":(.3,.4,.5),"true_color":0x123456}),"root-b")
    identify(msp.add_blockref("Base",(30,40,5),dxfattribs={"xscale":1,"yscale":.3,"zscale":2,"rotation":180,"extrusion":(0,0,-1),"row_count":2,"column_count":3,"row_spacing":-9,"column_spacing":7}),"grid")
    identify(msp.add_blockref("Base",(10,20,1),dxfattribs={"row_count":3,"column_count":4,"row_spacing":0,"column_spacing":0}),"zero-grid")
    identify(msp.add_blockref("Base",(10,30,1),dxfattribs={"layer":"Off"}),"hidden-root")
    msp.add_text("Unsupported text must be reported")
    doc.classes.add_required_classes(doc.dxfversion);doc.classes.classes=dict(sorted(doc.classes.classes.items()))
    doc.saveas(OUT/"scene-ascii.dxf",fmt="asc");doc.saveas(OUT/"scene-binary.dxf",fmt="bin")
    doc=ezdxf.readfile(OUT/"scene-ascii.dxf");expected=[]
    def walk(entities,transform=lambda p:Vec3(p),parent_layer="",parent_rgb=None,parent_visible=True,path=()):
        for entity in entities:
            layer=entity.dxf.layer if entity.dxf.layer!="0" or not parent_layer else parent_layer
            style=doc.layers.get(layer);aci=entity.dxf.color
            if entity.dxf.hasattr("true_color"):rgb=list(colors.int2rgb(entity.dxf.true_color))
            elif aci==0 and parent_rgb is not None:rgb=parent_rgb
            else:
                if aci in (0,256):aci=abs(style.dxf.color)
                rgb=list(colors.aci2rgb(max(1,min(255,aci))))
                if rgb==[255,255,255]:rgb=[220,20,60]
            visible=parent_visible and not entity.dxf.get("invisible",0) and not style.is_off() and not style.is_frozen()
            if entity.dxftype()=="INSERT":
                block=entity.block();cols=entity.dxf.column_count if entity.dxf.column_spacing else 1
                for index,copy in enumerate(entity.multi_insert()):
                    matrix=copy.matrix44()
                    subpath=path+(f"{entity.dxf.handle}:{block.name}[{index//cols},{index%cols}]",)
                    walk(block,lambda p,t=transform,m=matrix:t(m.transform(p)),layer,rgb,visible,subpath)
                continue
            points,linear=samples(entity)
            if points:
                name=entity.get_xdata("SOURCE_ID")[0].value
                expected.append(dict(id=name,path=" / ".join(path),layer=layer,rgb=rgb,visible=bool(visible),linear=linear,
                    points=[list(transform(p)) for p in points]))
    walk(doc.modelspace())
    (OUT/"expected.json").write_text(json.dumps(expected,separators=(",",":"))+"\n",encoding="utf-8")
    print(f"Generated {len(matrices)} placement cases, {len(expected)} WCS primitives")


if __name__=="__main__":main()
