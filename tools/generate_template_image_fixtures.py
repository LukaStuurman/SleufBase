"""Original _add_box_image geometry/ownership references, no native code."""
from dataclasses import asdict
from pathlib import Path
import importlib
import json
import tempfile
import ezdxf
from PIL import Image
from generate_object_export_fixtures import reference

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "rust/tests/fixtures/template-images"


def pixels(width, height, index):
    return [((x * 7 + index) % 256, (y * 11 + index * 3) % 256, ((x + y) * 13) % 256, 80 + (x * 3 + y * 5) % 176) for y in range(height) for x in range(width)]


def signature(doc, entity):
    definition = doc.entitydb[entity.dxf.image_def_handle]
    image_dict = doc.rootdict["ACAD_IMAGE_DICT"]
    name = next(key for key, value in image_dict.items() if value is definition)
    return dict(name=name, filename=Path(definition.dxf.filename).name, insert=list(entity.dxf.insert), u_pixel=list(entity.dxf.u_pixel), v_pixel=list(entity.dxf.v_pixel), image_size=list(entity.dxf.image_size), layer=entity.dxf.layer, flags=entity.dxf.flags, paperspace=entity.dxf.paperspace, clipping=entity.dxf.clipping, brightness=entity.dxf.brightness, contrast=entity.dxf.contrast, fade=entity.dxf.fade, clipping_boundary=[list(p) for p in entity.boundary_path], pixel_size=list(definition.dxf.pixel_size)[:2], loaded=definition.dxf.loaded, resolution_units=definition.dxf.resolution_units)


def main():
    assert ezdxf.__version__ == "1.4.4"
    OUT.mkdir(parents=True, exist_ok=True)
    ref = reference()
    module = importlib.import_module(ref.__package__ + ".cadastral_export")
    exporter = module.CadastralDxfExporter(wfs_client=None)
    ezdxf.options.write_fixed_meta_data_for_testing = True
    existing = ezdxf.readfile(ROOT / "rust/tests/fixtures/templates/standard.dxf")
    exporter._ensure_template_image_layer(existing)
    definition = existing.add_image_def("existing.png", (40, 20), name="kaart")
    existing.modelspace().add_image(definition, (100, 100), (4, 2))
    existing.set_raster_variables(frame=1, quality=0, units="mm")
    existing.classes.add_required_classes(existing.dxfversion)
    existing.classes.classes = type(existing.classes.classes)(sorted(existing.classes.classes.items()))
    path = OUT / "existing-images.dxf"
    existing.saveas(path)
    path.write_bytes(path.read_bytes().replace(b"\r\n", b"\n"))
    cases = []
    for name, path in [("shipped", ROOT / "assets/cadastral_template.dxf"), ("standard", ROOT / "rust/tests/fixtures/templates/standard.dxf"), ("existing", path)]:
        document = ezdxf.readfile(path)
        exporter._ensure_template_image_layer(document)
        slots = exporter._detect_template_slots(document)
        requests, expected = [], []
        with tempfile.TemporaryDirectory() as directory:
            index = 0
            boxes = [(box, None) for slot in slots for box in [slot.tiff_box, slot.map_box]]
            paper = next(name for name in document.layouts.names() if name != "Model")
            boxes.append((module.Bounds(0, 0, 100, 50), paper))
            for box, layout in boxes:
                    dimensions = [(40, 20), (20, 40), (80, 30), (20, 40), (40, 20), (20, 40)][index % 6]
                    inset, preserve = [(0.04, True), (0.04, True), (0.1, False), (0.0, False), (3.0, True), (-0.1, True)][index % 6]
                    filename = f"raster{index:04}.png"
                    image = Image.new("RGBA", dimensions)
                    image.putdata(pixels(*dimensions, index))
                    image_path = Path(directory) / filename
                    image.save(image_path)
                    requests.append(dict(filename=filename, width=dimensions[0], height=dimensions[1], index=index, bounds=asdict(box), definition_name="kaart", inset=inset, preserve_aspect=preserve, layout=layout))
                    target = document.layouts.get(layout) if layout is not None else document.modelspace()
                    exporter._add_box_image(document, target, image_path, box, "kaart", inset=inset, preserve_aspect=preserve)
                    expected.append(signature(document, list(target.query("IMAGE"))[-1]))
                    index += 1
        cases.append(dict(name=name, path=path.relative_to(ROOT).as_posix(), requests=requests, expected=expected))
    (OUT / "reference.json").write_text(json.dumps(cases, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print([(c["name"], len(c["expected"])) for c in cases])


if __name__ == "__main__":
    main()
