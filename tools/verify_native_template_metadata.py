"""Read native output independently; allow only requested ATTRIB text changes."""
from datetime import datetime
from pathlib import Path
import json
import sys
import ezdxf
from verify_native_template_edits import records

ROOT = Path(__file__).resolve().parents[1]


def main():
    output = Path(sys.argv[1])
    cases = json.loads((ROOT / "rust/tests/fixtures/template-metadata/reference.json").read_text(encoding="utf-8"))
    for case in cases:
        source = ROOT / case["path"]
        actual_path = output / (case["name"] + ".dxf")
        original, actual = records(source), records(actual_path)
        assert len(original) == len(actual)
        for before, after in zip(original, actual):
            handle = next((v for c, v in before if c in (5, 105)), None)
            wanted = [(c, case["expected"][handle] if c == 1 else v) for c, v in before] if handle in case["expected"] else before
            assert after == wanted, (case["name"], handle)
        doc = ezdxf.readfile(actual_path)
        for handle, value in case["expected"].items():
            assert doc.entitydb[handle].dxf.text == value
        audit = doc.audit()
        baseline = ezdxf.readfile(source).audit()
        assert not audit.errors and [e.code for e in audit.fixes] == [e.code for e in baseline.fixes]
        print(f'{case["name"]}: {len(case["expected"])} title fields; all other source tags preserved')
    if sys.platform == "win32":
        assert (output / "local-date.txt").read_text() == datetime.now().strftime("%d-%m-%Y")
    print("Native project/date fields independently verified")


if __name__ == "__main__":
    main()
