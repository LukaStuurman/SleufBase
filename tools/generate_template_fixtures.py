"""Original cadastral slot detection on the shipped template and synthetic DXFs."""
from dataclasses import asdict
from pathlib import Path
import importlib
import json
import ezdxf
from generate_object_export_fixtures import reference

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "rust/tests/fixtures/templates"


def main():
    assert ezdxf.__version__ == "1.4.4"
    ref = reference()
    module = importlib.import_module(ref.__package__ + ".cadastral_export")
    exporter = module.CadastralDxfExporter(wfs_client=None)
    ezdxf.options.write_fixed_meta_data_for_testing = True
    OUT.mkdir(parents=True, exist_ok=True)
    cases = []

    def record(name, path):
        doc = ezdxf.readfile(path)
        try:
            slots = [asdict(slot) for slot in exporter._detect_template_slots(doc)]
            error = False
        except module.CadastralExportError:
            slots, error = [], True
        cases.append(dict(name=name, path=path.relative_to(ROOT).as_posix(), slots=slots, error=error))

    record("shipped", ROOT / "assets/cadastral_template.dxf")
    for variant in ["standard", "paragraph-prefix", "escaped-prefix", "missing-optional", "empty"]:
        doc = ezdxf.new("R2018")
        for layer in ["KADER", "Hulplijn kader"]:
            doc.layers.add(layer)
        model = doc.modelspace()

        def box(layout, x, y, width, height, layer):
            layout.add_lwpolyline([(x, y), (x + width, y), (x + width, y + height), (x, y + height)], close=True, dxfattribs={"layer": layer})

        if variant != "empty":
            for x in [0.0, 30.0, 60.0]:
                for y in [0.0, -3.0, -12.0]:
                    box(model, x, y - 0.6, 29.0, 2.75, "KADER")
                    box(model, x + 24.0, y, 2.1, 1.6, "Hulplijn kader")
                    box(model, x + 26.5, y - 0.1, 2.5, 1.8, "Hulplijn kader")
                    title = r"\U+0050roefsleuf" if variant == "escaped-prefix" else "Proefsleuf"
                    model.add_text(title, dxfattribs={"layer": "KADER", "height": 0.1, "insert": (x + 26.3, y + 1.8)})
                    if variant != "missing-optional":
                        prefix = r"\P" if variant == "paragraph-prefix" else ""
                        address = r"\U+0041dres" if variant == "escaped-prefix" else "Adres"
                        model.add_mtext(prefix + address + r"\Pvervolg", dxfattribs={"layer": "KADER", "insert": (x + 24.1, y + 0.7), "char_height": 0.1})
                        model.add_mtext(prefix + "Opmerkingen", dxfattribs={"layer": "0", "insert": (x + 26.3, y - 0.3), "char_height": 0.1})
            # Same geometry in paperspace must never produce extra slots.
            paper = doc.layouts.get("Layout1")
            box(paper, 0.0, 0.0, 2.1, 1.6, "Hulplijn kader")
            box(paper, 2.5, -0.1, 2.5, 1.8, "Hulplijn kader")
            paper.add_text("Proefsleuf paper", dxfattribs={"layer": "KADER", "height": 0.1})
        doc.classes.add_required_classes(doc.dxfversion)
        doc.classes.classes = type(doc.classes.classes)(sorted(doc.classes.classes.items()))
        path = OUT / (variant + ".dxf")
        doc.saveas(path)
        # DXF fixtures are marked -text in git; pin bytes across Windows/Linux.
        path.write_bytes(path.read_bytes().replace(b"\r\n", b"\n"))
        record(variant, path)
    (OUT / "reference.json").write_text(json.dumps(cases, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print([(c["name"], len(c["slots"]), c["error"]) for c in cases])


if __name__ == "__main__":
    main()
