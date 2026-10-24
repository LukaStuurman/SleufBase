"""Original template block insertion and ezdxf primitive references."""
from pathlib import Path
import importlib
import json
import ezdxf
from ezdxf.enums import TextEntityAlignment
from generate_object_export_fixtures import reference

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "rust/tests/fixtures/template-geometry"


def signature(entity):
    d = entity.dxf
    result = dict(kind=entity.dxftype(), layer=d.layer, color=d.color, true_color=d.get("true_color"), lineweight=d.lineweight)
    if entity.dxftype() == "LWPOLYLINE":
        result.update(vertices=[list(v) for v in entity.get_points("xy")], closed=entity.closed)
    elif entity.dxftype() == "CIRCLE":
        result.update(center=list(d.center), radius=d.radius)
    elif entity.dxftype() in {"TEXT", "ATTRIB"}:
        result.update(text=d.text, insert=list(d.insert), align=list(d.align_point), height=d.height, rotation=d.rotation, width=d.width, oblique=d.oblique, generation=d.text_generation_flag, halign=d.halign, style=d.style, extrusion=list(d.extrusion), thickness=d.thickness)
        if entity.dxftype() == "ATTRIB":
            result.update(tag=d.tag, flags=d.flags, valign=d.valign, lock_position=d.lock_position, field_length=d.field_length, attribute_type=d.attribute_type, linetype=d.linetype)
        else:
            result.update(valign=d.valign)
    elif entity.dxftype() == "INSERT":
        result.update(name=d.name, insert=list(d.insert), scale=[d.xscale,d.yscale,d.zscale], rotation=d.rotation, attributes=[signature(a) for a in entity.attribs])
    return result


def main():
    assert ezdxf.__version__ == "1.4.4"
    ref = reference()
    exporter = importlib.import_module(ref.__package__ + ".cadastral_export").CadastralDxfExporter(wfs_client=None)
    ezdxf.options.write_fixed_meta_data_for_testing = True
    OUT.mkdir(parents=True, exist_ok=True)
    doc = ezdxf.new("R2018")
    doc.layouts.new("Papier Ø")
    doc.layers.add("Attribute layer", color=3)
    block = doc.blocks.new("Titel Ø", base_point=(5,7,2))
    block.add_circle((5,7),2)
    for index, tag in enumerate(["NAME","DEFAULT","CONSTANT","CENTERED"]):
        block.add_attdef(tag,(6+index,9+index,3),text="default "+tag,dxfattribs=dict(height=2.5+index,style="Standard",layer="Attribute layer",color=index,linetype="ByBlock",flags=[0,1,2,8][index],rotation=[0,45,90,270][index],oblique=15,width=0.8,text_generation_flag=2,halign=0 if index<3 else 1,valign=0 if index<3 else 2,align_point=(4,5,2),lock_position=1,field_length=5))
    doc.blocks.new("Empty").add_line((0,0),(1,1))
    doc.classes.add_required_classes(doc.dxfversion)
    doc.classes.classes = type(doc.classes.classes)(sorted(doc.classes.classes.items()))
    path = OUT / "blocks.dxf"
    doc.saveas(path)
    path.write_bytes(path.read_bytes().replace(b"\r\n",b"\n"))
    cases = []
    for name, path in [("shipped",ROOT / "assets/cadastral_template.dxf"),("blocks",path)]:
        doc = ezdxf.readfile(path)
        paper = next(layout.name for layout in doc.layouts if layout.name != "Model")
        # The shipped A0 sheet is inactive; the synthetic cases cover active too.
        layouts = [None,paper] if name == "shipped" else [None,"Layout1","Papier Ø"]
        layers = [dict(name="Native geometry",color=3,true_color=0x123456),dict(name="0",color=2,true_color=None)]
        requests, expected = [], []
        names = [b.name for b in doc.blocks if "PROFIELRASTER" in b.name] if name == "shipped" else ["Titel Ø","Empty"]
        for layout in layouts:
            for index, block_name in enumerate(names):
                for scale in [.02,.1,1.5]:
                    requests.append(dict(layer="Native geometry",layout=layout,paint=dict(color=2,lineweight=25),shape=dict(Block=dict(name=block_name,insert=[index*5.5+2,-scale*10],scale=scale,attributes={"NUMMER":"DP007B","GROEP":"Proefsleuven","OMSCHRIJVING":"PS 7B","SCHAAL":"Schaal: 1:100","REFERENTIE":"8.00 N.A.P.","WAARDE":"3.14","TITEL":"N.A.P.","NAME":"Nieuw Ø 漢😀"}))))
            requests.extend([
                dict(layer="Native geometry",layout=layout,paint={},shape=dict(Block=dict(name=names[0],insert=[8,9],scale=.02,attributes={}))),
                dict(layer="Native geometry",layout=layout,paint=dict(color=0,true_color=0x987654,lineweight=13),shape=dict(Polyline=dict(vertices=[[0,1],[2,3],[4,2]],closed=True))),
                dict(layer="Native geometry",layout=layout,paint={},shape=dict(Circle=dict(center=[-3,4],radius=.1))),
            ])
            for alignment in [(0,0),(1,2),(2,3),(4,0)]:
                requests.append(dict(layer="Native geometry",layout=layout,paint={},shape=dict(Text=dict(text="Maaiveld: 10.25 N.A.P. Ø 漢😀",insert=[12,34],height=.035,rotation=90,style="Standard",halign=alignment[0],valign=alignment[1]))))
        for layer in layers:
            if layer["name"] not in doc.layers:
                doc.layers.add(layer["name"],color=layer["color"],dxfattribs={"true_color":layer["true_color"]})
        for request in requests:
            target = doc.modelspace() if request["layout"] is None else doc.layouts.get(request["layout"])
            attrs = dict(layer=request["layer"], **request["paint"])
            kind, shape = next(iter(request["shape"].items()))
            if kind == "Block":
                entity = exporter._insert_template_block(target,shape["name"],shape["insert"],request["layer"],attributes=shape["attributes"],scale=shape["scale"],color=request["paint"].get("color"))
                for key,value in request["paint"].items():setattr(entity.dxf,key,value)
            elif kind == "Polyline":
                entity = target.add_lwpolyline(shape["vertices"],close=shape["closed"],dxfattribs=attrs)
            elif kind == "Circle":
                entity = target.add_circle(shape["center"],shape["radius"],dxfattribs=attrs)
            else:
                attrs.update(height=shape["height"],rotation=shape["rotation"],style=shape["style"])
                entity = target.add_text(shape["text"],dxfattribs=attrs)
                alignment = {(0,0):TextEntityAlignment.LEFT,(1,2):TextEntityAlignment.MIDDLE_CENTER,(2,3):TextEntityAlignment.TOP_RIGHT,(4,0):TextEntityAlignment.MIDDLE}[(shape["halign"],shape["valign"])]
                entity.set_placement(shape["insert"],align=alignment)
            expected.append(signature(entity))
        cases.append(dict(name=name,path=path.relative_to(ROOT).as_posix(),layers=layers,requests=requests,expected=expected))
    (OUT / "reference.json").write_text(json.dumps(cases,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    print([(c["name"],len(c["requests"])) for c in cases])


if __name__ == "__main__":main()
