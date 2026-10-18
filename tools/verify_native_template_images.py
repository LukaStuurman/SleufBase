"""Read native DXF/PNG pairs with ezdxf/Pillow; compare original image geometry."""
from pathlib import Path
import json
import sys
import ezdxf
from PIL import Image
from generate_template_image_fixtures import signature, pixels
from verify_native_template_edits import records

ROOT = Path(__file__).resolve().parents[1]


def compare(actual, expected, path):
    if isinstance(expected, (int, float)):
        assert abs(actual - expected) < 1e-9, (path, actual, expected)
    elif isinstance(expected, list):
        assert len(actual) == len(expected), path
        for i, (a, b) in enumerate(zip(actual, expected)):
            compare(a, b, f"{path}[{i}]")
    elif isinstance(expected, dict):
        assert actual.keys() == expected.keys(), path
        for key in expected:
            compare(actual[key], expected[key], path + "." + key)
    else:
        assert actual == expected, (path, actual, expected)


def identity(record):
    kind = record[0][1]
    if kind in {"SECTION", "TABLE"}:
        return kind, next(v for c, v in record if c == 2)
    if kind == "CLASS":
        return kind, next(v for c, v in record if c == 1)
    for c, v in record:
        if c == 100:
            break
        if c in {5, 105}:
            return kind, v
    return kind, None


def mask_seed(record):
    result, seed = [], False
    for c, v in record:
        if c == 9:
            seed = v == "$HANDSEED"
        result.append((c, "<seed>" if c == 5 and seed else v))
    return result


def dictionary(record):
    base, entries = [], []
    tags = iter(record)
    for c, v in tags:
        if c == 3:
            pointer = next(tags)
            assert pointer[0] in {350, 360}
            entries.append((v, pointer))
        else:
            base.append((c, v))
    return base, entries


def verify_preservation(source, native, original_doc, allowed_extra=(), allowed_tables=("LAYER",)):
    before, after = records(source), records(native)
    root = original_doc.rootdict.dxf.handle
    old_dictionary = original_doc.rootdict.get("ACAD_IMAGE_DICT")
    old_dictionary = old_dictionary.dxf.handle if old_dictionary is not None else None
    position = 0
    matched = set()
    for old in before:
        key = identity(old)
        while position < len(after) and identity(after[position]) != key:
            position += 1
        assert position < len(after), ("lost or reordered source record", key)
        new = after[position]
        matched.add(position)
        position += 1
        if key == ("SECTION", "HEADER"):
            assert mask_seed(old) == mask_seed(new)
        elif key[0] == "TABLE" and key[1] in allowed_tables:
            assert [(c, v) for c, v in old if c != 70] == [(c, v) for c, v in new if c != 70]
            if old != new:
                assert int(next(v for c,v in new if c==70))==sum(r[0]==(0,key[1]) for r in after), key
        elif key[0] == "CLASS" and key[1] in {"IMAGE", "IMAGEDEF", "IMAGEDEF_REACTOR", "RASTERVARIABLES"}:
            assert [(c, v) for c, v in old if c != 91] == [(c, v) for c, v in new if c != 91]
        elif key[0] == "DICTIONARY" and key[1] in {root, old_dictionary}:
            old_base, old_entries = dictionary(old)
            new_base, new_entries = dictionary(new)
            assert old_base == new_base and new_entries[:len(old_entries)] == old_entries, key
            if key[1] == root:
                assert all(k in {"ACAD_IMAGE_DICT", "ACAD_IMAGE_VARS"} for k, _ in new_entries[len(old_entries):])
        else:
            assert old == new, ("unexpected source change", key)
    allowed = {"CLASS", "LAYER", "IMAGE", "IMAGEDEF", "IMAGEDEF_REACTOR", "RASTERVARIABLES", "DICTIONARY"}
    assert all(r[0][1] in allowed | set(allowed_extra) for i, r in enumerate(after) if i not in matched)


def main():
    output = Path(sys.argv[1])
    cases = json.loads((ROOT / "rust/tests/fixtures/template-images/reference.json").read_text(encoding="utf-8"))
    for case in cases:
        source = ROOT / case["path"]
        manifests = json.loads((output / (case["name"] + "-manifest.json")).read_text(encoding="utf-8"))
        assert len(manifests) == 2 and manifests[0]["assets"] != manifests[1]["assets"]
        for manifest in manifests:
            original, native = ezdxf.readfile(source), ezdxf.readfile(manifest["drawing"])
            verify_preservation(source, Path(manifest["drawing"]), original)
            old_images = [e for e in original.entitydb.values() if e.dxftype() == "IMAGE"]
            images = [e for e in native.entitydb.values() if e.dxftype() == "IMAGE"]
            assert len(images) == len(old_images) + len(case["expected"])
            old_handles = {e.dxf.handle for e in old_images}
            images = [e for e in images if e.dxf.handle not in old_handles]
            images = sorted(images, key=lambda e: Path(native.entitydb[e.dxf.image_def_handle].dxf.filename).name)
            image_dict = native.rootdict["ACAD_IMAGE_DICT"]
            for index, (entity, expected, request) in enumerate(zip(images, case["expected"], case["requests"])):
                compare(signature(native, entity), expected, f"{case['name']}.{index}")
                definition = native.entitydb[entity.dxf.image_def_handle]
                reactor = native.entitydb[entity.dxf.image_def_reactor_handle]
                assert definition.dxf.owner == image_dict.dxf.handle
                assert reactor.dxf.owner == entity.dxf.handle
                assert reactor.dxf.image_handle == entity.dxf.handle
                assert set(definition.get_reactors()) == {image_dict.dxf.handle, reactor.dxf.handle}
                assert tuple(definition.dxf.image_size)[:2] == (request["width"], request["height"])
                assert all(len(tuple(v)) < 3 or tuple(v)[2] == 0 for v in [definition.dxf.image_size, definition.dxf.pixel_size])
                assert native.layouts.get_layout_by_key(entity.dxf.owner).name == (request["layout"] or "Model")
                image_path = Path(definition.dxf.filename)
                assert image_path.is_absolute() and image_path.parent == Path(manifest["assets"])
                assert not str(image_path).startswith("\\\\?\\")
                with Image.open(image_path) as image:
                    assert image.size == (request["width"], request["height"])
                    assert list(image.convert("RGBA").getdata()) == pixels(*image.size, request["index"])
            # ezdxf can synthesize missing SEQEND records on load. HANDSEED
            # must exceed the serialized handles, not those new in-memory IDs.
            handles = []
            for record in records(Path(manifest["drawing"])):
                if record[0][1] == "SECTION":
                    continue
                for code, value in record:
                    if code == 100:
                        break
                    if code in {5, 105}:
                        handles.append(int(value, 16))
                        break
            assert int(native.header["$HANDSEED"], 16) > max(handles)
            for cls in native.classes:
                if cls.dxf.name in {"IMAGE", "IMAGEDEF", "IMAGEDEF_REACTOR", "RASTERVARIABLES"}:
                    assert cls.dxf.instance_count == sum(e.dxftype() == cls.dxf.name for e in native.entitydb.values())
            baseline, audit = original.audit(), native.audit()
            assert not baseline.has_errors and not audit.has_errors
            assert [(int(e.code), e.message) for e in audit.fixes] == [(int(e.code), e.message) for e in baseline.fixes]
        print(f"{case['name']}: {len(case['expected'])} original image placements, two permanent bundles and all PNG pixels verified")
    print("Native template image bundles independently verified")


if __name__ == "__main__":
    main()
