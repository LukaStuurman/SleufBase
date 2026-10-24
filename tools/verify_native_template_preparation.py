"""Independent repeated-state symbol/property and complete-tag preservation audit."""
from pathlib import Path
import json
import sys
import ezdxf
from ezdxf.lldxf.validator import make_table_key
from generate_template_preparation_fixtures import ROOT,OUT,snapshot
from verify_native_template_images import compare,identity,mask_seed
from verify_native_template_edits import records


def preserve(source,output,updated):
    before=records(source);after=records(output);pos=0;matched=set()
    for old in before:
        key=identity(old)
        while pos<len(after) and identity(after[pos])!=key:pos+=1
        assert pos<len(after),("lost/reordered source",key)
        new=after[pos];matched.add(pos);pos+=1
        if key==("SECTION","HEADER"):assert mask_seed(old)==mask_seed(new)
        elif key[0]=="TABLE" and key[1] in {"STYLE","LAYER"}:
            assert [(c,v) for c,v in old if c!=70]==[(c,v) for c,v in new if c!=70]
        elif key[0]=="LAYER" and make_table_key(next(v for c,v in old if c==2)) in updated:
            assert [(c,v) for c,v in old if c not in {62,370}]==[(c,v) for c,v in new if c not in {62,370}],key
        else:assert old==new,("unexpected source change",key)
    handles=[]
    for r in after:
        if r[0][1]=="SECTION":continue
        for c,v in r:
            if c==100:break
            if c in {5,105}:handles.append(int(v,16));break
    assert len(handles)==len(set(handles))
    assert all(r[0][1] in {"LAYER","STYLE","CIRCLE","TEXT","INSERT","ATTRIB","SEQEND"} for i,r in enumerate(after) if i not in matched)
    for name in ["STYLE","LAYER"]:
        table=next(r for r in after if r[0]==(0,"TABLE") and (2,name) in r)
        assert int(next(v for c,v in table if c==70))==sum(r[0]==(0,name) for r in after)
    return max(handles)


def main():
    output=Path(sys.argv[1]);cases=json.loads((OUT / "reference.json").read_text(encoding="utf-8"));states=0
    for case in cases:
        updated=set()
        for i,step in enumerate(case["steps"]):
            if step["kind"]=="prepare":updated|={make_table_key(n) for n in ["B-WE-OG-TERREIN_PROFIELLIJN-GD","X-XX-AL-PROFIELBALK-SD","X-XX-AL-VERWIJZING-SD",*[p["name"] for p in step["points"] if p["name"]!="0"]]}
            path=output / f'{case["name"]}-{i}.dxf';doc=ezdxf.readfile(path)
            compare(snapshot(doc,case.get("compare_entities",True)),step["expected"],f'{case["name"]}.{i}')
            maximum=preserve(ROOT / case["path"],path,updated)
            assert int(doc.header["$HANDSEED"],16)>maximum
            baseline=ezdxf.readfile(ROOT / case["path"]).audit();audit=doc.audit();assert not audit.errors and [e.code for e in audit.fixes]==[e.code for e in baseline.fixes],(case["name"],i,audit.errors,audit.fixes)
            states+=1
    print(f"{len(cases)} original preparation sequences/{states} states: fonts, layer properties, Unicode blocks/attributes, source tags and handles verified")


if __name__=="__main__":main()
