# Original synthetic SHP/SHX fonts

These fonts were written specifically for SleufBase's regression corpus. They
contain synthetic vectors, rectangles, arcs, subshapes and Unicode glyphs; no
installed/commercial CAD fonts are redistributed. Generate with ezdxf 1.4.4:
`tools/generate_stroke_font_fixtures.py` (Python 3.11).

The independently parsed SHP, legacy SHX 1.0/1.1 and Unicode SHX/SHP versions
provide **2,486 original control traces** and **990 original text extents**.
Curved A/x/p programs also exercise raw-control metrics. Decimal font headers,
leading-0 hexadecimal glyph codes, parentheses, comments and continued lines are
parsed separately. Traces cover all sixteen vector directions and all defined opcodes 0–14,
scaling, pen moves, four stack positions, shared subshape state, Unicode
subshape IDs, positive/negative/full/fractional arcs, multi-displacements,
bulges and vertical conditional commands. Text cases cover all 15 alignments,
FIT/ALIGNED, OCS, mirror, oblique, rotation, missing glyph/space fallbacks,
font-header and derived A/x/p metrics. Compare raw cubic controls, not flattened
pixels; tolerance is 1e-11 + 64 f64 ULP factors per scalar.

Separate hand-calculated regressions retain the original interpreter's output
and test two fixes against Autodesk's shape-code specification: nested code 7
preserves the pen state, and code 14 skips just the next command (including
another code 14). A third fixture proves that raw legacy font metadata with
cap-height 12 must not be interpreted as a bulge instruction. Missing control
glyphs use the original cached space-width fallback without its first-use IndexError.

`verify_native_stroke_fonts.py` builds eight fresh curved fonts at runtime and
compares 120 compiled text extents with the original functions, without saved
expected data. The paired exporter audit also uses a real SHX profile style and
checks its automatic grip with the original code.

The native interpreter bounds source bytes, normalized program instructions,
glyph controls, recursion/cycles, stack depth and advance-cache entries. Bad
fonts fail explicitly; they do not return plausible fabricated geometry.
This implements SHP/SHX glyph geometry and horizontal fast text measurement
for template selectors. It does not yet render SHX strokes in the map UI;
LFF metrics are now implemented separately (see lff-font). BIGFONT/error
fallback compatibility, viewport text and broader CAD/MTEXT remain release gates.

Protocol and measurements were ported from ezdxf's MIT-licensed shapefile
module; retain `rust/third_party/ezdxf-LICENSE.txt`. Shape command specification:
https://help.autodesk.com/cloudhelp/2023/ENU/AutoCAD-Customization/files/GUID-06832147-16BE-4A66-A6D0-3ADF98DC8228.htm
