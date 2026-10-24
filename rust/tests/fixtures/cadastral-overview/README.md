# Original cadastral overview exports

`tools/generate_cadastral_overview_fixtures.py` runs the original writer with
synthetic affine TIFFs, Kadaster lines and labels. It never starts the app,
settings, accounts or a network service. Eight cases cover polygon/centerline/no
trench, TIFFs on/off, projection and PCA fallback. Class order and metadata are
fixed so fresh fixtures reproduce byte for byte.

`export_cadastral_overview` writes eight standalone and eight paired-template
exports. `tools/verify_native_cadastral_overview.py OUTPUT` independently runs
the original template overview writer and reads both written DXFs. It compares
all new overview entities, image transforms/graphs, exact copied TIFF bytes,
layer appearance, resolved text-style inputs and draw order. Overview objects
must remain outside both profile states, with no extra DXF audit repairs.

Native tests also cover cancellation/rollback, protection of loaded sources,
growing-file quotas, shared metadata accounting and skipping pixel reads when
neither trench geometry nor TIFFs are requested. Full-resolution processing
retains the existing raster limits; it does not downsample source geometry.

The original writer replaces its intended finite header extents with empty
layout sentinels during save. Rust preserves the intended extents, checked
against the original bounds/padding helpers. Real WFS/project/AutoCAD acceptance
remains separate from these offline contracts.
