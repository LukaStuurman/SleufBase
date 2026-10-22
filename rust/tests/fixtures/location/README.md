# Native location workflow

The original `location_search.py` is loaded directly, with no SleufBase package
startup. Only the source AST of `_parse_inline_coordinate_query` and the validated
legacy `_zoom_for_location_type` function are invoked on synthetic objects.
Requests are recorded using a fake provider; no account, settings or projects
are read by the oracle.

The corpus covers RD parsing, decimal commas, mixed Unicode-14 decimal scripts
(all 66 alphabets), GPS/WGS84/EPSG prefixes, both Dutch coordinate orders, named
axes, invalid input, actual PDOK URL/parameter/field defaults, 36 service-search
responses, two reverse-lookup responses and all six map scales. GPS coordinates
are compared with the original pyproj EPSG:4326→28992 operation within 0.002 m
horizontally. The operation is the original seven-parameter transformation
without downloaded datum grids. This is navigation; source measurements and
elevations are never transformed or overwritten.

An explicit correction preserves old results as `legacy_coordinate`: prefixed
longitude-first Dutch GPS input was previously interpreted as a latitude near
5 degrees. Dutch ranges now determine unlabelled order; explicit lat/lon labels
take precedence. Corrected expected positions still come from the original
transformer after normalizing the order, not from the Rust code.

The native desktop provides debounced address suggestions, arrow/Enter/Escape
handling, Ctrl+L, selecting a result, temporarily marking without moving, and
jumping with the original type-dependent scale. It works on an empty map, uses
the actual panned/zoomed viewport for subsequent providers, and loads PDOK aerial
imagery after a jump. Search/navigation does not edit project data. A project
switch cancels and clears its search, marker and camera focus.

One reusable HTTP client and one worker maintain only an active query, one latest
pending query and one result. A thousand replacements verify bounded queueing;
cancelled, hidden and stale results cannot replace current suggestions. Service
responses are capped at 2 MiB, input at 4 KiB, and retries/backoff are cancellable.
An already waiting socket returns within its 20-second request timeout. A local
HTTP server verifies real status retries and response-size rejection. Malformed
point strings and nonfinite coordinates/distances are rejected/skipped instead
of reaching the camera or producing a crash.

Projection uses the pinned pure-Rust [proj4rs](https://github.com/3liz/proj4rs)
dependency with the same original projection/datum parameters. No Python, PROJ C
library, external process or grid download is used by the app. Cargo.lock fixes
the complete dependency set; CI builds with `--locked`. Its notices accompany
the portable preview.

```
python tools/generate_location_fixtures.py
cargo test --locked --manifest-path rust/Cargo.toml --test location_search
cargo build --locked --release --manifest-path rust/Cargo.toml --example search_location
python tools/verify_native_location.py rust/target/release/examples/search_location.exe
```

Use Python 3.11 and pyproj 3.7.1 for the independent oracle. Set
`SLEUFBASE_LOCATION_FIXTURES` to reproduce in another directory. The runtime audit
changes the random seed and generates new coordinate/service documents.
Reverse lookup is ready for the remaining TIFF-address overview/template tasks;
those UI workflows, StreetSmart and full settings/project acceptance remain open.
