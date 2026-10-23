# Virtual trench map placement, translation and references

The original hash-validated app methods run on synthetic virtual layers and fake
application services, without starting the app, settings, accounts, clipboard or
AHN/network. The actual original virtual raster helpers rebuild image transforms
with the active MarXact boundary and local-cross-section patches. The settings-driven
live-color hook is not invoked.
The fixtures execute whole-layer movement/start/click decisions, raw/displayed
references, reversed nearest-layer selection, automatic names and two-point
creation/minimum-distance decisions. The real native egui canvas is tested
separately with pointer and keyboard events; these are not original GUI sessions.

32 original chains execute 128 original press/release pairs and compare 288
nearest hits, raw/displayed references, all moved point XYZ values, metadata, automatic names and creation decisions. The 0.5 m
boundary is inclusive; too-short input stays in endpoint mode. Names include
gaps, physical-source collisions, leading zeroes, Unicode-14 decimal digits,
Python's special case-insensitive I/S matches and integer suffixes beyond u64.
Source functions, not Rust, supply every original expected result. Numeric
coordinates compare within 0.00000002 m. A fresh seed runs 47 chains/423 hits
against the optimized production functions (188 original press/release pairs).

The original measured-layer move changes point XY but leaves its 3D outline
behind. Those actual old results remain in `expected.boundary_3d`. The separately
named `corrected_boundary` deliberately applies the same translation to that
outline. Native whole-layer movement translates start/end, objects, measured
outline and automatic alignment basis as one validated operation. Z, source
files, raw reference metadata, unrelated options and vector allocations remain
intact. No full trench/metadata clone is made while moving or hovering.
Structural axis/width editing still protects measured geometry in the editor.

Native tools provide two primary-button presses with a live length preview,
selection, confirmed whole-layer placement with an outline preview, a context
menu anchored at the original right-click position, raw/display reference
placement and clearing. Primary gestures are captured through release; middle
dragging still pans. Escape/right-click cancels pending actions. Camera focus
is held after committing geometry changes. Creation opens the existing staged
editor; unknown heights stay unknown until measured/entered, rather than being
invented as zero. The original automatic AHN workflow is still an open gate.

Optional virtual-layer metadata persists in native projects; old projects without
the field remain readable. Python import activates non-geometric layer metadata
without duplicating the typed virtual payload/boundary; the complete original
archive remains unchanged. Limits are 2 MiB/layer and 16 MiB combined physical/
virtual project metadata. Current source metadata wins over old physical-slot
snapshots for virtual profiles. Image-only maps also keep an explicit reference.
Virtual layer notes join free/physical notes in both planners and clear actions.
Clearing notes/references updates an open editor draft so Save cannot revive
them. Manual copies preserve the displayed reference while dropping the old
rotation/job association. Other metadata remains preserved.

Actual project-worker tests exercise source ownership/return, virtual reference
metadata, notes and written DXF leaders. A separate production planner/export
driver writes manual, moved measured and image-only cases. Independent ezdxf/
Pillow/XLSX readback compares complete original profile entities, reference rings,
notes, local date/project fields and workbooks. Project opening and later
MarXact imports also retain visible reference-marker caches.

The export audit exposed an analytic-ellipse approximation in reference rings.
The bounded native scanline implementation now follows the original Pillow 11.3
quarter/ellipse algorithm, including inside outline width and fractional-bound
truncation. `ellipses.json` independently records all 589,824 RGBA pixels in
192 Pillow fill/outline cases: actual 22 px reference rings, odd/even axes,
fractional/negative/clipped/degenerate bounds, zero and oversized stroke widths.
Rendering has cancellation, checked traversal/pixel budgets and no full-page
mask. The adaptation uses the existing `third_party/Pillow-LICENSE.txt`; source:
https://raw.githubusercontent.com/python-pillow/Pillow/11.3.0/src/libImaging/Draw.c.
Both JSON fixtures reproduce byte-for-byte outside the build directory.
All 531 Rust tests (69 desktop tests), release app/drivers and Clippy without
new warning categories pass. The release readback verifies 3 DXFs / 118 complete
profile entities / 6 PNGs / 3 workbooks, including actual reference rings.
Visual inspection also preserves evidence of the original oversized legend at
low export DPI; that layout improvement remains separate. Virtual raster
masks/colors are not declared fully equivalent by this geometry/reference audit.

```
python tools/generate_virtual_map_fixtures.py
cargo test --locked --manifest-path rust/Cargo.toml --test virtual_map --test legacy_project
cargo build --locked --release --manifest-path rust/Cargo.toml --example resolve_virtual_map --example export_virtual_map
rust/target/release/examples/export_virtual_map.exe fresh-output-directory
python tools/verify_native_virtual_map.py rust/target/release/examples/resolve_virtual_map.exe fresh-output-directory
```

Use `SLEUFBASE_VIRTUAL_MAP_FIXTURES` to regenerate elsewhere. All remaining map
tools (individual point/end movement, copy direction, deck-band picking), original
surface/point-color/raster-mask behavior, settings, real projects, performance,
release and final repository transfer remain required in PARITY.md.
