# Native vector CAD TEXT

675 independent ezdxf 1.4.4 `addons.text2path.make_paths_from_entity`
references compare every transformed line/quadratic/cubic control point.
Nine font modes use eight original redistributable TTF/SHP/SHX/Unicode/LFF
fixtures plus explicit test monospace. No commercial or installed font is
copied. Cases cover all 15 alignments, FIT/ALIGNED, oblique, backward/upside-down
flags, width, rotation, four OCS normals, percent/caret escapes, missing glyphs,
spaces and empty strings. Comparison budget: `1e-10 + 128*EPSILON*magnitude`.
These are actual glyph contours, not the separate fast-bbox rectangle contract.

ASCII and binary DXFs exercise 25 model/nested text instances. Independent
control geometry samples verify at most 0.005 drawing units of world-space
chord error after nonuniform/reflected/rotated/extruded INSERT/MINSERT chains.
Layer-0/by-block color inheritance, visibility, bounds, pen-up path breaks,
real map strokes and actual egui canvas painting are checked. Native project
roundtrip preserves font-independent contours after deleting the temporary
font files; visibility changes keep the shared block cache.

FontTools' TrueType LSB-vs-glyf.xMin correction and baseline removal are
preserved. Stroke-font pen moves remain distinct from ink. Coincident glyph
end/start points follow original concatenation. Glyph cache retention is
bounded by 8 MiB / 8192 entries, drawing control geometry by two million
points, scene points by the existing ten-million budget and uncached glyph
work by twenty million operations. An adversarial SHP with many aliases of
an expensive, invisible subshape checks that zero output cannot evade the
work limit. These are component budgets, not total process-RAM/performance
claims. Cancellation and invalid-project preservation are tested.

`tools/generate_cad_text_fixtures.py` reproduces JSON and both DXFs with Python
3.11, ezdxf 1.4.4 and FontTools 4.67.0. Set `SLEUFBASE_CAD_TEXT_FIXTURES` to
regenerate elsewhere. All three files reproduce byte for byte under another
hashseed. `verify_native_cad_text.py` generates nine fresh TTF/Unicode-SHX/LFF
fonts and 270 new vector cases, and independently compares the optimized
`render_cad_text` driver without saved expected output.

TEXT now imports STYLE font references and reaches the native canvas and
template-map stroke pipeline. Import discovers system and drawing-folder
fonts; callers can supply a bounded catalog with extra support directories.
Resolved contours are saved so reopening does not require those files.
Vertical styles and auxiliary BIGFONT receive explicit partial-support
warnings. The current display is vector outlines/strokes: TrueType fill,
ATTRIB/ATTDEF drawing, MTEXT layout, full settings integration for import and
interactive AutoCAD/real-project acceptance remain open. Nothing here proves
all CAD text or complete application parity. MIT notice for text2path is in
the source distribution, `--licenses`, and Windows preview artifacts.
