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

This implements the frame/band drawing stage. Terrain/surface texts, markers,
reference indicators, dynamic leaders, page replication and the complete native
template-export UI remain required for full migration parity.
