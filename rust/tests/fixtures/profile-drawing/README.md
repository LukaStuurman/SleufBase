`generate_profile_drawing_fixtures.py` invokes the complete original V9-patched
profile drawer and records its frame, chainage/height bands, verticals and band
connectors. Capturing hooks observe actual band positions and exclude the
separate terrain, marker, reference-indicator and dynamic-leader drawing stages.
No native implementation is used by the reference generator.

The compact `blocks.dxf` imports all six real profile-raster block definitions
and their fonts/layers from the shipped template with ezdxf 1.4.4. Twenty-four
drawings cover eight independently verified profile cases, normal/reverse,
manual and automatic scale, polyline maximum heights, duplicate markers,
reference-chainage offsets and RD descriptions. There are 928 compared drawing
objects, including every auto-generated attribute and its final position.
Profile inputs/rules are taken from `../profiles/reference.json`; the generator
checks each rebuilt profile against that existing oracle before drawing it.

One hundred further cases exercise the original band distributor on real DXF
block references: unsorted/equal anchors, pinned endpoints, outside anchors,
different text heights and overcrowded/narrow bands. These preserve the original
placement behavior; they do not claim that every overcrowded label can fit.
Unicode profile identifiers and decimal/negative-zero formatting are also
compared with the original helper methods.

`export_profile_drawings` computes profiles, scale, frame and band placement
entirely in Rust, writes real DXFs through the native template builder, and emits
placement metadata. `verify_native_profile_drawings.py` independently compares
all entities/attributes, preserved source tags, INSERT links and CAD audits.
The original template-geometry and image audits continue to cover composition
with the complete shipped template and permanent raster bundles.

The `--body` audit additionally compares terrain lines/colors, surface texts and
their rotated bounds, the soil cell, projected measured polylines and standalone
markers (including 64-segment clipping). The 24 composed drawings contain 2,209
objects. All hard/soft segments and text obstacles for the next leader stage are
compared with the original collector. The marker block is declared natively and
its circle, ownership and block-table count are independently checked.

Twelve extra fill cases compare custom/automatic precedence and Unicode colors;
150 original segment-clip cases include crossing/rejection/boundary behavior.
Malformed underscore-containing hex colors now safely fall back instead of
triggering the original integer-conversion failure. Source vertices remain
borrowed; direct profile-point indices avoid scanning all points for every band.
Frame, body, per-point geometry and connectors are composed by moving entities.

Automatic text resolution accepts pre-resolved BGT values at the drawing-stage
boundary; BGT physical-appearance geometry/fetch integration remains open.
Reference indicators are now independently covered by 216 direct original
helper calls across normal/reverse profiles, three scales, exact boundaries,
visible side padding and points beyond either edge. RD integer rounding and
rotated display coordinates are also compared. `--reference-cases` writes each
native indicator plus its segments/boxes; `--reference` composes the indicators
into the frame/body stage. All 24 drawings then contain 2,249 compared objects.
The reference metadata is resolved natively before frame distance labels are
created; the integration no longer substitutes the Python reference chainage.

`--leaders` completes the active profile drawing from native measurement data:
profile axes/points, frame/bands, body/markers/reference, derived label entries,
collision solver and linked INSERT/LEADER/band lines. `--leaders-collisions`
uses the original collision-enabled path. The 48 complete drawings compare
5,398 objects and every attribute/annotation handle. Native leader entries are
also compared with those captured from the complete original V9 drawer.

Global leader scale stays distinct from measured point diameter. Displaced
bands, double markers, endpoints and depth from start elevation retain their
original behavior. 336 independent original description cases cover sleeves,
bundles, Unicode decimals, rounding, custom diameter prefixes and material.
Adjacent marker instances reuse the resolved description/depth; source features
are borrowed. Stale band indices/heights, cancellation and numeric overflow are
rejected before appending any profile annotation geometry.

Existing oracle fields are semantically unchanged. Only entry/leader/description
references are added. BGT surface geometry/fetch, page replication, styles for
existing custom layers, raster preparation, real-project acceptance and the full
native template UI remain open. The optional dynamic profile patch is not active
in the production package and does not replace the active LEADER contract.
