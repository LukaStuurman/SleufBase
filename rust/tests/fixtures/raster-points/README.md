# Start/reference points on the native map

`generate_raster_point_fixtures.py` invokes the hash-validated original
`KlicViewerApp` point methods on synthetic layers, through the actual active V9
profile exporter. It does not initialize the application, settings, accounts,
clipboard or network. The checked-in 96 cases cover automatic starts, all
candidate rows, 384 nearest-marker picks, inverse rotation when placing a
reference, raw start storage and clear-all side effects. Numeric Unicode marker
names, missing heights, coincident points and the 0.5 m forced-start threshold
are included. Values compare within 0.00000002 m.

The native desktop selects a GeoTIFF, places/moves a reference on the current
zoomed/panned map, selects a measured start marker, clears references and loads
starts for all linked KickTheMap TIFFs. The latter reuses loaded/local object
data or the existing logged-in download queue. Original BRT/BGT orientation
inputs are available online by default; the checkbox permits offline use of
loaded BGT. Provider failures produce warnings and preserve original fallback
behavior. Account-backed download/fallback acceptance remains open.

Raw points and unrelated metadata persist with the raster in native projects;
older projects without the optional field still load. Python project import
applies this metadata and resolves the known local object-data path. Both native
template planners pass the selected start/reference to the existing renderer.
Current source edits override older slot snapshots, and clearing removes those
snapshots too. Virtual-raster reference editing and full legacy settings/metadata
application are not finished.

Real egui pointer events use the actual canvas transform, including zoom/pan and
37.5-degree rotation. Worker tests check source allocation ownership, cancellation,
deferred project changes and atomic update validation. Pixels, object coordinates
and measured heights are not copied or changed. Metadata limits are 2 MiB/layer
and 16 MiB/project/export; batches allow 5,000 rasters, candidate lists 10,000.
Only one locally decoded object dataset is retained during bulk preparation.

```
python tools/generate_raster_point_fixtures.py
python tools/generate_profile_context_fixtures.py
cargo test --locked --manifest-path rust/Cargo.toml --test raster_points --test profile_context
cargo build --locked --release --manifest-path rust/Cargo.toml --example resolve_raster_points
python tools/verify_native_raster_points.py rust/target/release/examples/resolve_raster_points.exe
```

Set `SLEUFBASE_RASTER_POINT_FIXTURES` to regenerate elsewhere. The optimized
runtime audit generates 129 new original cases/516 selections and 62 new complete
context batches with different seeds. CI runs regeneration and the runtime audit.
Full workflow, project and delivery acceptance still follows PARITY.md.

Final local evidence: 498 tests including 53 desktop tests, the targeted final
atomic/legacy-import checks, Clippy without new diagnostics and optimized app/
drivers pass. The fresh 129/516/62 runtime audit passes. Live public PDOK requests
return 20 road paths/91 points and 72 terrain paths/1,337 points in RD, with
pointer-identical shared cache reuse. The real template worker exports six
drawings, 14 permanent PNGs and six XLSX companions; independent ezdxf/Pillow
readback compares the selected points' complete 52-entity profile and every TIFF
pixel with the original, alongside date/project/title/ownership checks.
