"""Compare actual native frame/band DXFs with the original complete drawer."""
from pathlib import Path
import json
import sys
import ezdxf
from generate_template_geometry_fixtures import signature
from verify_native_template_images import compare,verify_preservation

ROOT=Path(__file__).resolve().parents[1]


def main():
    output=Path(sys.argv[1]);source=ROOT / "rust/tests/fixtures/profile-drawing/blocks.dxf"
    cases=json.loads((ROOT / "rust/tests/fixtures/profile-drawing/reference.json").read_text(encoding="utf-8"))["cases"]
    total=0
    for case in cases:
        native=output / (case["name"]+".dxf")
        original=ezdxf.readfile(source);doc=ezdxf.readfile(native)
        verify_preservation(source,native,original,allowed_extra={"INSERT","ATTRIB","SEQEND","LWPOLYLINE"})
        actual=[signature(e) for e in doc.modelspace()]
        compare(actual,case["expected"],case["name"])
        metadata=json.loads((output / (case["name"]+".json")).read_text(encoding="utf-8"))
        assert len(metadata["bands"])==len(case["bands"])
        for a,e in zip(metadata["bands"],case["bands"]):
            compare({k:a[k] for k in e},e,case["name"]+".band")
        for insert in doc.modelspace().query("INSERT"):
            assert insert.dxf.owner==doc.modelspace().layout_key
            assert all(a.dxf.owner==insert.dxf.handle for a in insert.attribs)
        baseline=original.audit();audit=doc.audit()
        assert not audit.errors and [(e.code,e.message) for e in audit.fixes]==[(e.code,e.message) for e in baseline.fixes],case["name"]
        total+=len(actual)
    print(f'{len(cases)} complete original frame/band comparisons, {total} native objects and all attributes verified')


if __name__=="__main__":main()
