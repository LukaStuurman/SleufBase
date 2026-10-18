"""Independent geometry/attribute/ownership audit after native composition."""
from pathlib import Path
import json
import sys
import ezdxf
from PIL import Image
from generate_template_geometry_fixtures import signature
from verify_native_template_images import compare, verify_preservation, identity
from verify_native_template_edits import records

ROOT=Path(__file__).resolve().parents[1]


def main():
    output=Path(sys.argv[1])
    cases=json.loads((ROOT / "rust/tests/fixtures/template-geometry/reference.json").read_text(encoding="utf-8"))
    for case in cases:
        source=ROOT / case["path"]
        native=output / (case["name"]+".dxf")
        original=ezdxf.readfile(source)
        doc=ezdxf.readfile(native)
        raw=records(native)
        raw_by_handle={identity(r)[1]:r for r in raw if identity(r)[0] not in {"SECTION","TABLE","CLASS"}}
        verify_preservation(source,native,original,allowed_extra={"INSERT","ATTRIB","SEQEND","LWPOLYLINE","TEXT","CIRCLE"})
        new=[]
        for entity in doc.entitydb.values():
            if entity.is_alive and entity.dxftype() in {"INSERT","LWPOLYLINE","TEXT","CIRCLE"} and entity.dxf.layer=="Native geometry":
                # Model/paper requests only; no block definition should gain entities.
                new.append(entity)
        by_layout={}
        for entity in new:
            name=entity.get_layout().name
            by_layout.setdefault(name,[]).append(entity)
        for request,expected in zip(case["requests"],case["expected"]):
            layout=request["layout"] or "Model"
            entity=by_layout[layout].pop(0)
            compare(signature(entity),expected,f'{case["name"]}.{layout}')
            assert entity.dxf.owner==doc.layouts.get(layout).layout_key
            if entity.dxftype()=="INSERT" and entity.attribs:
                # ezdxf reassigns ENTITIES-linked SEQEND owners on reload; inspect
                # the serialized chain, matching the original writer's ownership.
                seqend=raw_by_handle[entity.seqend.dxf.handle]
                assert next(v for c,v in seqend if c==330)==entity.dxf.handle
                assert all(a.dxf.owner==entity.dxf.handle and a.dxf.paperspace==entity.dxf.paperspace for a in entity.attribs)
                assert all(next(v for c,v in raw_by_handle[a.dxf.handle] if c==330)==entity.dxf.owner for a in entity.attribs)
        assert all(not remaining for remaining in by_layout.values())
        assert len(new)==len(case["requests"])
        for name,color,true_color in [("Native geometry",3,0x123456),("PROEFSLEUF_TEMPLATE_IMAGES",7,None)]:
            layer=doc.layers.get(name)
            assert layer.dxf.color==color and layer.dxf.get("true_color")==true_color
        assert doc.layers.get("0").dxf.color==original.layers.get("0").dxf.color
        table=next(r for r in raw if r[0]==(0,"TABLE") and (2,"LAYER") in r)
        assert int(next(v for c,v in table if c==70))==len(doc.layers)
        primary=[int(v,16) for r in raw if r[0][1] not in {"SECTION","ENDSEC","EOF"} for c,v in r[:next((i for i,(c,_) in enumerate(r) if c==100),len(r))] if c in {5,105}]
        assert len(set(primary))==len(primary) and int(doc.header["$HANDSEED"],16)>max(primary)
        images=list(doc.modelspace().query("IMAGE"));assert len(images)==1
        image=images[0];definition=doc.entitydb[image.dxf.image_def_handle]
        assert doc.entitydb[image.dxf.image_def_reactor_handle].dxf.owner==image.dxf.handle
        with Image.open(definition.dxf.filename) as png:
            assert png.size==(12,8) and set(png.getdata())=={(4,5,6,255)}
        baseline=original.audit();audit=doc.audit()
        assert not audit.errors and [(e.code,e.message) for e in audit.fixes]==[(e.code,e.message) for e in baseline.fixes], audit.fixes
        print(f'{case["name"]}: {len(new)} original primitives/block inserts, attributes and mixed raster/layer ownership verified')
    print("Native template geometry independently verified")


if __name__=="__main__":main()
