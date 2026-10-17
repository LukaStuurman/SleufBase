"""Synthetic ASCII/binary DXF pairs, authored and re-read with ezdxf 1.4.4."""
from pathlib import Path
import json
import ezdxf

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/"rust/tests/fixtures/dxf-encoding"


def main():
    OUT.mkdir(parents=True,exist_ok=True)
    ezdxf.options.write_fixed_meta_data_for_testing=True
    cases=[]
    for version,encoding,layer in [
        ("R12","cp1252","Réseau_ë_水"),
        ("R2000","cp1251","Вода_水"),
        ("R2000","cp932","水道_ë"),
        ("R2018","utf8","水_Δ_ë"),
    ]:
        doc=ezdxf.new(version);doc.encoding=encoding
        doc.layers.new(layer,dxfattribs={"color":5})
        doc.appids.new("SLEUFBASE")
        msp=doc.modelspace()
        line=msp.add_line((168000.123,401000.456,-12.75),(168004.5,401002.25,0),dxfattribs={"layer":layer,"color":5})
        line.set_xdata("SLEUFBASE",[(1000,"measurement"),(1002,"{"),
            (1004,bytes([0,128,255,12,0,255])),(1070,-17),(1071,2_000_000_000),
            (1010,(1.25,2.5,-3.75)),(1040,12.25),(1002,"}")])
        msp.add_circle((168001,401001,3),0.5,dxfattribs={"layer":layer})
        if version=="R12":msp.add_polyline2d([(0,0),(2,0),(2,1)],close=True,dxfattribs={"layer":layer})
        else:msp.add_lwpolyline([(0,0),(2,0),(2,1)],close=True,dxfattribs={"layer":layer})
        doc.classes.add_required_classes(doc.dxfversion)
        doc.classes.classes=dict(sorted(doc.classes.classes.items()))
        name=f"{version.lower()}-{encoding}"
        ascii_path=OUT/f"{name}-ascii.dxf"
        binary_path=OUT/f"{name}-binary.dxf"
        doc.saveas(ascii_path,fmt="asc");doc.saveas(binary_path,fmt="bin")
        original=ezdxf.readfile(ascii_path).modelspace()[0]
        binary=ezdxf.readfile(binary_path).modelspace()[0]
        assert original.dxf.layer==binary.dxf.layer
        cases.append(dict(ascii=ascii_path.name,binary=binary_path.name,layer=original.dxf.layer,
            start=list(original.dxf.start),end=list(original.dxf.end),features=3))
    (OUT/"manifest.json").write_text(json.dumps(cases,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    print(f"Generated {len(cases)} DXF ASCII/binary pairs")


if __name__=="__main__":main()
