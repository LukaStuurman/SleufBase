"""Independent full-tag and ezdxf audits of native template-preserving edits."""
from pathlib import Path
import json
import sys
import ezdxf

ROOT = Path(__file__).resolve().parents[1]


def records(path):
    lines = path.read_text(encoding="utf-8-sig").splitlines()
    assert len(lines) % 2 == 0
    result = []
    for code, value in zip(lines[::2], lines[1::2]):
        tag = (int(code), value)
        if tag[0] == 0:
            result.append([])
        result[-1].append(tag)
    return result


def main():
    output = Path(sys.argv[1])
    cases = json.loads((ROOT / "rust/tests/fixtures/templates/reference.json").read_text(encoding="utf-8"))
    for case in cases:
        source = ROOT / case["path"]
        unchanged = output / (case["name"] + "-unchanged.dxf")
        assert unchanged.read_bytes() == source.read_bytes(), case["name"]
        if case["error"]:
            continue
        expected = {}
        for i, slot in enumerate(case["slots"], 1):
            expected[slot["label_handle"]] = f"Proefsleuf {i}"
            if slot["address_handle"]:
                expected[slot["address_handle"]] = f"Adres\\PNieuw Ø {i} " + "漢😀" * 150
            if slot["comments_handle"]:
                expected[slot["comments_handle"]] = f"Opmerkingen\\PMeting {i}"
        edited = output / (case["name"] + "-edited.dxf")
        before, after = records(source), records(edited)
        assert len(before) == len(after)
        found = set()
        for old, new in zip(before, after):
            # Restrict changes to original text records named by the Python slot oracle.
            kind = old[0][1]
            handle = next((v for c, v in old if c == 5), None) if kind in {"TEXT", "MTEXT"} else None
            if handle not in expected:
                assert old == new, (case["name"], kind, handle)
                continue
            found.add(handle)
            codes = {1, 3} if kind == "MTEXT" else {1}
            assert [(c, v) for c, v in old if c not in codes] == [(c, v) for c, v in new if c not in codes], handle
            chunks = [(c, v) for c, v in new if c in codes]
            assert "".join(v for c, v in chunks) == expected[handle], handle
            if kind == "MTEXT":
                assert all(len(v) <= 250 for c, v in chunks)
                assert all(c == 3 for c, v in chunks[:-1]) and chunks[-1][0] == 1
        assert found == set(expected)
        original_doc, native_doc = ezdxf.readfile(source), ezdxf.readfile(edited)
        for handle, text in expected.items():
            entity = native_doc.entitydb[handle]
            assert (entity.text if entity.dxftype() == "MTEXT" else entity.dxf.text) == text
        assert original_doc.layouts.names() == native_doc.layouts.names()
        assert set(original_doc.blocks.block_names()) == set(native_doc.blocks.block_names())
        original_audit, native_audit = original_doc.audit(), native_doc.audit()
        assert not original_audit.has_errors and not native_audit.has_errors
        baseline = [(int(e.code), e.message) for e in original_audit.fixes]
        fixes = [(int(e.code), e.message) for e in native_audit.fixes]
        assert fixes == baseline, (case["name"], fixes, baseline)
        print(f"{case['name']}: {len(expected)} texts; all other tags preserved; {len(fixes)} baseline audit fixes")
    print("Native template preservation independently verified")


if __name__ == "__main__":
    main()
