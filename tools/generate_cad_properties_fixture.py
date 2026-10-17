"""CAD appearance/metadata oracles from original dxf.py and ezdxf 1.4.4.

Uses synthetic data and AST-extracted helpers; does not start SleufBase.
Writes ASCII/binary pairs and the canonical palette used by the Python app.
"""
from pathlib import Path
import json
import ezdxf
from ezdxf import colors
from generate_marxact_alignment_fixtures import functions

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/"rust/tests/fixtures/cad-properties"


def main():
    OUT.mkdir(parents=True,exist_ok=True)
    ref=functions("dxf.py",["_resolve_entity_color","_format_xdata","_format_insert_attributes","_entity_metadata"],
        {"Path":Path,"colors":colors})
    ezdxf.options.write_fixed_meta_data_for_testing=True
    doc=ezdxf.new("R2018")
    doc.layers.new("Water",dxfattribs={"color":5})
    doc.layers.new("True",dxfattribs={"color":5,"true_color":0x123456})
    doc.layers.new("Black",dxfattribs={"color":5,"true_color":0})
    doc.layers.new("Off",dxfattribs={"color":4}).off()
    doc.layers.new("Frozen",dxfattribs={"color":190}).freeze()
    doc.appids.new("SLEUFBASE");doc.appids.new("MEASUREMENTS")
    msp=doc.modelspace()
    cases=[]
    def record(entity,label,difference=None,native_rgb=None):
        cases.append(dict(handle=entity.dxf.handle,label=label,difference=difference,native_rgb=native_rgb))
    for index in range(1,256):
        entity=msp.add_line((168000,401000+index,0),(168001,401000+index,0),dxfattribs={"layer":"Water","color":index})
        record(entity,f"ACI {index}")
    for layer,aci,label in [("Water",256,"BYLAYER"),("Water",0,"BYBLOCK at top level"),
        ("True",256,"layer true-color"),("Black",256,"layer black"),
        ("Off",256,"off layer"),("Frozen",256,"frozen layer")]:
        entity=msp.add_line((168010,401010,0),(168011,401011,0),dxfattribs={"layer":layer,"color":aci})
        rgb=None;difference=None
        if layer=="True":rgb=[0x12,0x34,0x56];difference="Honor layer true-color, ignored by original Python reader."
        elif layer=="Black":rgb=[0,0,0];difference="Honor explicit black layer true-color."
        elif layer=="Off":rgb=list(colors.aci2rgb(4));difference="Keep off layer hidden by default and retain its actual ACI color."
        elif layer=="Frozen":difference="Keep frozen layer hidden by default."
        record(entity,label,difference,rgb)
    for value in [0x123456,0xffffff,0]:
        entity=msp.add_line((168020,401020,0),(168021,401021,0),dxfattribs={"layer":"Water","true_color":value})
        record(entity,f"entity true-color {value}","Honor explicit RGB black instead of treating zero as missing." if value==0 else None,
            [0,0,0] if value==0 else None)
    entity=msp.add_line((1e6,2e6,0),(1e6+1,2e6+1,0),dxfattribs={"invisible":1})
    record(entity,"invisible entity","Respect entity invisibility, ignored by original overlay renderer.")
    block=doc.blocks.new("POINT_BLOCK");block.add_line((0,0),(1,0))
    insert=msp.add_blockref("POINT_BLOCK",(168025,401025,3),dxfattribs={"layer":"Water","linetype":"CONTINUOUS","lineweight":35})
    insert.add_attrib("Code","first");insert.add_attrib("Code","PS 42");insert.add_attrib("Opmerking","Meting ë")
    insert.set_xdata("SLEUFBASE",[(1000,"original metadata"),(1002,"{"),(1004,b"\x00\x80\xff\n\r\t'\"\\"),
        (1010,(1.25,2.5,-3.75)),(1040,1e-5),(1040,1e20),(1070,-17),(1071,2_000_000_000),(1002,"}")])
    insert.set_xdata("MEASUREMENTS",[(1000,"ID-42"),(1005,insert.dxf.handle)])
    record(insert,"INSERT attributes and XDATA")
    doc.classes.add_required_classes(doc.dxfversion)
    doc.classes.classes=dict(sorted(doc.classes.classes.items()))
    ascii_path=OUT/"properties-ascii.dxf";binary_path=OUT/"properties-binary.dxf"
    doc.saveas(ascii_path,fmt="asc");doc.saveas(binary_path,fmt="bin")
    loaded=ezdxf.readfile(ascii_path)
    for case in cases:
        entity=loaded.entitydb[case["handle"]]
        layer=loaded.layers.get(entity.dxf.layer)
        case["python_rgb"]=list(ref._resolve_entity_color(entity,loaded))
        case["native_rgb"]=case["native_rgb"] or case["python_rgb"]
        case["visible"]=not (entity.dxf.get("invisible",0) or layer.is_off() or layer.is_frozen())
        case["details"]=ref._entity_metadata(entity,ascii_path)
        case["details"]["Punten"]="2"
    (OUT/"expected.json").write_text(json.dumps(cases,indent=2,ensure_ascii=False)+"\n",encoding="utf-8")
    table="// SleufBase Python baseline: ezdxf 1.4.4 default ACI table.\n"
    table+="pub const ACI: [u32; 256] = [\n"
    for start in range(0,256,8):table+="    "+", ".join(f"0x{value:06x}" for value in colors.DXF_DEFAULT_COLORS[start:start+8])+",\n"
    (ROOT/"rust/src/cad_palette.rs").write_text(table+"];\n",encoding="utf-8")
    print(f"Generated {len(cases)} CAD appearance/metadata cases")


if __name__=="__main__":main()
