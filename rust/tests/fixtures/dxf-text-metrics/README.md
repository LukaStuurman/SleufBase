# Font-dependent DXF text measurement

`SleufBaseFixture.ttf` is an original synthetic test font generated with
FontTools 4.67.0. It contains rectangles and quadratic curves, deliberately
different A/x/p heights, a nonzero baseline, a descender, space advances and a
missing-character glyph. It is not copied from an installed/proprietary font.

The original ezdxf 1.4.4 primitive renderer produces 360 text-extents cases plus
23 one-line decoding cases. They cover all 15 TEXT alignments, FIT/ALIGNED
stretching, height/width, oblique, rotation, backward/upside-down flags, OCS,
Unicode, empty/space-only text and caret/percent codes. Both the synthetic TTF
and ezdxf's explicit test monospace font are exercised. Quadratic glyph controls
are measured as quadratic controls; geometric curve bounds use a different
path contract. Native text corners are checked as point sets because ezdxf's
Path drops duplicate corners of zero-width rectangles.

The independent executable audit also asks the original font manager to resolve
six real font requests, then gives native code those exact local files. This
tests current installed/fallback fonts without redistributing them or recording
one machine's font choices in a golden fixture. It includes 36 additional text
cases. Fonts are measured with unkerned advances, matching the original fast
bbox calculation, rather than using the map/UI shaping rules.

Coordinates use a comparison budget of `1e-12 + 32 * f64::EPSILON * magnitude`;
this is an explicit floating-point comparison budget, not a claim of universal
byte-identical text rendering. No font rasterization or CAD text display is
implemented by this component. Windows font discovery and the paired-export UI are now connected through the
real worker (see template-paired). Native SHP/SHX metrics and explicit CAD support folders are now connected too
(see stroke-font). LFF/BIGFONT, remaining settings migration, MTEXT layout and
interactive acceptance remain open.

`tools/generate_dxf_text_metrics_fixtures.py` reproduces the reference using
Python 3.11, ezdxf 1.4.4 and FontTools 4.67.0. Set
`SLEUFBASE_DXF_TEXT_METRICS_FIXTURES` to regenerate elsewhere; two hashseeds
produce the same font and JSON bytes.
