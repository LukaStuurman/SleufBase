# Modelspace selection for map overlays

The original `dxf.py::load_dxf()` iterates `document.modelspace()`. Native
overlays now select the same root entities instead of drawing the entire
ENTITIES section, which also holds the active paper layout. Inactive paper
layout blocks are skipped before geometry/font preparation. Modelspace
content stored in its special BLOCK is promoted to roots in original order;
normal block definitions remain available for INSERT expansion.

Selection follows ezdxf 1.4.4 `sections/entities.py`: known model/active-paper
BLOCK_RECORD owners take priority over 67; otherwise 67 is the fallback.
Layout tab name 410 does not override these rules. APPDATA/reactor, XDATA
and embedded-object fields do not masquerade as owners, names or flags.
POLYLINE/VERTEX and INSERT/ATTRIB/SEQEND follow their parent, including
AcDbMInsertBlock. Normal block contents inherit the referencing root's space.
Primary references: [common entity codes](https://help.autodesk.com/cloudhelp/2023/ENU/AutoCAD-DXF/files/GUID-3610039E-27D1-4E23-B6D3-7E60B22BB5BD.htm),
[layout block records](https://help.autodesk.com/cloudhelp/2026/ESP/OARX-DevGuide-Managed/files/GUID-5FA86EF3-DEFD-4256-BB1C-56DAC32BD868.htm).

A bounded preliminary scan indexes only two space handles, ending at the
TABLES boundary for ordinary drawings. It also handles TABLES placed after
ENTITIES without retaining a second tag document. Cancellation/field budgets
apply. Skipped linked sequences still require SEQEND; malformed APPDATA,
duplicate layout blocks and repeated TABLES sections are explicit errors.
Ignored 1e100-radius paper circles are not tessellated, and excluded paper
MTEXT is not reported as missing map content. This is a component improvement,
not a total application memory/speed claim. Unreferenced normal block
definitions still pass through the existing bounded geometry decoder.

The 24 ASCII/binary fixtures span R12, R2000, R2007 and R2018, with **168
selected source entities**. Expected membership, original 2D feature paths
and map bounds come from AST-extracted production `dxf.py` functions plus
the original ezdxf loader. No application initialization, preferences,
credentials or installed fonts are read. Contradictory owner/67/410 values,
APPDATA shadows, child flags, moved model BLOCK content and late TABLES are
deliberate. Required class definitions are sorted; all 25 generated files
reproduce byte-for-byte under another hashseed.

`generate_cad_space_fixtures.py` regenerates the corpus; set
`SLEUFBASE_CAD_SPACE_FIXTURES` to target another directory.
`verify_native_cad_space.py` generates eight fresh drawings with shifted
coordinates, ASCII/binary encoding and reordered content, then compares
the optimized production reader via `render_cad_scene` summary mode.
The real egui canvas verifies model-only fitting and painted strokes.

New projects retain `space_policy=modelspace`; programmatically supplied
geometry defaults to `explicit`. Older cached overlays without the field
read as `legacy`: they retain all their data and show a source-reload notice.
The reader never guesses which old cached shapes were paper content or
silently replaces a saved snapshot from an external file. Reload/conversion
uses the corrected source reader; old project bytes stay unchanged on open.
This does not implement a paper-layout viewer, paper viewport clipping or
full CAD/project compatibility. All global PARITY.md gates remain required.
The ezdxf entity-section MIT notice is shipped with source and `--licenses`.
