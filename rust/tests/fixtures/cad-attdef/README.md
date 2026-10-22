# Native one-line attribute definitions

The original drawing route displays a standalone ATTDEF's tag, not its default
value: ezdxf 1.4.4 `addons/drawing/text.py::_split_into_lines`. Its INSERT
`virtual_entities()` route (`explode.py`) excludes ATTDEFs, including constants;
attached ATTRIBs are drawn separately. The native reader follows these rules
instead of reporting every definition as unsupported or duplicating block text.
It preserves the default value, tag, prompt, flags, field length, lock state,
OCS/alignment/style and R2018 embedded MTEXT separately. The inspector exposes
the prompt. Skipped block definitions keep their source/style without font
resolution or contour work, so unused definitions cannot distort map bounds.
The drawable source indices are selected once per physical block and shared
across nested/grid references, with cancellation during selection. Ordinary
blocks require no index vector. A regression uses 2,000 definitions, 20,000
nested grid cells and 40,000 visibility entries: only the actual line is drawn,
all instances borrow its source, and project JSON stays below 2 MB. This avoids
repeating the definition scan; it is not a total application performance claim.

Eight R12/R2000/R2007/R2018 ASCII/binary drawings contain **442 source
definitions and 602 scene nodes**, covering all 15 TEXT alignments with
TTF/SHX/LFF fonts, oblique/mirror/OCS, hidden definitions/layers, empty tags,
constant/preset/verification flags, nested/nonuniform/grid INSERTs and attached
values. Large distant block definitions deliberately must not become map ink.
Expected visible tag strings come from the actual original frontend function;
the original text2path calculates every vector control point using a virtual
TEXT with identical supported text fields. This conversion avoids an old helper
bug that forwards attribute_type to TEXT; no source data is changed. Native WCS
chords are checked within 0.005 drawing units. Font fixtures already carry their
redistributable licenses; no proprietary fonts are included.

`generate_cad_attdef_fixtures.py` regenerates the corpus, with fixed metadata and
sorted required classes. Set `SLEUFBASE_CAD_ATTDEF_FIXTURES` to another output
directory. `verify_native_cad_attdef.py` creates fresh shifted/rotated/tag-changed
drawings and compares the optimized production reader via `render_cad_scene`
(eight fresh files / 602 nodes / 402 root definitions).
Neither helper initializes SleufBase or reads settings/accounts/installed fonts.
Native project roundtrips preserve source and glyphs without source fonts;
oversized invalid prompts cannot replace an existing file. Real canvas and
template-map tests verify ink/visibility and absence of pen-up bridges. The
optional prompt field keeps previous ATTRIB project data readable.

R2018 multiline ATTDEF is preserved and reported with an explicit layout
warning, without drawing its misleading one-line fallback. Multiline layout,
TrueType fill, full CAD styles/vendor cases, paper-layout viewing and real
project/AutoCAD acceptance remain open. All global PARITY.md gates still apply.
Original drawing/explode MIT notices ship in `--licenses` and preview artifacts.

`benchmark_cad_attdef.py before.exe after.exe` compares two optimized readers
on 10,000 definitions / 20,000 nested grid cells. It warms both variants,
alternates sample order and requires the same complete scene bytes for every
sample. Only the definition-selection algorithm should differ between builds.

The recorded Windows GNU release comparison in `benchmark.json` measured
0.607 s before / 0.120 s after (median, about 5.07x) on this synthetic case,
including JSON output. Scene bytes match exactly. The baseline is the same
ATTDEF implementation with only the shared selection index removed, not the
previous Python application. This does not prove whole-application speed/RAM.
