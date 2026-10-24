# Approved profile leader metadata

`tools/generate_profile_leader_fixtures.py` freezes the 15 native AutoCAD donor
records (523 tags) from `autocad_profile_leader_donor.py` into the Rust runtime
data. These are parameter/action objects, not a visual reconstruction. The donor
SHA-256 identity is retained from the original module. Runtime Rust has no Python
dependency.

The two synthetic R2018 drawings use the original approved geometry constructor.
They include unrelated nested dictionaries, paper/model entities and Unicode.
`tagged.dxf` adds old block identity/representation tags, unrelated XDATA, a UTF-8
BOM and CRLF line endings. Neither fixture contains promoted metadata yet.

The independent verifier runs the original raw promoter on fresh copies and
compares every record/tag with the native output. Fresh GUIDs and freshly
allocated handles are normalized, with uniqueness, free HANDSEED, topology,
parameters, action targets and representation tags checked separately. It also
tests a high HANDSEED/lower-case pointers and the actual shipped template after
the original geometry preparation step. CAD readback must introduce no additional
audit errors or repairs. Running promotion again must retain identical bytes.

`profile-leader-geometry.json` freezes the original authored BLOCK_RECORD and
six BLOCKS records. `empty.dxf` is the same envelope before installation.
`references.json` freezes 40 original authored-block placements: minimum scales,
Unicode descriptions/depths and five optional/explicit colours. Three further
audits compare complete native installation or retention, 120 INSERTs and 240
ATTRIBs in two batches, all source records, handle allocation, table counts and
missing APPID registration. Source NLCS-ISO/linetypes are retained and required.

The shipped template **already contains a complete approved authored block**;
it has no layout example found by the original prototype finder. The native
installer retains that graph, its geometry, BLKREFS and XDATA verbatim. The
synthetic empty envelope exercises atomic installation instead. Neither audit
silently substitutes reconstructed geometry for an existing AutoCAD graph.

Prototype INSERT cloning, profile leader entry generation/distribution and the
complete profile/UI export remain separate requirements. These comparisons do
not prove interactive AutoCAD behavior.
