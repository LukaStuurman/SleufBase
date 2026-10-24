Native profile/map drawing primitives are compared with ezdxf 1.4.4 and the
original `CadastralDxfExporter._insert_template_block`, without Rust or services
in the reference generator. The shipped template covers all six profile-raster
block definitions, attribute defaults, three scales and model/inactive paper
space. A synthetic drawing adds nonzero XYZ block bases, oblique/rotated/aligned
text, Unicode, inherited colors, invisible/constant/preset attributes and active
plus inactive sheets. There are 89 primitive/block requests in total.

`tools/generate_template_geometry_fixtures.py` regenerates source/expectations.
`export_template_geometry` inserts two native geometry batches and a raster
batch, in both relative orders, into permanent transaction bundles. The Python
verifier checks every resulting primitive/attribute, serialized INSERT chains,
layout ownership, unique handles, HANDSEED, actual layer counts, PNG content and
preservation of all original records except intended table/seed/image additions.
No new ezdxf audit fixes are allowed relative to the source template.

The builder reads requested block definitions once and shares their parsed
attribute metadata across inserts. It supports positive uniform XY scales with
zero insert rotation, matching the current template block helper. Arbitrary
insert rotations, tilted/multiline attributes, full profile layout, band-label
distribution and dynamic leaders remain migration work. It does not synthesize
missing blocks/fonts or silently flatten unsupported attributes.

Limits: 100,000 requested entities, 10,000 declared layers/attributes per block,
one million output polyline vertices and 128 MiB staged entity text per call.
The preserved document's separate 256 MiB output limit remains enforced.
Cancellation or validation failures commit neither geometry nor resource state.
