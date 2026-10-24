# Original template profile calculations

`reference.json` freezes 224 independent results from the original
`CadastralDxfExporter._build_template_cross_section_profile`, after installing
the V9 pipeline (which installs the preceding pipeline patches). The generator
imports the existing Python sources in an isolated package; it does not start
the UI, call services, load user settings, or invoke any Rust implementation.

Regenerate from the repository with Python 3.11 / Unicode 14:

```powershell
python -m pip install ezdxf==1.4.4 numpy==2.2.6 Pillow==11.3.0 shapely==2.0.7 requests==2.32.4 pyproj==3.7.1 mapbox-vector-tile==2.2.0
python tools/generate_profile_fixtures.py
cargo test --manifest-path rust/Cargo.toml --test profiles
```

Each normal/reverse pair is independently built. Cases cover road distance,
perpendicular projections outside segments, degenerate road segments, the 0.8
terrain distance weighting, an explicit start and its 0.5 m threshold, stable
endpoint ties, numeric marker names (including Unicode), missing heights,
degenerate axes, line midpoints interpolated by XY length, missing-Z vertices,
zero-length lines, marker extents, decimal-comma/Unicode diameters, double
markers, unknown disciplines, custom layers/labels and deckband descriptions.
Sixty deterministic generated datasets also exercise RD-coordinate precision.

The tests compare the complete profile signature: selected source feature,
synthetic endpoints and heights, axis, chainages, datum, ordering, descriptions,
layers and colors. Separate original methods provide expected projected
polyline coordinates (non-unit X/Y scales), visible chainage ranges and
interpolated surface heights inside and outside the axis. Geometry uses an
absolute tolerance of 2e-8 metres; text, counts and structure compare exactly.

`profile_unicode.rs` freezes the original `str.isdigit()` contract separately
from decimal parsing, so superscript marker names remain endpoint candidates.
CI regenerates both files, requires a clean diff, then reruns the native tests.

This proves the pure calculation kernel. Full template layouts, dimension
bands, leaders/collisions, marker clipping, generated deckbands from BGT, raster
assets, photos, dynamic normal/reverse blocks and UI workflows remain open.
Supplying a reverse dataset's own start/height metadata is the caller's job;
the kernel does not manufacture that metadata by mirroring a normal profile.

The native kernel rejects nonfinite inputs/overflow and offers cooperative
cancellation. It keeps feature indices and rule-layer references, and scans
line vertices without allocating copied vertex or segment arrays. Run
`cargo run --release --manifest-path rust/Cargo.toml --example benchmark_profiles`
for a synthetic 1,000 / 100,000 / 1,000,000-vertex allocation check. Its input
arrays are excluded; largest individual allocation is not total heap or RSS.
