"""Independent ezdxf title-block fixtures for the requested project/date fields."""
from datetime import datetime
from pathlib import Path
import importlib
import json
import ezdxf
from generate_object_export_fixtures import reference

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "rust/tests/fixtures/template-metadata"


def main():
    assert ezdxf.__version__ == "1.4.4"
    ref = reference()
    metadata = importlib.import_module(ref.__package__ + ".template_metadata")
    OUT.mkdir(parents=True, exist_ok=True)
    ezdxf.options.write_fixed_meta_data_for_testing = True
    doc = ezdxf.new("R2018")
    for name in ("Plain", "Numbered"):
        block = doc.blocks.new(name)
        block.add_attdef("PROJECTNAAM", (0, 0), text="definition stays")
    for name, block_name, tags in [
        ("Layout1", "Plain", [" projectomschrijving ", " datum ", "OMSCHRIJVING", "TEKENAAR", "DATUM", "OMSCHRIJVING", "TEKENAAR"]),
        ("Blad 2", "Numbered", ["GEM-PROJECT", "DATUM1", "DATUM2", "OMSCHRIJVING1", "OMSCHRIJVING2", "TEKENAAR1", "TEKENAAR2", "DATUM", "PROJECT.NR", "TEK-OMSCH-PRO-OPD"]),
        ("Unused", "Numbered", ["PROJECTNAAM", "DATUM1"]),
    ]:
        layout = doc.layouts.get(name) if name == "Layout1" else doc.layouts.new(name)
        for copy in range(2):
            insert = layout.add_blockref(block_name, (copy * 100, 0))
            for index, tag in enumerate(tags):
                insert.add_attrib(tag, f"original {index}", (0, index))
    insert = doc.modelspace().add_blockref("Numbered", (0, 0))
    insert.add_attrib("DATUM1", "model date")
    insert.add_attrib("GEM-PROJECT", "model project")
    doc.classes.add_required_classes(doc.dxfversion)
    doc.classes.classes = type(doc.classes.classes)(sorted(doc.classes.classes.items()))
    path = OUT / "sheets.dxf"
    doc.saveas(path)
    path.write_bytes(path.read_bytes().replace(b"\r\n", b"\n"))
    cases = []
    for name, source, layouts, project in [
        ("shipped", ROOT / "assets/cadastral_template.dxf", None, "  Project Ø – Zuid 漢😀  "),
        ("sheets", path, ["Layout1", "Blad 2", "Missing"], "Project Ø – Zuid 漢😀"),
        ("preserve-project", path, ["Blad 2"], None),
        ("clear-project", path, ["Layout1", "Blad 2"], ""),
    ]:
        doc = ezdxf.readfile(source)
        layouts = layouts or [layout.name for layout in doc.layouts if layout.name != "Model"]
        before = {e.dxf.handle: e.dxf.text for e in doc.entitydb.values() if e.dxftype() == "ATTRIB"}
        metadata.update_title_blocks(doc, layouts, drawn_by="  L. Test  ", project=project, export_date=datetime(2026, 10, 18, 23, 59))
        expected = {e.dxf.handle: e.dxf.text for e in doc.entitydb.values() if e.dxftype() == "ATTRIB" and before[e.dxf.handle] != e.dxf.text}
        cases.append(dict(name=name, path=source.relative_to(ROOT).as_posix(), layouts=layouts, project=project, drawn_by="  L. Test  ", date=[2026, 10, 18], expected=expected))
    (OUT / "reference.json").write_text(json.dumps(cases, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print([(c["name"], len(c["expected"])) for c in cases])


if __name__ == "__main__":
    main()
