# Original font-factory compatibility

`generate_font_resolution_fixtures.py` runs the original ezdxf 1.4.4 font factory
with private copies of original synthetic TTF/SHP/SHX/LFF fixtures, never installed
fonts or application/account settings. `no-outline.ttf` is derived from our own
TTF fixture by removing its outline table; its timestamp is fixed. Both generated
files reproduce byte-for-byte under different hashseeds.

**17 scenarios / 32 queries** compare the actual font type/file, advance width,
font measurements and every transformed TEXT corner. Cases include BIGFONT,
unknown SHX signatures, unrenderable SHP metrics/TrueType outlines, missing
defaults, fatal malformed SHX/LFF/default-TTF data, files disappearing after
indexing, missing-name fallbacks losing their file, and loaded glyph caches
surviving file deletion. The oracle inspects the actual glyph-cache backing font,
not the requested name or the last cache lookup. Fatal source exceptions remain
recorded separately; native error messages may differ, but may not return invented
successful extents. Numeric budget: 1e-12 + 32 f64 epsilon factors per scalar.

Native font errors retain an explicit invalid/unsupported/unavailable/limit/
cancelled category. The factory uses the original phase-specific recovery rules:
unsupported SHX formats (including BIGFONT) and unsupported initial glyph caches
can select TrueType, with monospace as last resort. LFF corruption and structurally
malformed supported formats remain errors. A failed default does not recursively
select itself. Transient failed reads are retried on later requests; successful
font data remains a shared immutable snapshot for the catalog lifetime.

Cancellation, file/operation/point limits and allocation failures remain fatal.
Tests cover typed errors, a large indexed TTF exceeding the actual read budget,
a SHP point budget and cancelled font resolution. Metadata scans read only bounded
name/OS2 tables; large unused glyph payloads are not read. On use, their byte limit
is enforced explicitly instead of silently skipping the indexed font.

A selector regression gives 32 font names the same 9 MiB shared font. The old
budget counted 288 MiB and incorrectly rejected the drawing. The native selector
now counts the allocation once while retaining the 256 MiB limit for distinct
data. This is an allocation-accounting correction, not a total RSS/speed claim.

The compiled `measure_font_resolution` driver uses the same catalog on private
temporary copies and performs the same file-removal sequence. Its output is also
checked by `verify_native_font_resolution.py`. The real paired exporter adds an
indexed BIGFONT profile style; the original factory supplies the same replacement
font to the independent grip/visibility oracle. Output font names are preserved;
glyph-program support beyond the original factory is not implied by fallback.

Viewport text/stroke rendering, additional font/vendor cases, full CAD/MTEXT,
real CAD/project acceptance and all global `rust/PARITY.md` release gates remain
open. This corpus closes the factory routes it actually exercises.
