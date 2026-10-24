# Standalone map PNG workflow

The actual original `MapExporter.export_single/export_all` runs on six synthetic
RGBA TIFFs without application startup or personal settings. It supplies original
RGB PNG names, A4 dimensions, physical DPI metadata and 2,400 sampled pixels from
the completed pages. Samples include TIFF alpha/white masking, the original 70%
standalone opacity, affine rotations and coordinate crosses. Twenty independent
Pillow axis-stroke masks cover odd/even widths and both drawing directions.
The existing map-export fixture separately compares complete resampling pixels
and complete annotation drawing plans; font rasterization is not pixel identical.

The original duplicate-name overwrite is retained as explicit bug evidence: two
TIFFs named scan.tif/scan.tiff produce the same output path and lose the first page.
Native batches retain original names where possible and add deterministic suffixes
for duplicates, including Windows case collisions. Long/unsafe stems are shortened
and cleaned. Existing legacy PDFs are retained instead of deleted as a side effect.

The desktop Export kaarten action exports all loaded TIFFs in source order, with
PDOK, OpenStreetMap, Kadaster or a white offline background, configurable DPI/scale, visible
CAD inclusion, progress/cancellation and remembered settings. Original provider
and KLIC preferences migrate without credentials. The cadastral choice now uses
the original WMTS grid/crop/bilinear path; see [cadastral-tiles](../cadastral-tiles/README.md) for independent
complete image, cache/network and actual standalone export comparisons.
Cyclomedia backgrounds and the StreetSmart streetview window were removed from
the Rust scope at the user's explicit request on 22 October 2026. Old Cyclomedia
background preferences migrate to the default PDOK; no native window/provider is required.
Full provider/cache/opacity-control and real-project acceptance remain open.

The existing export worker owns and returns the actual loaded source allocations,
including after errors and project-switch cancellation. No full RGB page copy is
created: a reusable row buffer streams RGB PNG with pHYs resolution metadata.
Rendering/encoding/flushing all pages precedes publication. Late render failures,
cancellation and protected-source collisions keep prior outputs untouched.
Publication failures roll back earlier replacements. If rollback itself fails,
original backups and a destination manifest are retained and the error names the
recovery directory. This is not a crash-atomic multi-file filesystem transaction;
full crash recovery and real-project/UI acceptance remain global release gates.
Limits: 5,000 pages, 8 GiB encoded batch, existing full-TIFF/page/CAD budgets.
OSM export uses the page raster budget instead of the smaller desktop-preview
budget, while retaining at most 24 on-demand server tiles. A full 3344×2316 map
is assembled from a temporary seeded cache and every pixel checked without network.

Real GUI/worker tests exercise original source ownership, actual saved PNGs,
read-only project behavior, cancellation and deferred project switches. CAD checks
read back real PNGs with included/hidden/crossing layers. The full existing template
tests retain their opaque TIFF default. The corrected axis strokes remove the
previous one-pixel shift and extra square endpoints of even-width crosses.

```
python tools/generate_map_batch_fixtures.py
cargo test --locked --manifest-path rust/Cargo.toml --test map_batch
cargo build --locked --release --manifest-path rust/Cargo.toml --example export_map_batch
python tools/verify_native_map_batch.py rust/target/release/examples/export_map_batch.exe
```

Set `SLEUFBASE_MAP_BATCH_FIXTURES` to regenerate elsewhere. The runtime audit uses
a fresh seed, altered TIFF pixels/angles and new samples through the actual optimized
batch. Windows CI regenerates references and runs that audit. Full functionality,
project compatibility and standalone delivery still follow PARITY.md.

Local final verification: 487 tests including 49 desktop tests pass. The optimized
driver passes six fresh pages/2,400 changed-source samples. The existing optimized
template/map audit also passes 66,667 exact resampling pixels, 12 drawing plans,
41,915 virtual-map pixels and 53,095 actual CLI map-interior pixels. A live PDOK
standalone probe with a removed Cyclomedia preference produces a valid RGB/DPI
A4 PNG via migration to PDOK; 140,570 pixels differ from the white offline page.
All new JSON/TIFF/stroke fixtures reproduce byte-identically under another hash seed.
