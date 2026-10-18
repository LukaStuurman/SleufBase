# Original template image insertion and permanent asset bundles

The generator imports the original `CadastralDxfExporter._add_box_image` and
`_fit_image_to_box` in an isolated package, without Rust, UI, services or user
settings. Dependencies are the pinned Python 3.11 dependencies from
[the profile fixtures](../profiles/README.md).

`reference.json` freezes 91 placements: all 64 map/TIFF boxes of the shipped
template plus an inactive-paper-layout logo; twelve boxes plus a paper logo
in a synthetic template; and the same thirteen additions to a drawing that
already has an image layer/dictionary/definition/reactor and custom raster
variables. Repeated definition names check the original `(2)`, `(3)` suffixes.
Landscape/portrait, preserved aspect, stretch, zero/negative inset and an inset
too large for the box are covered. Input PNGs use a deterministic RGBA pattern.

The native batch writes IMAGE, IMAGEDEF and IMAGEDEF_REACTOR records, image
dictionary entries, missing raster variables/layer/classes, instance counters
and HANDSEED. Modelspace and the active paperspace use ENTITIES; inactive paper
layouts store their entities inside their own BLOCK. A named paper layout must
remain the owner after independent reload. This check caught and corrected a
native placement on the active sheet instead of the requested inactive sheet.

The document retains source ranges and stages a disjoint edit batch. Failed
batch validation preserves prior edits; the original drawing is not copied.
Collect images for all maps/logos/pages, then call `add_images` once per in-memory
document. Reloading a published document allows another batch. The limit is
10,000 new images and a conservative 128 MiB estimate for new raster metadata,
in addition to the existing document's 256 MiB output cap. Inputs are absolute
paths to native PNG assets; the function reads dimensions, not full pixel copies.

`template_transaction::Transaction` uses a new owned `export-*` directory under
`<stem>_assets` on the destination volume. Asset references point there before
publication. It validates and flushes completed drawing/assets, then resolves
cancellation at one atomic gate and replaces the drawing. Old bundles remain
for saved/open drawing copies. Failure, cancellation or unwinding deletes only
the attempt's directory. Native assets are flat files; subdirectories, links and
empty results are rejected. Windows paths use normal drive/UNC spelling for
CAD references. Filename traversal, device names and a collision with the staged
drawing are rejected. This is not a power-failure durability guarantee or an
automatic orphan-bundle cleanup policy.

Regenerate and independently audit in a fresh output directory:

```powershell
python tools/generate_template_image_fixtures.py
cargo test --manifest-path rust/Cargo.toml --all-targets
cargo run --manifest-path rust/Cargo.toml --example export_template_images -- C:/fresh-image-audit
python tools/verify_native_template_images.py C:/fresh-image-audit
```

The audit driver generates actual native PNGs, inserts them into each source
and publishes twice to the same output. The first drawing is copied before
replacement. The independent verifier opens both drawings and all permanent
images with ezdxf/Pillow, compares every RGBA pixel, original geometry and
display flags, definitions, reactors, paper-layout ownership and counters.
It compares every original ordered record, permitting only intended dictionary,
class-counter, layer-table-counter and HANDSEED changes. Existing image records,
raster preferences, unrelated source data and record order remain intact.
HANDSEED is compared with serialized handles; ezdxf synthesizes extra missing
SEQENDs while loading the original template. The 2D default pixel size is
normalized to XY, with zero Z checked explicitly after reload.

The shipped template retains its one known invalid-owner proxy audit fix;
native additions introduce no new audit errors/fixes. This layer does not
relocate arbitrary pre-existing external images. The synthetic existing image
has a preserved relative source reference, separate from the new PNGs being
audited. CI regenerates fixtures byte-identically and runs the complete audit.

This is real DXF raster insertion/publication, not complete template export.
Background-map rendering, TIFF preparation/rotation/masks, photos, general
entity insertion/page cloning, profile drawings, viewport/layout finalization,
dynamic normal/reverse variants and the user-facing template workflow remain.
