"""Active production v0.3.10 LEADER contract, without inactive donor patches."""
from pathlib import Path
from types import SimpleNamespace
import importlib
import json
import random
import ezdxf
from generate_object_export_fixtures import reference
from verify_native_template_edits import records

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT / "rust/tests/fixtures/profile-leaders"


def capture(exporter,case):
    leaders=[];lines=[]
    exporter._ensure_template_profile_leader_block=lambda doc:None
    exporter._add_template_profile_multileader=lambda ms,**kw:leaders.append(kw)
    model=SimpleNamespace(add_lwpolyline=lambda points,**kw:lines.append((points,kw)))
    exporter._distribute_template_leader_labels(None,model,case["entries"],case.get("marker_scale",0.02),**case["options"])
    return json.loads(json.dumps(dict(leaders=leaders,lines=lines)))


def main():
    assert ezdxf.__version__=="1.4.4"
    ref=reference();module=importlib.import_module(ref.__package__+".cadastral_export")
    assert module.CadastralDxfExporter._distribute_template_leader_labels.__module__==module.__name__
    assert "install_dynamic_profile_leader_patch" not in (ROOT / "__init__.py").read_text(encoding="utf-8")
    exporter=module.CadastralDxfExporter(wfs_client=None)
    cases=json.loads((ROOT / "tests/fixtures/profile_label_layout.json").read_text(encoding="utf-8"))
    for case in cases:assert capture(exporter,case)==case["expected"],"original golden changed"
    rng=random.Random(73665)
    for index in range(60):
        entries=[]
        for j in range(index%11):
            x=rng.uniform(-5,5);y=rng.uniform(-1,2);bx=x+rng.choice([0.,0.02,-0.03])
            entries.append(dict(anchor_x=x,leader_top_y=y,leader_scale=rng.choice([0.,0.005,0.02,0.1]),description=rng.choice(["Waterleiding"," Datakabel 漢😀 ","", "Gas"]),depth_text=f"Diepte: {rng.uniform(0,2):.2f}",band_x=bx,band_y=-2,band_connector_y=-1.96,leader_line_start_y=y-0.4,leader_block_start_y=y-0.2,leader_color=rng.choice([0,1,3,7,255])))
        options=dict(hard_static_line_segments=[[-2,-0.1,2,-0.1]],soft_static_line_segments=[[-1,.3,1,.3]],static_text_boxes=[[-.2,-.2,.2,.2]],min_text_x=-5,max_text_x=5,avoid_collisions=index%7!=0)
        case=dict(entries=entries,options=options,marker_scale=0.02);case["expected"]=capture(exporter,case);cases.append(case)
    OUT.mkdir(parents=True,exist_ok=True)
    (OUT / "layout.json").write_text(json.dumps(cases,ensure_ascii=False,indent=2)+"\n",encoding="utf-8",newline="\n")
    # Original active annotation block for native resource declaration.
    ezdxf.options.write_fixed_meta_data_for_testing=True
    doc=ezdxf.new("R2018");exporter=module.CadastralDxfExporter(wfs_client=None);exporter._setup_text_styles(doc)
    doc.layers.add("X-XX-AL-VERWIJZING-SD",color=7,dxfattribs={"lineweight":0})
    doc.layers.add("X-XX-AL-PROFIELBALK-SD",color=7,dxfattribs={"lineweight":0})
    doc.classes.add_required_classes(doc.dxfversion);doc.classes.classes=type(doc.classes.classes)(sorted(doc.classes.classes.items()))
    path=OUT / "empty.dxf";doc.saveas(path);path.write_bytes(path.read_bytes().replace(b"\r\n",b"\n"))
    name=exporter._ensure_template_profile_leader_block(doc);handle=doc.blocks[name].block_record_handle
    block_path=OUT / "blocks.dxf";doc.saveas(block_path);block_path.write_bytes(block_path.read_bytes().replace(b"\r\n",b"\n"))
    all_records=records(block_path)
    record=next(r for r in all_records if r[0]==(0,"BLOCK_RECORD") and (2,name) in r)
    geometry=[r for r in all_records if (330,handle) in r]
    assert len(geometry)==4
    (ROOT / "rust/src/data/profile-leader-annotation.json").write_text(json.dumps(dict(record=record,geometry=geometry),ensure_ascii=False,indent=2)+"\n",encoding="utf-8",newline="\n")
    print(len(cases),"active original label layouts;",sum(len(c["entries"]) for c in cases),"placements")


if __name__=="__main__":main()
