# Actual MTEXT DXF import, scene placement and projects

This corpus checks the ordinary MTEXT source adapter in real ASCII and binary
R2018 drawings. It combines the two independently checked rendering routes with
STYLE filename resolution, width/oblique, font-based automatic width, attachment
paragraph alignment, extrusion/rotation/direction and nested INSERT/MINSERT.

129 original source texts / 1,440 vector segments are compared after native
import, including empty text, manual line breaks, control characters, fractions,
inline font/height/shear, all nine attachments and TTF/SHP/SHX/LFF. The shared map
scene has 138 instances, including 12 descendants in mirrored, nonuniform nested
grids. The project test removes its temporary font files before save/reopen;
source contours and scene geometry survive. Invalid updates preserve prior
project bytes. The ordinary drawing route inherits existing layer/visibility/
base-color behavior; [inline paints](../cad-mtext-paint/README.md) now separately
check run colors throughout source, scene, project, canvas and template paths.

Expected geometry comes from the actual **reloaded DXF**, not only the editor's
in-memory entity. DXF saving can replace a rotation by a WCS direction, which
changes what the original simple frontend observes. Positive/negative extrusion,
rotation presence and explicit direction vary independently. Group 10 remains
WCS. With no direction or rotation the complex UCS uses global X; when rotation
is present it uses the original OCS conversion. Zero extrusion is normalized to
the original default; tiny nonzero directions are not rejected by an arbitrary
epsilon. Subclass isolation and group-1 tail order prevent unrelated/embedded
tags from becoming primary text. The embedded object is retained for later work.

Four original frontend decisions would send escaped nonbreaking-space `\\~`
content to the permissive simple decoder and render the literal code. The source
adapter instead sends it to real NBSP cells in the complex route. The unmodified
old vector segments remain as `legacy_segments`; native geometry is checked
against the corrected route. Existing bold/font and SHX-control corrections are
retained in the independent component corpora.

STYLE font names use the already audited filename/shape/fallback factory,
including extensionless CAD names and monospace fallback. Actual physical choices
and immutable faces are cached, so repeated source texts do not rescan families
or load font bytes again. STYLE width/oblique choose the appropriate route.
Older source caches without the new entity placement remain readable; source
reload is needed to replace their previously prepared layout.

`tools/verify_native_cad_mtext.py` generates fresh physical TTF bytes and shifted
drawings, then runs the optimized actual DXF reader/map-scene driver. Original
curves are sampled densely and checked in both directions against native chords,
with the native 0.005 drawing-unit WCS budget. This also checks contour breaks,
source content and nested instances. No SleufBase initialization, account or
user settings are used. The oracle shares the private empty font-cache import.

The ordinary source and inline-color routes are connected. Backgrounds/frames,
TTF fills, complete column-source extraction, embedded multiline attributes,
extended STYLE fonts, other vendor variants and real CAD/UI acceptance remain
required for full parity. The current application reports those limitations;
these tests do not claim whole-product parity or representative total RAM/time.

```
python tools/generate_cad_mtext_fixtures.py
cargo test --manifest-path rust/Cargo.toml --test cad_mtext
cargo build --release --manifest-path rust/Cargo.toml --example render_cad_scene
python tools/verify_native_cad_mtext.py rust/target/release/examples/render_cad_scene.exe
```

Use Python 3.11, ezdxf 1.4.4, FontTools 4.67.0 and NumPy 2.2.6.
Set `SLEUFBASE_CAD_MTEXT_FIXTURES` to regenerate elsewhere.
