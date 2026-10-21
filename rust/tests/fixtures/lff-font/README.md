# Native LibreCAD LFF metrics

All six fonts here are original synthetic test programs. No installed or
commercial fonts are redistributed. `tools/generate_lff_font_fixtures.py`
uses ezdxf 1.4.4 (Python 3.11) as an independent parser/geometry/metric oracle.
The format is documented by the [LibreCAD project](https://github.com/Rallaz/LibreCAD/wiki/lff-definition).

The corpus contains **71 glyph control traces and 360 complete text extents**.
It covers header spacing and invalid-property rules, Unicode/hex identifiers,
invalid coordinate coercion, ignored extra dimensions, disconnected/empty
polylines, positive/negative/large/tiny bulges, parent inheritance, missing parents
and redefining a parent after existing children. Inheritance refers to immutable
snapshots, so an existing child's contours never change when its parent code is
redefined. Raw controls and command counts are compared, not flattened pixels.
All 15 TEXT alignments, OCS, height/width, rotation, oblique/mirror, missing glyphs,
spaces, control characters and empty A/x/p metrics are exercised. Numeric budget:
1e-11 + 64 f64 epsilon factors per scalar; corners are point sets.

Separate regressions preserve the old `fallback_lff()` typo: it returns no font
despite an available `standard.lff`, or selects `txt.shx` when one is indexed.
Native code uses the declared LFF priority: standard, iso, simplex. It never
attempts to parse that SHX file as LFF. This is an explicit correction rather
than silently changing an original expected result.

Parent nodes and contour lists are shared rather than repeatedly copying large
lists into each child. A test gives 1,000 children a parent with 2,000 separate
polylines and bounds retained program storage below 512 KiB; this is a structural
storage check, not a total application RSS/speed claim. Glyph geometry is temporary
for width calculation; at most 8,192 advances are cached. Source/definition/vertex,
inheritance-depth, rendered-point, per-glyph and per-text work budgets are checked.
Ignored polylines count as work without allocating temporary paths. Cancellation
and invalid/nonfinite font data produce explicit errors.

The independent executable audit creates eight fresh curved/inherited fonts and
compares 120 text extents directly against original functions, without saved
expected data. The real paired-export audit also changes the profile style to a
local LFF font and checks the automatic grip with the original code. CLI and both
native dialogs use the same local font catalog and saved support folders.

Glyph metrics/controls are available for selectors; viewport stroke rendering,
Further font/vendor variants, broader CAD/MTEXT and actual CAD application
acceptance remain open. All global `rust/PARITY.md` release gates still apply.

Protocol, path and metric semantics were ported from ezdxf's MIT-licensed LFF and
path/math helpers. Retain `rust/third_party/ezdxf-LICENSE.txt`, also included in
`--licenses` and the Windows artifacts. The seven generated files reproduce
byte-for-byte under different hashseeds; set `SLEUFBASE_LFF_FONT_FIXTURES` to write
references into an independent directory.
