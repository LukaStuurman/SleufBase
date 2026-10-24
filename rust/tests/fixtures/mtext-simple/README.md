# Separate simple-MTEXT rendering and automatic width estimation

The original drawing frontend uses two distinct MTEXT routes. This component
ports `simplified_text_chunks` with its own `fast_plain_mtext` and `text_wrap`
semantics, font measurements, line positions and glyph curves. It does not
substitute the complex cell layouter for ordinary text.

`reference.json` checks:

* 616 original permissive decoder inputs, including scoped text, unknown and
  unterminated commands, caret/percent escapes and incomplete fractions.
* 189 composed simple-MTEXT documents / 467 lines, with all nine attachments,
  manual and automatic breaks, leading/trailing/Unicode whitespace, tabs,
  empty paragraphs, default/minimum height, direction-vector priority, extrusion
  sign, rotation and RD placement. TTF, SHP, SHX and LFF use the existing owned
  synthetic fonts. Exact width plus its immediately adjacent floating-point
  values explicitly check every wrap decision.
* 324 original fast content-extent estimates, including zero height, whitespace
  space-width fallback, naive column counts, 1.667 leading and the 1.01 width
  safety factor. This estimates one column; it is not precise formatted layout.

The original SHX `get_shape` correctly caches a space advance for an empty
control glyph, but the first `get_advance_width` call reads the empty glyph's
endpoint again and raises IndexError. The corrected oracle returns that already
cached fallback for control codes only. All 21 unmodified failures remain under
`legacy`. No font geometry, other advances, decoding, wrapping, alignment or
matrices are replaced. Native SHX metrics already used this fallback.

The new streaming reader adds each character's **unscaled** advance in the
original order and checkpoints the last non-whitespace width. This preserves
exact wrap boundaries without scanning the complete prefix after every word;
world-width prefix subtraction would introduce a different rounding order.
TTF tables are parsed once per run. Existing font data/advance caches remain
shared; the view of those tables occupies one small heap allocation. SHX/LFF
interpreter work is counted against the shared measurement budget, including
uncached glyphs. No shape/control budget is turned into a fallback font.

The fast decoder scans forward without copying the unconsumed suffix per
formatting command. It preserves even the original unterminated-stack behavior,
but stops at a 2 MiB decoded-output budget. A 60,000-word unwrapped line and
100,000 unknown escapes check bounded processing; a repeated unterminated stack
is rejected before the old expansion can consume unbounded memory.

`tools/verify_native_mtext_simple.py` compares fresh complete original results
against the optimized `render_mtext` audit driver with changed physical glyph
controls/advances and placements. The shared transport preserves decimal bits,
limits input/output, writes atomically and refuses overwrite. The corpus
reproduces under a different hashseed. The oracle imports ezdxf using the empty
private cache from the composed-MTEXT helper and never initializes SleufBase.

The two render components are now connected to ordinary DXF/STYLE source,
project persistence and the shared map scene; see [cad-mtext](../cad-mtext/README.md).
The source adapter routes escaped nonbreaking spaces through the complex parser
and tests the four corrected cases against preserved original literal-code
contours. The fast-decoder compatibility API remains exact for estimation/oracle
purposes. Complete column/embedded sources, per-run colors, backgrounds/TrueType
fill and real CAD/UI acceptance remain required for full parity.

```
python tools/generate_mtext_simple_fixtures.py
cargo test --manifest-path rust/Cargo.toml --test mtext_simple
cargo build --release --manifest-path rust/Cargo.toml --example render_mtext
python tools/verify_native_mtext_simple.py rust/target/release/examples/render_mtext.exe
```

Use Python 3.11, ezdxf 1.4.4 and the existing owned `mtext-render/fonts` assets.
Set `SLEUFBASE_MTEXT_SIMPLE_FIXTURES` to regenerate elsewhere.
