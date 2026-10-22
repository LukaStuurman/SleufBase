# Original map comments, TIFF notes and workspace clearing

The generator runs only hash-validated original `KlicViewerApp` methods on fake
layers/widgets and the actual original cadastral comments formatter. It does
not start the application or load settings, accounts, network or clipboard.
54 complete batches cover 162 TIFF notes: Python value coercion, Unicode
whitespace, blank/false metadata, multiline/casefold deduplication, affine
centres/rotations, bounds filtering, edit/cancel, selected clearing and clear-all
counts/status. The original workspace reset's direct state changes, helper calls
and preserved project/settings/session sentinels are recorded separately.
Internal tabs are included alongside CR/LF and Unicode line separators.
The historical StreetSmart reset callback is source evidence only; that window
and Cyclomedia backgrounds are outside Rust scope by user request.

Native TIFF comments can be edited, removed or cancelled beside the selected
GeoTIFF. They persist in raster metadata, including imported Python notes.
Both native template routes combine free-map notes with TIFF notes at their
actual affine image centres, then format/filter once per slot in original order.
Extra export-slot notes remain separate user input and are appended after project
notes. Clearing project comments does not erase those separate export preferences.

The Kaartacties menu supplies Wis opmerkingen and Wis alles. Clearing comments
preserves layers and reference/start metadata; a no-op does not dirty the project.
Clearing all layers retains project name/path, settings, login/jobs and original
source files, clears loaded geometry/images/selection/order and background work,
and marks the empty project changed. Existing unsaved-work and source-owning
export guards apply: cancellation restores owned sources before clearing, and
late downloads/background results cannot repopulate the emptied workspace.
Read-only imported-source archives remain as provenance.

The real egui editor/menu receives pointer events for save and clear-comments;
Escape/cancel discards drafts. Native tests cover project round trips, atomic
validation, retained saved bytes, deferred worker cancellation and actual project
template output through both paired variants. Large free-map notes are checked
before copying; TIFF strings move into one per-export vector. New note edits
allow 64 KiB, the combined export allows 16 MiB/100,000 notes, and existing raster
metadata/project/raster budgets still apply. Non-RD TIFF notes are rejected for
RD template export. These are bounded text operations, not total application
memory or real-project acceptance guarantees.

An actual old executable rejected a valid internal tab in a comment, while
ezdxf preserved it on write/read. MTEXT replacement now accepts tabs, including
Unicode chunk boundaries; raw line endings/NUL and tabs in single-line fields
remain rejected. The complete native worker readback includes a TIFF note with
an internal tab and combines it with free-map and separate slot notes.

```
python tools/generate_map_action_fixtures.py
cargo test --locked --manifest-path rust/Cargo.toml --test map_actions
cargo build --locked --release --manifest-path rust/Cargo.toml --example resolve_map_actions
python tools/verify_native_map_actions.py rust/target/release/examples/resolve_map_actions.exe rust/target/release/sleufbase-rust.exe
```

Set `SLEUFBASE_MAP_ACTION_FIXTURES` to regenerate elsewhere. The runtime audit
uses a new seed and 73 complete original batches/219 affine TIFF notes through
the optimized production functions. CI regenerates the reference and runs the
fresh audit. Free-comment editing/dragging on the main canvas and full legacy/UI/
project/release acceptance remain part of PARITY.md.

Final local evidence: the complete 505-test suite (57 desktop tests), the final
76 targeted desktop/document/planner checks, Clippy without new diagnostics and
optimized app/comment/workflow drivers pass. The 73-batch/219-note runtime audit
passes, followed by real CLI tab exports in single and paired modes, independent
CAD readback and refusal to replace prior output. Seven real worker exports
produce 16 permanent PNGs and seven automatic XLSX companions; original comments
formatting, date/project/title fields, ownership, selected-point profile objects
and prepared TIFF pixels are checked independently. The reference reproduces
byte for byte with another Python hash seed.
