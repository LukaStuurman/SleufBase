# Original TIFF-address overview and clipboard

Only the hash-validated original `_lookup_tiff_address_record` and nested
`copy_selected` functions are invoked on synthetic objects. No app, GUI, account,
settings or user projects are initialized. Fake clipboard operations record the
actual original values and status messages without modifying the OS clipboard.

48 row cases / 144 original clipboard actions cover successes, empty results,
errors, multiple matches (the first wins), duplicate filenames, long Unicode
names, negative/zero distances and adjacent floating-point rounding boundaries.
Native rows, both copy variants and complete source-order batches are compared.
The driver audit uses a new seed and 81 fresh rows/243 original clipboard contracts.
The optimized driver also passed a live PDOK batch for two public test coordinates
(Amersfoort/Utrecht), preserving order, distance formatting and both copy values.
The stored reference reproduced byte-for-byte under a different Python hash seed.

The native overview provides progress, cancellation, refresh, row selection,
double-click address copying and both original clipboard buttons. A real encoded
GeoTIFF is read and its actual loaded bounds determine the provider query; neither
pixels nor loaded source allocations move into the worker. Real egui clipboard
commands retain complete strings even when the table preview is shortened.
Opening/copying is read-only project work; project switching clears the overview
and cancels old publication.

Address batches and location suggestions share one bounded latest-job mechanism,
with separate reusable HTTP clients/workers. Each has at most one active job,
one latest pending job and one result. Replacement cancels the entire old batch,
including queries not yet sent; no partial report or stale response is published.
Only names/centers/EPSG metadata travel to the batch. Cancellation of a waiting
socket completes within the client's 20-second request timeout. Completed rows
are capped at 8 MiB of text / 5,000 sources; source/projection errors are displayed
per row and never change project data. An explicit correction rejects non-RD
declared GeoTIFFs before sending their coordinates to an RD-only service.

The earlier local HTTP test now explicitly restores blocking mode on accepted
Windows sockets; the nonblocking listener is retained for bounded test shutdown.
This fixes a timing-dependent test-server failure, not a production-client fault.

```
python tools/generate_raster_address_fixtures.py
cargo test --locked --manifest-path rust/Cargo.toml --test raster_addresses
cargo build --locked --release --manifest-path rust/Cargo.toml --example search_location
python tools/verify_native_raster_addresses.py rust/target/release/examples/search_location.exe
```

Use Python 3.11. Set `SLEUFBASE_RASTER_ADDRESS_FIXTURES` to reproduce elsewhere.
This finishes the ordinary TIFF-address overview/copy workflow. It does not finish
the complete settings, project, other exports, external services or delivery gates
in PARITY.md.
