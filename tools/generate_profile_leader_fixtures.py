"""Freeze approved donor values and synthetic geometry from the Python original.

No application startup, settings, network or user documents are accessed.
"""
from pathlib import Path
import importlib
import json
import ezdxf
from generate_object_export_fixtures import reference
from generate_template_geometry_fixtures import signature

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "rust/tests/fixtures/profile-leader"


def donor_module():
    ref = reference()
    return importlib.import_module(ref.__package__ + ".autocad_profile_leader_donor")


def main():
    assert ezdxf.__version__ == "1.4.4"
    module = donor_module()
    OUT.mkdir(parents=True, exist_ok=True)
    data = ROOT / "rust/src/data/profile-leader-donor.json"
    data.parent.mkdir(exist_ok=True)
    data.write_text(json.dumps(module._donor_metadata_records(), ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
    ezdxf.options.write_fixed_meta_data_for_testing = True
    doc = ezdxf.new("R2018")
    doc.styles.new("NLCS-ISO", dxfattribs={"font": "isocp.ttf"})
    for app in ["AcDbBlockRepETag", "AcDbDynamicBlockTrueName", "AcDbDynamicBlockGUID", "KEEP_ME"]:
        doc.appids.add(app)
    module.ensure_profile_leader_geometry(doc)
    # Unrelated nested dictionaries and model/paper records must survive.
    circle = doc.modelspace().add_circle((100, 200, 3), 2)
    dictionary = circle.new_extension_dict()
    dictionary.add_dictionary("UNRELATED").add_xrecord("KEEP").extend([(1, "Ø 漢😀"), (70, 7)])
    doc.paperspace().add_mtext("Bewaren Ø 漢😀", dxfattribs={"insert": (10, 20)})
    doc.classes.add_required_classes(doc.dxfversion)
    doc.classes.classes = type(doc.classes.classes)(sorted(doc.classes.classes.items()))
    path = OUT / "leader.dxf"
    doc.saveas(path)
    path.write_bytes(path.read_bytes().replace(b"\r\n", b"\n"))
    pairs, _, _ = module.dv._read_pairs(path)
    all_records=module.dv._split_records(pairs)
    block_handle=doc.blocks.get(module.BLOCK_NAME).block_record_handle
    definition=[r for r in all_records if module.dv._record_type(r)=="BLOCK_RECORD" and module.dv._record_handle(r)==block_handle]
    geometry=[r for r in all_records if module.dv._record_owner(r)==block_handle and module.dv._record_type(r)!="BLOCK_RECORD"]
    assert len(definition)==1 and len(geometry)==6
    (data.parent / "profile-leader-geometry.json").write_text(json.dumps(dict(record=definition[0],geometry=geometry),ensure_ascii=False,indent=2)+"\n",encoding="utf-8",newline="\n")
    # Same envelope, before donor installation, for native install/readback tests.
    empty=ezdxf.readfile(path)
    empty.blocks.delete_block(module.BLOCK_NAME,safe=False)
    empty_path=OUT / "empty.dxf"
    empty.saveas(empty_path)
    empty_path.write_bytes(empty_path.read_bytes().replace(b"\r\n",b"\n"))
    block = doc.blocks.get(module.BLOCK_NAME)
    for entity in [block.block_record, *list(block)]:
        entity.set_xdata("KEEP_ME", [(1000, "Niet wijzigen Ø 漢😀"), (1070, 9)])
        entity.set_xdata("AcDbBlockRepETag", [(1070, 9), (1071, 99), (1005, "0")])
    block.block_record.set_xdata("AcDbDynamicBlockTrueName", [(1000, "Oude naam")])
    block.block_record.set_xdata("AcDbDynamicBlockGUID", [(1000, "Oude GUID")])
    path = OUT / "tagged.dxf"
    doc.saveas(path)
    path.write_bytes(b"\xef\xbb\xbf" + path.read_bytes().replace(b"\r\n", b"\n").replace(b"\n", b"\r\n"))
    patch=importlib.import_module(module.__package__+".template_dynamic_profile_leader_patch")
    exporter=patch.CadastralDxfExporter(wfs_client=None)
    doc=ezdxf.readfile(OUT / "leader.dxf")
    doc.layers.add(module.LAYER_NAME,color=7,dxfattribs={"lineweight":0})
    cases=[];expected=[]
    for index,scale in enumerate([-1.,0.,0.004,0.005,0.02,0.1,1.,2.5]):
        for color in [None,0,7,255,256]:
            request=dict(description=f"Water Ø{index} 漢😀",depth=f"-{index}.25 N.A.P.",start=[index*3.5-10,20-index*0.7],scale=scale,color=color)
            patch._add_dynamic_template_profile_leader(exporter,doc.modelspace(),request["description"],request["depth"],request["start"],(999,999),scale,color)
            cases.append(request);expected.append(signature(list(doc.modelspace())[-1]))
    (OUT / "references.json").write_text(json.dumps(dict(requests=cases,expected=expected),ensure_ascii=False,indent=2)+"\n",encoding="utf-8",newline="\n")
    print("Approved donor: 15 metadata records / 523 tags; install envelope and 40 original placements")


if __name__ == "__main__":
    main()
