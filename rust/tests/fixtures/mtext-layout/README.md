# Native measured-cell MTEXT placement

This component ports the complete placement path used by ezdxf 1.4.4
`tools/text_layout.py`: text/fraction/space/nonbreaking-space/tab cells,
normalization and rigid connections, left/right/center/justified lines,
tab locking, capped glue growth, paragraph margins/indents/leading, empty
paragraphs, column continuation/explicit breaks/cloning and all nine layout
attachment positions. Text metrics are supplied by the caller. The result is
an ordered plan of text frames, background frames and fraction/decoration
lines, plus complete placement diagnostics. It does not call Python.

**369 original cases / 8747 primitives / 66 original errors** compare every
rectangle, offset, locked cell, nested fraction/rigid child, used column height,
paragraph spacing, line width and rendering order. The oracle executes the
actual original engine and renderer callbacks. The only instrumentation carries
a diagnostic source id into a split paragraph; no placement arithmetic is
replaced. Cases cover exact-width differences between line classes, ordinary
and fixed columns, unequal widths/margins, overflowing cells, blank paragraphs
that exceed a fixed column, mixed-height alignments, all fraction modes,
continued decorations, fixed/relative tab positions and unsorted/duplicate
stops. Current original column-cloning and line-spacing rules are retained.
Finite arithmetic and explicit work/node/column/depth/event/primitive budgets
prevent unbounded work or invalid geometry; cancellation returns no partial
plan. Background frames are layout boxes, not clipping boundaries.

Raw normalized cells use a shared arena/range across column continuations.
Column height uses cached prefix sums with the original addition grouping,
avoiding rescans of all earlier paragraphs. Tab tables are indexed once per
physical shared source/indent/width. Sorted positions plus suffix-minimum
source indices preserve the original first eligible tab even for unsorted
mixed tab types. Ordinary repeated lookups do not scan every tab again.
One regression lays out 3000 paragraphs sharing 20,000 descending tabstops;
another lays out 50,000 blank paragraphs. Both check output as well as bounded
work. These are component improvements, not whole-application RAM/speed claims.

`generate_mtext_layout_fixtures.py` regenerates `layout.json`; set
`SLEUFBASE_MTEXT_LAYOUT_FIXTURES` to another directory for byte reproduction.
`verify_native_mtext_layout.py` creates fresh shifted/rescaled-spacing cases
with a different seed and compares the optimized `layout_mtext` driver
directly to the original complete engine: **425 fresh layouts / 928 columns /
11,700 primitives / 77 original errors** match. No application initialization,
settings/accounts or installed-font scan is used. Parser and layout drivers
share the same buffered bounded/atomic JSON transport; existing output is
never overwritten. The parser's size/cleanup probes exercise this transport.

Remaining full MTEXT work: the original simple-MTEXT route (the frontend selects
it separately from this complex measured-cell engine), parser/font-metric bridge, font-face/style choices,
glyph curves and per-run colors/backgrounds, standalone/embedded CAD source
and project persistence, OCS/attachment/INSERT transforms, actual canvas/map
integration and original/real-project acceptance. The desktop still reports
MTEXT as incomplete. This placement component does not prove full MTEXT or
global feature parity; every PARITY.md release gate remains required.
Original tools/text_layout.py MIT notice ships in source and --licenses.

The shared transport decodes JSON through Value before tagged event/cell
deserialization, matching the native project reader and avoiding the global
arbitrary_precision private-number-map trap for valid decimal inputs. A
precision regression and fresh parser tabstop cases verify the fix. The
parser's 1104-case audit (including 32/64 MiB limits and cleanup) passes again.
The optimized app/driver and tools/text_layout.py --licenses notice are checked.

All 435 Rust tests (39 desktop tests) and the final Clippy report pass without
new warnings. All three Windows runs for the preceding parser commit 0ca51fc
passed. Full glyph/CAD/project/canvas acceptance remains required.
