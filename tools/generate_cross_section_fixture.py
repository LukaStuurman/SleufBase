"""Freeze local-span geometry from the original Python patch, no GUI/dependencies."""
import json
from math import hypot
from generate_marxact_alignment_fixtures import functions, ROOT


def main():
    ref=functions("marxact_local_cross_section_patch.py",
        ["_cross","_point_in_polygon","_line_polygon_parameters","local_cross_section_segment"],
        {"hypot":hypot,"_EPSILON":1e-9,"_INTERSECTION_TOLERANCE":1e-8})
    tapered=[[-2,-1],[2,-0.3],[2,0.3],[-2,1]]
    concave=[[-2,-2],[2,-2],[2,-0.5],[-0.5,-0.5],[-0.5,0.5],[2,0.5],[2,2],[-2,2]]
    cases=[]
    for polygon in [tapered,concave]:
        for origin in [[-1.5,0],[0,0],[1.5,0],[2.5,0],[-2,-1],[-1,1],[0,1],[0,-1],[0,2],[2,2]]:
            for normal in [[0,1],[1,0],[1,2],[0,0]]:
                for preferred in [0,1,-1]:
                    expected=ref.local_cross_section_segment(polygon,*origin,*normal,preferred_t=preferred)
                    cases.append(dict(polygon=polygon,origin=origin,normal=normal,preferred=preferred,expected=expected))
    output=ROOT/"rust/tests/fixtures/local_cross_sections.json"
    output.write_text(json.dumps(cases,separators=(",",":"),allow_nan=False)+"\n",encoding="utf-8")
    print(f"Generated {len(cases)} Python local-span cases: {output}")


if __name__=="__main__":
    main()
