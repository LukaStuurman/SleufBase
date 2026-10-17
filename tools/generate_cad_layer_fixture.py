"""Case-insensitive layer references from the primary ezdxf symbol-table API."""
import json
from pathlib import Path
import ezdxf
from ezdxf import colors

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "rust/tests/fixtures/cad-layers"


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    ezdxf.options.write_fixed_meta_data_for_testing = True
    doc = ezdxf.new("R2018")
    msp = doc.modelspace()
    for index, (name, reference, aci, visible) in enumerate([
        ("Water", "wATER", 5, True),
        ("Off", "oFF", 4, False),
        ("Électricité", "éLECTRICITÉ", 2, True),
        ("Данные", "дАННЫЕ", 3, True),
        ("ΟΣ", "ος", 6, True),
        ("İnfra", "i\u0307NFRA", 1, True),
        ("Straße", "sTRAßE", 30, True),
        ("STRASSE", "strasse", 150, True),
    ]):
        layer = doc.layers.new(name, dxfattribs={"color": aci})
        if not visible:
            layer.off()
        msp.add_line((index * 10, 0), (index * 10 + 1, 1), dxfattribs={"layer": reference})
    doc.layers.new("Root", dxfattribs={"true_color": 0x123456})
    block = doc.blocks.new("LayerCase")
    block.add_line((0, 0), (1, 0), dxfattribs={"layer": "wATER"})
    block.add_line((0, 1), (1, 1), dxfattribs={"layer": "0", "color": 0})
    block.add_line((0, 2), (1, 2), dxfattribs={"layer": "oFF"})
    msp.add_blockref("LayerCase", (168000, 401000), dxfattribs={"layer": "rOoT"})
    doc.classes.add_required_classes(doc.dxfversion)
    doc.classes.classes = dict(sorted(doc.classes.classes.items()))
    doc.saveas(OUT / "layers-ascii.dxf", fmt="asc")
    doc.saveas(OUT / "layers-binary.dxf", fmt="bin")
    doc = ezdxf.readfile(OUT / "layers-ascii.dxf")
    expected = []

    def walk(entities, parent=None, inherited_rgb=None, visible=True):
        for entity in entities:
            raw = entity.dxf.layer
            effective = parent if raw == "0" and parent is not None else raw
            layer = doc.layers.get(effective)
            aci = entity.dxf.color
            if aci == 0 and inherited_rgb is not None:
                rgb = inherited_rgb
            elif entity.dxf.hasattr("true_color"):
                rgb = list(entity.rgb)
            elif aci in (0, 256) and layer.dxf.hasattr("true_color"):
                rgb = list(layer.rgb)
            else:
                rgb = list(colors.aci2rgb(layer.color if aci in (0, 256) else aci))
                if rgb == [255, 255, 255]:
                    rgb = [220, 20, 60]
            shown = visible and not layer.is_off() and not layer.is_frozen()
            if entity.dxftype() == "INSERT":
                walk(entity.block(), layer.dxf.name, rgb, shown)
            else:
                expected.append(dict(handle=entity.dxf.handle, raw=raw, layer=layer.dxf.name, rgb=rgb, visible=shown))
    walk(doc.modelspace())
    (OUT / "expected.json").write_text(json.dumps(expected, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"Generated {len(expected)} layer references in ASCII and binary")


if __name__ == "__main__":
    main()
