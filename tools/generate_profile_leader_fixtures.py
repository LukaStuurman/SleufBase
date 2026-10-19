"""Freeze approved donor values and synthetic geometry from the Python original.

No application startup, settings, network or user documents are accessed.
"""
from pathlib import Path
import importlib
import json
import ezdxf
from generate_object_export_fixtures import reference

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
    block = doc.blocks.get(module.BLOCK_NAME)
    for entity in [block.block_record, *list(block)]:
        entity.set_xdata("KEEP_ME", [(1000, "Niet wijzigen Ø 漢😀"), (1070, 9)])
        entity.set_xdata("AcDbBlockRepETag", [(1070, 9), (1071, 99), (1005, "0")])
    block.block_record.set_xdata("AcDbDynamicBlockTrueName", [(1000, "Oude naam")])
    block.block_record.set_xdata("AcDbDynamicBlockGUID", [(1000, "Oude GUID")])
    path = OUT / "tagged.dxf"
    doc.saveas(path)
    path.write_bytes(b"\xef\xbb\xbf" + path.read_bytes().replace(b"\r\n", b"\n").replace(b"\n", b"\r\n"))
    print("Approved donor: 15 metadata records / 523 tags; two original geometry fixtures")


if __name__ == "__main__":
    main()
