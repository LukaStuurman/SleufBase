# Original road/terrain inputs for profile orientation

The original `RoadCenterlineClient` and `BgtTerrainBoundaryClient` run against
injected synthetic responses, without application startup, accounts or network.
The 48 complete original batches record 96 actual request URLs/parameters,
pagination, geometry order and repeated-query behavior. They cover line/multiline,
polygon/multipolygon rings and holes, ignored Z values and numeric strings.

Native code uses the original BRT top10nl `wegdeel_hartlijn` and BGT
`onbegroeidterreindeel` OGC collections, requests RD explicitly, pads bounds by
35% clamped to 60–180 m, and limits pages to the same HTTPS origin/collection.
Geometry is shared through a 24-entry LRU with a combined 2-million-point budget.
Limits are 24 MiB/page, 200 pages, 100,000 paths and 2 million points/batch.
Retries reuse one connection pool, and cancellation discards incomplete results.
Malformed coordinates, foreign pagination and repeated page URLs return errors.
This is orientation context, separate from polygon union/clipping for surface
labels and from full provider/account acceptance.

```
python tools/generate_profile_context_fixtures.py
cargo test --locked --manifest-path rust/Cargo.toml --test profile_context
```

Set `SLEUFBASE_PROFILE_CONTEXT_FIXTURES` to regenerate elsewhere. The optimized
point-mode audit additionally sends 62 freshly generated original context
batches through the real native parser/paginator. Live service acceptance is
separate from these deterministic tests.

Local live verification on 22 October 2026: the optimized native client returned
20 road paths/91 points and 72 terrain paths/1,337 points for a generic public
RD test area. Repeating each request returned the same shared allocation from
the actual client cache. Both original fixture sets regenerate byte for byte.
