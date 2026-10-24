# Kadaster WMTS background and standalone export

`generate_cadastral_tile_fixtures.py` executes the actual original
`PdokKadastralekaartWmtsTileClient` / `WebMercatorTileClient` planning, URL and
image composition helpers. It bypasses client initialization: no application,
settings, accounts, original persistent cache or network is started. The source
hashes (normalized LF line endings) are retained. Pyproj 3.7.1 supplies the original grid-free RD→3857
operation; Pillow 11.3 supplies crop, alpha composition and bilinear resize.

The 28 original requests cover Netherlands RD locations, aspect ratios,
decimal extents, zoom 22 and subpixel input. All pixels of 27 usable images are
compared exactly, including partial/zero alpha and tile boundaries. One original
zero-sized rounded crop silently becomes a transparent image; its actual image
is retained as old bug evidence. Rust rejects that empty crop before download.
Mercator coordinates allow 0.000002 m arithmetic differences; zoom, tile order,
resolution, integer crop and all output bytes must agree. The shared seven-
parameter definition also remains used by navigation; no source measurements
are reprojected or altered.

Native standalone export now offers Kadaster. The background uses the original
0–22 zoom selection without reducing export quality to the OSM preview limit.
Tile pixels are composited directly into the final crop, avoiding both a full
stitched mosaic and a second cropped copy. Four parallel downloads share an HTTP
client, seven-day atomic disk cache and a 32-entry Arc image cache (8 MiB raw).
Repeated hits share pixels without image cloning. Limits are 2 MiB/256×256 per
PNG, 2048 tiles, 50 million output pixels and 100 million crop pixels. Cancellation
is checked during batches/rows/read/retry backoff; a blocking socket is bounded
by 20 seconds. A failed or expired-only tile makes export fail rather than
publishing a partial or stale cadastral map. Earlier outputs remain protected
by the existing all-pages-before-publication batch transaction.

Local HTTP tests check retries, corrupt/wrong-sized/oversized replies, fresh
disk/memory hits, preserved expired cache bytes and actual concurrent requests.
Actual egui pointer events select Kadaster and verify the choice reaches the
source-owning worker, with source allocation returned. The independent runtime
audit uses 43 newly seeded requests and three real standalone A4/RGB/DPI exports
with synthetic cached WMTS tiles and original TIFF overlay, on isolated child-
process AppData paths. A later uncached location fails through a closed local
proxy; earlier output bytes and cleanup are checked. CI has no live-provider
dependency.

Verified locally: 518 Rust tests including 62 desktop tests; 15 final targeted
checks and Clippy with no additional diagnostics. All 29 canonical JSON/PNG
references and the optimized app/audit drivers build; the final runtime audit
also passes after cleanup. The canonical JSON/PNG
files reproduce byte-for-byte. The fresh 43-request runtime compares 148,474
RGBA pixels and rejects one preserved original empty crop. Three real A4 exports
compare 1,800 composited background/TIFF pixels plus RGB/DPI/dimensions and
late-network-failure preservation. The optional live probe on 23 October 2026
compares four actual PDOK tiles / all 49,152 output pixels with the original
compositor. A closed-proxy replay succeeds from fresh isolated cache without
rewriting bytes or timestamps. Live probing is not part of CI.

The original URL prefix is retained in fixtures. The native prefix follows the
[current PDOK service listing](https://www.pdok.nl/ogc-webservices/-/article/kadastrale-kaart):
`https://service.pdok.nl/kadaster/brk-kadastrale-kaart/wmts/v5_0`.
Its GetCapabilities was checked on 23 October 2026: Kadastralekaart,
EPSG:3857 matrices 00–22 and the same REST tile suffix. Attribution stays on
exported pages. No Cyclomedia or StreetSmart feature is reintroduced.

```
python tools/generate_cadastral_tile_fixtures.py
cargo test --locked --manifest-path rust/Cargo.toml --test cadastral_tiles
cargo test --locked --manifest-path rust/Cargo.toml --lib cadastral_tiles
cargo build --locked --release --manifest-path rust/Cargo.toml --example resolve_cadastral_tiles --example export_map_batch
python tools/verify_native_cadastral_tiles.py rust/target/release/examples/resolve_cadastral_tiles.exe rust/target/release/examples/export_map_batch.exe
```

Add `--live` to explicitly include the optional public-provider/cache probe.

Use `SLEUFBASE_CADASTRAL_TILE_FIXTURES` for alternate regeneration. Full raster
variants, other provider/cache parity, virtual tools, settings, real-project
acceptance and all release/transfer gates in PARITY.md remain required.
