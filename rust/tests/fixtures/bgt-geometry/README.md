# Native BGT source-feature preparation

`generate_bgt_geometry_fixtures.py` runs the original isolated Python client
with Shapely 2.0.7 / GEOS 3.11.4. It never opens app settings or uses HTTP.
Its reference scheduler consumes submitted tiles in a fixed order so equivalent
GEOS ring starts cannot make the oracle depend on worker completion order.

844 cases cover invalid/self-crossing/overlapping/nested polygons, holes,
collapsed lines/points, source-part union, rectangle edge/corner contacts,
zero-area bounds, 64 deeply nested rings, 300 seeded combinations and 487 source groups independently
decoded from the recorded public PDOK tile in `../bgt-tiles`. Three whole-batch
cases check grouping by layer/id/appearance, repeated tiles, resulting linework
and 120 original appearance decisions. `dedupe.json` separately checks exact
Python three-decimal/reversal-only path deduplication, including adjacent floats.

The original union-before-repair failure is intentionally fixed in Python too:
repair each part before union so a repairable part cannot discard good parts
of the same source feature. The pinned generator proves the old exception;
`test_bgt_geometry_repair.py` checks both corrected client methods against
independently repaired GEOS sets across supported GEOS versions.

The native engine is pure Rust (`i_overlay`, i64); there is no GEOS/Python runtime
dependency. Ring orientation/start, component order and collinear subdivision
can differ. Independent readback checks validity, polygon symmetric-difference
area, Hausdorff distance and total line/boundary length with a 2e-8 metre RD
coordinate tolerance, scaled by perimeter for area/length. Exact source vertices
are restored through engine roundtrip keys; new intersections use compensated
arithmetic. Constructed surface indexes use an eight-ULP coordinate roundoff
budget for inclusive boundary intervals only. Strict source indexes retain
their previous exact predicates and all 541 GEOS reference cases. Tests include
boundaries from GEOS results and verify that separated parallel lines stay out.

This covers geometry preparation, not the complete template export UI, page
extension, raster/foto preparation or full desktop application parity.
