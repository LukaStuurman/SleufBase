"""Compare actual native frame/band DXFs with the original complete drawer."""
from pathlib import Path
import json
import sys
import ezdxf
from generate_template_geometry_fixtures import signature
from verify_native_template_images import compare,verify_preservation

ROOT=Path(__file__).resolve().parents[1]


def main():
    reference_mode="--reference" in sys.argv
    body_mode=reference_mode or "--body" in sys.argv
    output=Path(sys.argv[1]);source=ROOT / "rust/tests/fixtures/profile-drawing/blocks.dxf"
    fixture=json.loads((ROOT / "rust/tests/fixtures/profile-drawing/reference.json").read_text(encoding="utf-8"))
    if "--reference-cases" in sys.argv:
        for i,case in enumerate(fixture["reference_cases"]):
            native=output / f"ref-{i}.dxf";doc=ezdxf.readfile(native);original=ezdxf.readfile(source)
            verify_preservation(source,native,original,allowed_extra={"CIRCLE","LWPOLYLINE","TEXT"})
            compare([signature(e) for e in doc.modelspace()],case["expected"],f"reference {i}")
            metadata=json.loads((output / f"ref-{i}.json").read_text(encoding="utf-8"))
            compare(metadata["reference"],{k:case[k] for k in ["raw","chainage","description"]},f"reference {i}.resolved")
            for key in ["display","segments","boxes"]:compare(metadata[key],case[key],f"reference {i}.{key}")
            audit=doc.audit();assert not audit.errors and not audit.fixes
        print(len(fixture["reference_cases"]),"original reference indicators, RD labels, rotations and obstacle sets verified")
        return
    cases=fixture["cases"]
    total=0
    for case in cases:
        native=output / (case["name"]+".dxf")
        original=ezdxf.readfile(source);doc=ezdxf.readfile(native)
        verify_preservation(source,native,original,allowed_extra={"INSERT","ATTRIB","SEQEND","LWPOLYLINE","TEXT","CIRCLE","BLOCK_RECORD","BLOCK","ENDBLK"} if body_mode else {"INSERT","ATTRIB","SEQEND","LWPOLYLINE"},allowed_tables=("LAYER","BLOCK_RECORD") if body_mode else ("LAYER",))
        actual=[signature(e) for e in doc.modelspace()]
        expected=case["body_expected"] if body_mode else case["expected"]
        if reference_mode and case["show_reference"]:
            index=case["reference_at"];expected=expected[:index]+case["reference_entities"]+expected[index:]
        compare(actual,expected,case["name"])
        metadata=json.loads((output / (case["name"]+".json")).read_text(encoding="utf-8"))
        assert len(metadata["bands"])==len(case["bands"])
        for a,e in zip(metadata["bands"],case["bands"]):
            compare({k:a[k] for k in e},e,case["name"]+".band")
        if body_mode:
            compare(metadata["fills"],case["fills"],case["name"]+".fills")
            compare(metadata["obstacles"],case["reference_obstacles"] if reference_mode else case["obstacles"],case["name"]+".obstacles")
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
    mode="body/reference" if reference_mode else "body/marker" if body_mode else "frame/band"
    print(f'{len(cases)} original {mode} comparisons, {total} native objects and all attributes verified')


if __name__=="__main__":main()
