# Composed MTEXT font and vector engine

The native complex-MTEXT engine now connects its parser to real font selection,
word/space measurement, paragraph/column layout and vector glyph/decorations.
`reference.json` contains 167 original complete font-face selections and 123
composed documents: 337 words, 430 drawing callbacks and one original malformed
tab-stop error. TTF, SHP, SHX and LFF, mixed formatting, fractions, tabs, nine
anchors, width/shear, column continuation and full affine placement are covered.
The existing measured-cell corpus separately compares every internal cell box.

All 13 TTFs are variants of the repository's original synthetic font. Their
family/style/weight/width and advances/contours intentionally differ; no installed
or proprietary fonts are redistributed. Five stroke fonts are copied from the
original stroke/LFF test assets. Each font has its own ordered support folder,
so tie-breaking is reproducible across filesystems. FontTools 4.67.0 creates
deterministic bytes; the complete font/oracle corpus reproduces under a second
Python hash seed.

Two observed original font-selection defects are explicitly corrected:

* MTEXT `b1` produces `Regular` with weight 700. The original style-first matcher
  can choose the sole Regular face before comparing weights. The MTEXT adapter
  supplies a Bold hint for this combination when no explicit filename exists.
* The original measurement adapter and UnifiedTextRenderer can select different
  fonts for the same inline face. Native measurement and glyph drawing share
  one resolved font. Cursive text therefore uses the measured cursive glyphs.

The unmodified original results remain under `legacy` for all 50 changed cases.
The corrected oracle changes only those selection decisions; parsing, original
ComplexMTextRenderer word/fraction/text renderers, layout arithmetic, FontTools
glyphs and transformation matrices stay original. Explicit column specs are
injected into the original adapter's column-construction callback; the separate
layout corpus already tests unequal/manual columns. No application initialization,
settings, credentials, accounts or installed-font scan occurs.

Native font matching retains original exact filename, family prefix, style
substring, weight, italic (separate from oblique), width and stable tie rules.
Names are normalized once during indexing. Physical font bytes and glyphs remain
shared. Descriptor identity aliases and tab arrays are retained while cached,
preventing pointer reuse; a 3,000-paragraph/20,000-tab case also uses one shared
27,000-byte font family across indent changes. Tab expansion has a 500,000-entry
combined budget in addition to the existing parser/layout/glyph budgets.

`tools/verify_native_mtext_render.py` creates fresh font bytes, changes actual
glyph controls/advances, varies formatting/anchors/tabs and compares the compiled
optimized engine against fresh original results. Its shared transport preserves
decimal bits, limits input/output, writes atomically and refuses overwrite.

This is a rendering component, not complete desktop MTEXT support. The separate
simple-MTEXT wrapping route and automatic width estimation are now implemented
and independently checked under [mtext-simple](../mtext-simple/README.md). DXF/embedded/source
extraction, color/layer resolution, persistent project/scène/canvas wiring,
background and TrueType fill, and representative CAD acceptance still need to
be completed. Desktop MTEXT continues to be reported as unsupported until that
integration is ready. The existing one-line TEXT pipeline now shares the raw
glyph engine; all 675 original vector cases remain checked.

Reproduce with Python 3.11, ezdxf 1.4.4 and FontTools 4.67.0:

```
python tools/generate_mtext_render_fixtures.py
cargo test --manifest-path rust/Cargo.toml --test mtext_render
cargo build --release --manifest-path rust/Cargo.toml --example render_mtext
python tools/verify_native_mtext_render.py rust/target/release/examples/render_mtext.exe
```

Set `SLEUFBASE_MTEXT_RENDER_FIXTURES` to generate/audit elsewhere.
