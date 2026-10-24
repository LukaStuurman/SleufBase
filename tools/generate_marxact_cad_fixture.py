"""Real DXF serialization with ezdxf 1.4.4 and original Python import oracles.

Synthetic measurements only. Extracts original parser definitions by AST without
starting SleufBase or importing its UI, settings, bytecode or account services.
"""
from __future__ import annotations
import ast
from dataclasses import dataclass, field, asdict
import json
import math
from pathlib import Path
import re
import sys
from types import ModuleType
import ezdxf
from ezdxf.math import Vec2
from ezdxf.render.mleader import ConnectionSide

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/"rust/tests/fixtures/marxact-cad"


def reference():
    source=ast.parse((ROOT/"marxact_import.py").read_text(encoding="utf-8"))
    nodes=[]
    for node in source.body:
        if isinstance(node,ast.FunctionDef) and node.name=="trench_centerline":break
        if isinstance(node,(ast.ClassDef,ast.FunctionDef)):nodes.append(node)
    module=ModuleType("marxact_fixture_reference")
    sys.modules[module.__name__]=module
    module.__dict__.update(dataclass=dataclass,field=field,math=math,re=re,Path=Path,ezdxf=ezdxf,
        LEADER_MATCH_TOLERANCE=0.08,BOUNDARY_MIN_AREA=0.02)
    future=ast.ImportFrom(module="__future__",names=[ast.alias(name="annotations")],level=0)
    tree=ast.fix_missing_locations(ast.Module(body=[future]+nodes,type_ignores=[]))
    exec(compile(tree,str(ROOT/"marxact_import.py"),"exec"),module.__dict__)
    return module


def main():
    OUT.mkdir(parents=True,exist_ok=True)
    ezdxf.options.write_fixed_meta_data_for_testing=True
    doc=ezdxf.new("R2018")
    msp=doc.modelspace()
    doc.blocks.new("marxact_point").add_circle((0,0),0.05)
    polygon=[(168000,401000,10),(168004,401000,14),(168004,401002,18),(168000,401002,12)]
    # Matching labels on this *open* line must never create another trench.
    open_polygon=[(168010+x-168000,y,z) for x,y,z in polygon]
    msp.add_polyline3d(polygon,close=True)
    msp.add_polyline3d(open_polygon,close=False)
    def label(point,content,index):
        builder=msp.add_multileader_mtext(dxfattribs={"layer":"LABEL"})
        builder.set_content(content,char_height=0.15)
        tip=Vec2(point[:2])
        builder.add_leader_line(ConnectionSide.left,[tip,tip+Vec2(0.4,0.4)])
        builder.build(insert=tip+Vec2(20+index,10))
        # The extra line deliberately contains a different tip. Python chooses
        # the first nonempty line, never the text/landing base or last line.
        from ezdxf.entities.mleader import LeaderLine
        from ezdxf.math import Vec3
        extra=LeaderLine();extra.vertices=[Vec3(tip.x+100,tip.y+100,0)]
        builder.context.leaders[0].lines.append(extra)
    for index,point in enumerate(polygon+open_polygon):
        label(point,"Id=28^JName=PS nested^JHeight=12,25^J",index)
    for index,(point,content,layer) in enumerate([
        ((168001,401001,3.0),"Name=water detail^JHeight=9,3 m NAP","Water"),
        ((168003,401001,7.5),"Name=ls kabel^JHeight=ongeldig","Laagspanning"),
        ((168020,401020,8.0),"Name=buiten^JHeight=8","0"),
    ]):
        msp.add_blockref("marxact_point",point,dxfattribs={"layer":layer})
        label(point,content,index+10)
    path=OUT/"nested-leaders.dxf"
    doc.classes.add_required_classes(doc.dxfversion)
    doc.classes.classes=dict(sorted(doc.classes.classes.items()))
    doc.saveas(path)
    doc.saveas(OUT/"nested-leaders-binary.dxf",fmt="bin")
    ref=reference()
    parsed=ref.parse_marxact_dxf(path)
    loaded=ezdxf.readfile(path)
    expected=dict(oracle="original marxact_import.py / ezdxf 1.4.4",file=path.name,
        leader_count=parsed.leader_count,insert_count=parsed.insert_count,
        assigned_insert_count=parsed.assigned_insert_count,
        leaders=[asdict(item) for item in ref._collect_leaders(loaded.modelspace())],
        trenches=[asdict(item) for item in parsed.trenches])
    texts=["Name=first^JName=second^JHeight=3,14", "Name=water^format^JHeight=-2.25 metres",
        "Height=+12.3e2^JName= spaced name  ","NAME=glasvezel\\PHeight = 3,1441",
        "Name=water\r\nHeight=4.5", "Name=datum^JHeight=NaN", "Name=ld^JHeight=.5"]
    expected["text_cases"]=[dict(content=text,name=ref.parse_marxact_leader_content(text)[0],
        height=ref.parse_marxact_leader_content(text)[1]) for text in texts]
    (OUT/"expected.json").write_text(json.dumps(expected,indent=2,ensure_ascii=False,allow_nan=False)+"\n",encoding="utf-8")
    print(f"Generated {path.name}: {len(parsed.trenches)} trench, {parsed.leader_count} labels")


if __name__=="__main__":main()
