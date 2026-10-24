"""Real native DXFs versus the active original annotation + LEADER export."""
from pathlib import Path
import importlib
import json
import sys
import ezdxf
from generate_object_export_fixtures import reference
from generate_template_geometry_fixtures import signature
from verify_native_template_images import compare, verify_preservation
from verify_native_template_edits import records

ROOT=Path(__file__).resolve().parents[1]
FIXTURE=ROOT / "rust/tests/fixtures/profile-leaders"


def full_signature(entity):
    result=signature(entity)
    if entity.dxftype()=="LEADER":
        d=entity.dxf
        result.update(vertices=[list(v) for v in entity.vertices],dimstyle=d.dimstyle,arrow=d.has_arrowhead,path_type=d.path_type,annotation_type=d.annotation_type,hook=d.has_hookline,block_color=d.block_color,normal=list(d.normal_vector),offset=list(d.leader_offset_block_ref))
    return result


def main():
    output=Path(sys.argv[1]);ref=reference();module=importlib.import_module(ref.__package__+".cadastral_export")
    cases=json.loads((FIXTURE / "layout.json").read_text(encoding="utf-8"));source=FIXTURE / "empty.dxf"
    count=0
    for index,case in enumerate(cases):
        original=ezdxf.readfile(source);baseline=ezdxf.readfile(source)
        exporter=module.CadastralDxfExporter(wfs_client=None)
        exporter._distribute_template_leader_labels(original,original.modelspace(),case["entries"],case.get("marker_scale",.02),**case["options"])
        path=output / f"{index}.dxf";native=ezdxf.readfile(path)
        actual=list(native.modelspace());expected=list(original.modelspace())
        assert len(actual)==len(expected)==len(case["entries"])*3
        for i,(a,b) in enumerate(zip(actual,expected)):
            compare(full_signature(a),full_signature(b),f"case[{index}].entity[{i}]")
            if a.dxftype()=="LEADER":
                assert a.dxf.annotation_handle==actual[i-1].dxf.handle
                assert b.dxf.annotation_handle==expected[i-1].dxf.handle
                assert native.entitydb[a.dxf.annotation_handle].dxftype()=="INSERT"
                assert len(native.entitydb[a.dxf.annotation_handle].attribs)==2
        verify_preservation(source,path,baseline,allowed_extra={"BLOCK_RECORD","BLOCK","ENDBLK","ATTDEF","LEADER","INSERT","ATTRIB","SEQEND","LWPOLYLINE"},allowed_tables=("BLOCK_RECORD",))
        raw=records(path);primary=[]
        for r in raw:
            if r[0][1]=="SECTION":continue
            for c,v in r:
                if c==100:break
                if c in {5,105}:primary.append(int(v,16));break
        assert len(set(primary))==len(primary) and int(native.header["$HANDSEED"],16)>max(primary)
        table=next(r for r in raw if r[0]==(0,"TABLE") and (2,"BLOCK_RECORD") in r)
        assert int(next(v for c,v in table if c==70))==sum(r[0]==(0,"BLOCK_RECORD") for r in raw)
        audit=native.audit();assert not audit.errors and not audit.fixes,(index,audit.errors,audit.fixes)
        count+=len(actual)
    print(f"{len(cases)} active original leader layouts, {count} native objects, all attributes/annotation links and source records verified")


if __name__=="__main__":main()
