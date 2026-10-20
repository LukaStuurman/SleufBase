# Original profile bounds and automatic visibility-grip locations

44 offline cases compare original ezdxf fast bounds and the actual production
`_selector_locations_from_document` helper. They cover LINE/POINT, open/full
arcs, circles, 2D/3D polylines, OCS, simple profile LEADERs, SOLID/3DFACE,
uniform transformed/rotated/mirrored block references, attached attributes,
omitted block ATTDEFs, image boundaries and clipped images.

The selector uses non-IMAGE candidates if any exist, otherwise image candidates.
Its margin follows the original clamp; empty profile geometry does not fall
back to images when a non-image candidate exists. Curve controls are retained
for the original fast bound; viewport preview tessellation cannot substitute
for them. The same numeric budget as the text-metrics fixtures applies.

Independent executable readback additionally checks thirteen selected slot groups
from five prepared native exports using the actual approved CAD template, its
real profile definitions, copied attributes and runtime original font files.
Global overview/sheet geometry preceding slot images is excluded because it is
not part of the normal/reverse content in production. All source and output
files are read only during this comparison.

The index rejects staged, unindexed edits. Missing or unsupported geometry/fonts,
cyclic or excessive nesting, invalid dimensions and cancellation cannot invent
a selector. Font source data can be shared across groups; glyph-advance caching
is bounded. Explicit limits are component limits, not total app-memory claims.
Nonuniform block TEXT regeneration, broad/bulged polylines, spline/arrow LEADERs,
MTEXT and other CAD families still need their own complete path semantics.

The original Python functions generate these references without starting the
app, reading preferences, using an account or accessing the network. Regenerate
with `tools/generate_template_extents_fixtures.py`; use
`SLEUFBASE_TEMPLATE_EXTENTS_FIXTURES` for another output directory. Python 3.11,
ezdxf 1.4.4 and the synthetic font from the text-metrics fixtures are required.
