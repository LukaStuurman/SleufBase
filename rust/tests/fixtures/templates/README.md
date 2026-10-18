# Template preservation and original slot recognition

`reference.json` comes from the original Python
`CadastralDxfExporter._detect_template_slots`, using ezdxf 1.4.4. It includes the
shipped `assets/cadastral_template.dxf` (32 slots) and five deterministic
synthetic sources. Three synthetic templates have six recognized slots, with
two columns and two page blocks, a third excluded column, paperspace decoys,
paragraph prefixes, or missing optional addresses/comments. Empty and literal
escaped-prefix templates reproduce the original recognition error. The native
reader preserves these strings without inventing a different interpretation.

Regenerate with the Python 3.11 dependencies listed in
[the profile fixtures](../profiles/README.md):

```powershell
python tools/generate_template_fixtures.py
cargo test --manifest-path rust/Cargo.toml --test template_slots --test dxf_document
cargo run --manifest-path rust/Cargo.toml --example export_template_fixtures -- C:/fresh-template-audit
python tools/verify_native_template_edits.py C:/fresh-template-audit
```

Use a fresh output directory: the example publishes with atomic no-clobber
writes. The example is a test driver, not a complete template export command.

The Rust document layer retains the original UTF-8 source and indexes record
ranges/handles. It does not flatten the template into overlay geometry or build
a second vector with all tags. Unedited drawings must remain byte-identical.
TEXT/ATTRIB/ATTDEF and MTEXT replacement modifies only the content tags; MTEXT
is split into at most 250 Unicode characters per chunk, including astral text.
All other tags, layouts, blocks, dictionaries, proxy objects and opaque data
remain unchanged. Repeated replacement, LF/CRLF, invalid handles/records,
unsupported formats, scalar/value limits, newline injection, cancellation and
the actual template round-trip have regression checks. Input and staged output
are limited to 256 MiB, individual text to 1 MiB, records to two million.

The independent verifier compares **every ordered source tag**, allowing only
the intended text fields to change. It also reads edited files with ezdxf,
checks resulting text, layouts/block names and compares audits with the source.
The shipped template already has one audit fix: invalid-owner proxy #147B7
(code 202). The preservation layer retains that source record. It does not
claim to repair or produce a repair-free template. Synthetic source audits
have zero fixes, and native edits introduce no new errors/fixes in these cases.

Supported editing input is modern ASCII DXF R2007/R2010/R2013/R2018 with valid
UTF-8. Binary and older codepage templates are explicitly rejected; the separate
overlay importer supports more input formats. This layer is not a full CAD
semantic validator, and input ownership/indexing are not a total memory cap.

Native raster insertion/publication is now separately covered by
[the template-image references](../template-images/README.md).
Page duplication, general entity insertion/handle remapping, active layouts/viewports,
profile bands/leaders, marker clipping, complete raster preparation, generated
deckbands, photos, dynamic normal/reverse variants, cleanup of the known source
proxy and the native UI export workflow remain to be implemented. CI regenerates
the independent sources, checks for identical diffs and audits native edits.
