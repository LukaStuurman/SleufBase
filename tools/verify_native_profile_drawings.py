"""Compare actual native frame/band DXFs with the original complete drawer."""
from pathlib import Path
import json
import sys
import ezdxf
from generate_template_geometry_fixtures import signature
from verify_native_template_images import compare,verify_preservation

ROOT=Path(__file__).resolve().parents[1]


def main():
    body_mode="--body" in sys.argv
    output=Path(sys.argv[1]);source=ROOT / "rust/tests/fixtures/profile-drawing/blocks.dxf"
    cases=json.loads((ROOT / "rust/tests/fixtures/profile-drawing/reference.json").read_text(encoding="utf-8"))["cases"]
    total=0
    for case in cases:
        native=output / (case["name"]+".dxf")
        original=ezdxf.readfile(source);doc=ezdxf.readfile(native)
        verify_preservation(source,native,original,allowed_extra={"INSERT","ATTRIB","SEQEND","LWPOLYLINE","TEXT","CIRCLE","BLOCK_RECORD","BLOCK","ENDBLK"} if body_mode else {"INSERT","ATTRIB","SEQEND","LWPOLYLINE"},allowed_tables=("LAYER","BLOCK_RECORD") if body_mode else ("LAYER",))
        actual=[signature(e) for e in doc.modelspace()]
        compare(actual,case["body_expected"] if body_mode else case["expected"],case["name"])
        metadata=json.loads((output / (case["name"]+".json")).read_text(encoding="utf-8"))
        assert len(metadata["bands"])==len(case["bands"])
        for a,e in zip(metadata["bands"],case["bands"]):
            compare({k:a[k] for k in e},e,case["name"]+".band")
        if body_mode:
            compare(metadata["fills"],case["fills"],case["name"]+".fills")
            compare(metadata["obstacles"],case["obstacles"],case["name"]+".obstacles")
            marker="SAL-VERWIJZING_PROFIELPUNT-MARKER-SOD"
            assert (marker in doc.blocks)==(case["marker_definition"] is not None)
            if case["marker_definition"] is not None:
                compare([signature(e) for e in doc.blocks[marker]],case["marker_definition"],case["name"]+".marker-definition")
        for insert in doc.modelspace().query("INSERT"):
            assert insert.dxf.owner==doc.modelspace().layout_key
            assert all(a.dxf.owner==insert.dxf.handle for a in insert.attribs)
        baseline=original.audit();audit=doc.audit()
        assert not audit.errors and [(e.code,e.message) for e in audit.fixes]==[(e.code,e.message) for e in baseline.fixes],case["name"]
        total+=len(actual)
    print(f'{len(cases)} original {"body/marker" if body_mode else "frame/band"} comparisons, {total} native objects and all attributes verified')


if __name__=="__main__":main()
