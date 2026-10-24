"""Independent full-document reader/auditor for generated native object exports."""
from pathlib import Path
import argparse
import json
import math
from generate_object_export_fixtures import signature
import ezdxf

ROOT=Path(__file__).resolve().parents[1]
def compare(actual,expected,path="document"):
    if isinstance(expected,float):
        if not math.isclose(actual,expected,rel_tol=0,abs_tol=1e-9):raise AssertionError(f"{path}: {actual!r} != {expected!r}")
    elif isinstance(expected,dict):
        if set(actual)!=set(expected):raise AssertionError(f"{path}: fields {set(actual)} != {set(expected)}")
        for k,v in expected.items():compare(actual[k],v,f"{path}.{k}")
    elif isinstance(expected,list):
        if len(actual)!=len(expected):raise AssertionError(f"{path}: {len(actual)} != {len(expected)} items")
        for i,(a,b) in enumerate(zip(actual,expected)):compare(a,b,f"{path}[{i}]")
    elif actual!=expected:raise AssertionError(f"{path}: {actual!r} != {expected!r}")

def main():
    p=argparse.ArgumentParser();p.add_argument("directory",type=Path);args=p.parse_args()
    cases=json.loads((ROOT/"rust/tests/fixtures/object-export/cases.json").read_text(encoding="utf-8"))
    for case in cases:
        path=args.directory/f"{case['name']}.dxf"
        doc=ezdxf.readfile(path)
        audit=doc.audit()
        if audit.errors or audit.fixes:raise AssertionError(f"{path}: audit errors={audit.errors}, repairs={audit.fixes}")
        expected=dict(case["expected"]);expected["layers"]=case["native_layers"]
        compare(signature(path),expected,case["name"])
    cli=next(case for case in cases if case["name"]=="cli")
    expected=dict(cli["expected"]);expected["layers"]=cli["native_layers"]
    compare(signature(args.directory/"cli-actual.dxf"),expected,"actual CLI")
    doc=ezdxf.readfile(args.directory/"extras.dxf")
    audit=doc.audit()
    if audit.errors or audit.fixes:raise AssertionError(f"Auxiliary CAD audit: {audit.errors}, {audit.fixes}")
    faces=list(doc.modelspace().query("POLYLINE"))
    assert len(faces)==1 and faces[0].is_closed
    assert [tuple(v.dxf.location) for v in faces[0].vertices]==[(0,0,10),(4,0,11),(4,1,12),(0,1,13)]
    assert "KADASTRALE_GRENZEN_MET_LANGE_NAAM_É" in doc.layers
    label=next(t for t in doc.modelspace().query("TEXT") if t.dxf.layer=="KAD_STRAATNAAM")
    assert label.dxf.text=="Straatnaam É" and label.dxf.rotation==30
    assert len(doc.modelspace().query('LINE[layer=="VIRTUAL_TRENCH_MARKER"]'))==1
    assert len(doc.modelspace().query('LINE[layer=="BGT_WEGDEEL"]'))==1
    print(f"All {len(cases)} native reference documents, the actual CLI and auxiliary map/trench layers pass independent verification; ezdxf audit needs no repairs. New-layer colors include the declared-rule correction.")

if __name__=="__main__":main()
