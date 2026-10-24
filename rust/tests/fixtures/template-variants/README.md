# Native Normaal/Reverse visibility kernel

`input.dxf` is a small synthetic R2018 envelope carrying the actual eight
AutoCAD donor records from `assets/cadastral_template.dxf`, their original
class definitions and two donor states. Two tiny RGBA PNGs are local image
resources. No project, account, settings or network access is required.

`tools/generate_template_variant_fixtures.py` runs the original container,
promotion and finalization functions in isolation. Seven drawings contain twelve
pairs: duplicate labels with separate slots, Unicode/long names, shuffled input,
existing disabled layers, an omitted selector and fractional 3D coordinates.
Two drawings compute selectors through the actual original automatic-placement
helper, with profile candidates and an image-only fallback. The native
`assemble_automatic` path measures and installs these through one fresh index.
Another 49 naming decisions cover Python Unicode uppercasing and whitespace.
UUID4 values are normalized; source metadata and references are compared tag
by tag, including the parameter/grip offset and initial normal visibility.

`export_template_variants` creates native drawings. Independent readback checks
all state metadata, child geometry, attributes, images and leader annotation
references. Raw DXF owner tags are checked before relying on ezdxf's entity
binding, which silently repairs linked owners during reading. An audit must
report neither errors nor fixes. Rust regressions also check late cancellation,
callback panic, invalid groups, staged edits, exhausted handles and preservation
of every original handled record (except table counts) in the actual template.

Regeneration uses Python 3.11, ezdxf 1.4.4 and Pillow 11.3.0. A separate output
directory can be selected with `SLEUFBASE_TEMPLATE_VARIANT_FIXTURES`; two hash
seeds reproduce all four generated files byte for byte.

This proves the same-document container/visibility kernel for fresh groups and
its automatic selector path with supplied font resolution. Windows font discovery, dual raster/profile preparation and both native dialogs
are now connected through the real worker; see the template-paired audit.
Interactive AutoCAD acceptance, other font types and complete legacy workflow
parity remain requirements. The
separate text-metrics and template-extents fixtures document the supported
geometry/font contracts and their remaining limitations.
