# Ordinary MTEXT inline paints

The actual ASCII/binary R2018 reader, project cache, nested block scene, desktop
canvas and both template-map paths now preserve inline ACI/RGB changes. One
logical entity remains one selectable scene feature; shared source contours and
metadata are not copied per word or per frame. Only small control/vertex offsets
describe color runs. Block instances subdivide after the full world transform
and retain their own vertex endpoints. Curve work, points and cancellation are
shared across every run, so splitting cannot bypass the limits.

141 independently rendered scene instances / 1,607 painted contours exercise
all 257 indexed roles, explicit black/white/RGB, color resets, nested groups,
manual paragraphs, decorations and fraction dividers, all nine attachments and
TTF/SHP/SHX/LFF. Layer zero inherits the INSERT layer, BYBLOCK follows its parent,
and BYLAYER uses the effective layer. Nested reflected/nonuniform 2×2 grids,
hidden parent/layers, layer true colors and true-black values are included.

Geometry and paint callbacks come from ezdxf 1.4.4's original complex renderer.
The fixture executes only SleufBase's pure `_resolve_entity_color` function
extracted from `dxf.py`; it never initializes the package or user settings.
Its established white-index contrast rule (crimson) is retained; explicit white
stays white. The previously accepted black-zero, layer-true-color and inherited
BYBLOCK corrections are applied explicitly. Font selection uses the separately
audited measurement/drawing corrections. The original renderer supplies every
word, decoration and fraction-divider paint; dividers retain the entity paint.

Native tests compare all original control curves after world subdivision and
every resolved RGB. They remove temporary font files before save/reopen, verify
shared source/cache identity after visibility changes, reject corrupt run ranges
without replacing the previous project, and read old uniform caches without
these optional fields. The real egui painter checks all 249 distinct visible
colors. Clipped template maps, desktop export preparation and written DXF group
420 values retain each path's color, including literal zero.

`verify_native_cad_mtext_paint.py` changes physical TTF metrics/outlines and a
physical SHP glyph, shifts source positions, generates new ASCII/binary files,
and audits the optimized actual import/scene driver. It checks colors and dense
original curves in both directions within the established 0.005-unit WCS chord
budget. Fixtures are byte-reproducible under a different Python hash seed.

This completes inline paint integration for ordinary MTEXT. Backgrounds/frames,
TTF fills, full column/embedded/extended-STYLE sources and real CAD/project/UI
acceptance remain required. This is no whole-product performance or parity claim.

```
python tools/generate_cad_mtext_paint_fixtures.py
cargo test --manifest-path rust/Cargo.toml --test cad_mtext_paint
cargo build --release --manifest-path rust/Cargo.toml --example render_cad_scene
python tools/verify_native_cad_mtext_paint.py rust/target/release/examples/render_cad_scene.exe
```

Use Python 3.11, ezdxf 1.4.4, FontTools 4.67.0 and NumPy 2.2.6.
Set `SLEUFBASE_CAD_MTEXT_PAINT_FIXTURES` to regenerate elsewhere.
