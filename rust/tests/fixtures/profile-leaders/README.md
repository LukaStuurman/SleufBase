# Active profile label and LEADER contract

The production package's `__init__.py` does **not** install the optional dynamic
profile leader patch. `tests/test_profile_leader_v0310_behavior.py` explicitly
requires the original `cadastral_export.py` methods. These active methods use an
attribute-only annotation block plus a native LEADER and a profile-band line.
The Rust active path follows that contract. The optional donor/parameter kernel
remains separate; it is not substituted for the active export behavior.

`tools/generate_profile_leader_layout_fixtures.py` verifies five existing
pre-optimization golden cases and freezes another 60 original helper calls:
382 placements across empty, crowded, obstacle/fallback, Unicode, shifted bands,
minimum scales, collision-off and >80-entry cases. The original exporter is
isolated without application startup, settings, network or user documents.

`empty.dxf` provides original prepared layers/styles. `blocks.dxf` and runtime
`profile-leader-annotation.json` freeze the active original annotation block.
The native declaration is atomic and caches its ATTDEFs for subsequent batches.

`tools/verify_native_profile_leaders.py` independently compares 65 real native
DXFs/1,146 objects with fresh original exporter calls. Every primitive/attribute,
LEADER vertex, style/flag/color and annotation-handle link is checked; source
records, table/handle counts, zero audit errors/repairs and ownership are retained.

Full profile entry generation and end-to-end UI/template export are not covered
by feeding these frozen entries to the native layout solver. They remain open.
