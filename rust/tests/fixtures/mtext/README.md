# Native MTEXT inline parser foundation

This is the first component of the full native MTEXT renderer, following
ezdxf 1.4.4 `tools/text.py::MTextParser` and `MTextContext`. It retains words,
spaces/nonbreaking spaces/tabs, paragraph/column/dimension-wrap commands,
stacked fractions and complete immutable formatting snapshots. It does not
strip MTEXT into the one-line TEXT renderer.

The corpus has **976 original cases / 3467 emitted tokens / 14 explicit
errors or numeric-overflow guards**, with property-command emission both on
and off. It covers caret/control decoding, escapes, unknown commands, %%
symbols, scoped/continued under-/over-/strike lines, absolute/relative height,
width/tracking/oblique/alignment, ACI/RGB reset/byte order/modulo, font-family
switches/bold/italic, fraction separators/escapes, paragraph resets/alignment,
indents and mixed left/center/right tabs. All 66 Unicode 14 Nd digit families
match the original Python 3.11 numeric grammar. Exponent prefixes and incomplete
commands preserve original consumption rules, including typed errors for
incomplete alignment/stack escapes. No app initialization, settings, accounts
or installed-font scan is performed.

The native core shares context, font and paragraph/tab arrays instead of
cloning large strings for each token. A 100,000-token test shares a 32 KiB
font name. Another test shares 20,000 tabstops across thousands of height and
paragraph changes; immutable, already-validated tab arrays are not rescanned
on each scalar change. These are component properties, not overall app RAM or
speed claims.

Limits are explicit: 1 MiB source, 200,000 tokens, 100,000 formatting changes,
128 nested scopes, bounded font/tab names and cancellation. Nonfinite numeric
states (which the original may allow) are rejected. Such cases are separately
labelled as guards rather than represented as successful original behavior.
Cancellation/limit/invalid errors remain typed; a failed parse returns no
partial token vector. Unmatched ordinary braces keep the original permissive
behavior within the depth limit.

`generate_mtext_fixtures.py` regenerates `parser.json` deterministically; set
`SLEUFBASE_MTEXT_FIXTURES` to another output directory for reproduction.
`verify_native_mtext.py` generates fresh cases with a different seed, text and
initial formatting and compares the optimized `parse_mtext` driver against the
original parser directly: **1104 fresh cases / 4246 tokens / 14 errors or guards**
match. The driver interns contexts in output, bounds request
and output size, streams documents and publishes a complete new output
atomically, refusing overwrite. Real probes exceed the 32 MiB input and 64 MiB
output limits and confirm cleanup without publishing a partial file. The writer
is buffered. It does not call Python from Rust.

The parser now feeds real font metrics, simple/complex layout and the ordinary
CAD source/project/scene route. See [cad-mtext](../cad-mtext/README.md) for actual
ASCII/binary import and independent geometry evidence. Full column/embedded
sources, per-run colors, backgrounds/fills and real CAD acceptance remain open.
This component does not satisfy full MTEXT or global PARITY.md gates.
Original tools/text.py MIT notice is retained in source and `--licenses`.
