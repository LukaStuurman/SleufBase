# Native template order and desktop export

`generate_template_order_fixtures.py` extracts the actual source installer from
`app.py` and calls its pure helpers without starting a window, app or account.
The 140 original orders, 64 original multi-selection moves, 20 filenames,
normalization/variant/suffix cases and 48 nearby-comment batches are independent
of Rust output. Decimal and word semantics remain Python 3.11 / Unicode 14.

The native window edits a separate draft: PS numbers/variants, ordering,
multi-selection, blank/excluded slots, scale, project/drawn-by, profile source,
direction, reference, collisions, comments/address and background/BGT choices.
Empty slots consume a position; excluded sources do not. Appended sources start
excluded; a project generation change closes the old draft. Confirm normalizes
values; Escape/closing discards draft edits. Actual egui pointer/keyboard tests
cover the export button, project value and cancellation at 1024×768.

The existing bounded worker moves project datasets, rasters, CAD and comments
and returns them after success, errors, cancellation or panic. Template assembly
borrows datasets rather than cloning their vertices. Every map slot shares one
CAD path allocation. Duplicate BGT bounds share one normalized batch with a
joint 2-million-vertex / 100,000-feature budget. Constructed geometry keeps its
explicit 8-ULP boundary contract; supplied prepared surfaces remain exact.
Disabling profile drawing keeps the measured TIFF orientation and reference.

Native project format 4 persists template order/choices and opaque row metadata.
Formats 1, 2 and 3 are read without rewriting input bytes. It remains separate
from Python projects; complete legacy UI/settings migration and Python-format
export are still open.

`export_template_workflows` uses the production desktop worker and embedded
template for normal, reversed, image-only and ordered-variant exports. The
independent ezdxf/Pillow verifier checks actual project/date/drawn-by/title
values, comments, annotation handles, blank slots and durable raster dimensions.
The template is embedded in the standalone executable; no source-checkout path
is required. Earlier prepared export/map oracles remain separate checks.

The final release worker passed all four independent readbacks (10 PNGs),
including visible CAD geometry/colors. The combined 360 Rust tests include 34 desktop
tests. Export targets that alias loaded measurements, CAD, TIFF/sidecars, the
template or current project/import container are rejected before network/assets.

This is the native ordering/export path. Paired normal/reverse publication,
automatic companion Excel, complete photo annotations/virtual-mask parity, legacy
settings, remaining CAD families and real-project desktop acceptance still
remain in PARITY.md; the migration is not a completed product replacement.
