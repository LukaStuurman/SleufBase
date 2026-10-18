# Original standalone KickTheMap object-export reference

Regenerate with Python 3.11, ezdxf **1.4.4**, numpy and Pillow available for the
original data models:

    python tools/generate_object_export_fixtures.py

The generator imports the original `kickthemap_dxf_export.py` under an isolated
package name without starting the application. The production Rust executable
does not use Python. Fourteen complete drawing scenarios cover isolated and
coincident points, point-only direction, reversed/vertical measurements,
interleaved objects, polyline-only data, optional Z, Unicode names, empty jobs,
options clamping, overlapping/fractional job bounds, endpoint order and CLI input.
Eighty-seven job-label cases cover all Unicode 14 decimal digit sets and Python
word boundaries, including combining marks, underscores, long s and leading zeroes.

`cases.json` freezes modelspace entities, ordered coordinates/XDATA, label
positions/alignment/height/style/true-color, layer declarations, font and units
from real Python-exported R2018 documents. The native `object_export` tests use
these values. A separate Python reader checks native files with ezdxf and requires
both `audit.errors` and `audit.fixes` to be empty. CI also runs the actual native
CLI and checks measured trench/marker, cadastral text and BGT output. Independent
auditing reads full documents, not just native self-generated expectations.

An intentional correction is explicit in `native_layers`: the original new-layer
call passes color in `dxfattribs`, which ezdxf's `layers.add` replaces with its
default color 7. A subsequent update can then change it. Native layers always
declare the requested rule color; individual entity colors remain equal to Python.
Nonfinite geometry, illegal layer names, line/control-character injection and
overlong XDATA return errors. They never produce a partial published drawing.
The writer also preserves f64 precision instead of rounding to eight decimals.

The frozen `object_dxf_base.txt` is an empty structural R2018 envelope generated
by the same script. It provides registered application/style tables and model/
paper-space ownership/dictionaries; Rust fills only reviewed layer/entity slots
and header extents/counts. Volatile header metadata is omitted. CLASS records are explicitly sorted so Python hash seeds do not change the envelope. The CLI case uses the original app's configured 3.5-metre gap, while bare-encoder cases retain their explicit/default options. New handles start
at `0x1000`, above every envelope handle. This is not a user template reader and
does not prove profile/template, raster bundle or arbitrary CAD-entity parity.

Primary format definitions: [DXF group value types / UTF-8](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-2553CF98-44F6-4828-82DD-FE3BC7448113.htm),
[255-byte extended-data strings](https://help.autodesk.com/cloudhelp/2025/ENU/AutoCAD-DXF/files/GUID-A2A628B0-3699-4740-A215-C560E7242F63.htm),
and [3D POLYLINE flags](https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-DXF/files/GUID-ABF6B778-BE20-4B49-9B58-A94E64CEFFF3.htm).
