# CAD surface control-cage reference

Regenerate with Python 3.11 and ezdxf **1.4.4**:

    python tools/generate_mesh_fixtures.py

The real ASCII/binary R2018 DXFs contain 28 sources, each repeated under nested
nonuniform/reflected/OCS block transforms: 56 world cases and 278 visible edges.
They cover all sixteen 3DFACE visibility masks, a triangle, OCS SOLID/TRACE,
polyface signed one-based indices and mixed record ordering, advisory polyface
header counts, both polygon-grid closure directions, MESH n-gons, explicit edges,
crease weights, subdivision levels, and retained vertex/face appearance/XDATA.

The reference reads ezdxf's source entities and matrix44 transformations without
calling Rust. The comparison concerns visible control-cage edges, not shaded or
subdivided surfaces. It removes coincident zero-length edges and duplicate edges.
The grid uses ezdxf's public row/column access and documented closure flags.

3DFACE edge bits address all four source corners. In ezdxf 1.4.4,
`Face3d.wcs_vertices()` collapses a duplicated fourth corner of a triangle despite
its docstring; applying the edge bits after that collapse changes the closing
edge's visibility. This oracle uses the four raw WCS corners and source masks
instead. A zero-length third edge does not move the fourth edge's bit.

Primary definitions: [Autodesk VERTEX](https://help.autodesk.com/cloudhelp/2018/ENU/AutoCAD-DXF/files/GUID-0741E831-599E-4CBF-91E1-8ADBCFD6556D.htm),
[POLYLINE](https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-DXF/files/GUID-ABF6B778-BE20-4B49-9B58-A94E64CEFFF3.htm),
[polyface ordering/counts](https://help.autodesk.com/cloudhelp/2015/ENU/AutoCAD-DXF/files/GUID-96B6288E-F413-46C0-968A-A314171C0AAE.htm),
[MESH lists](https://help.autodesk.com/cloudhelp/2020/ENU/AutoCAD-DXF/files/GUID-4B9ADA67-87C8-4673-A579-6E4C76FF7025.htm),
[ezdxf SOLID](https://ezdxf.readthedocs.io/en/stable/dxfentities/solid.html),
[TRACE](https://ezdxf.readthedocs.io/en/stable/dxfentities/trace.html),
and [3DFACE](https://ezdxf.readthedocs.io/en/stable/dxfentities/3dface.html).
