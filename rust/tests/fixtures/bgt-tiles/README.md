# Native BGT-tegels: MVT, RD en opvragen

`tools/generate_bgt_tile_fixtures.py` vergelijkt met mapbox-vector-tile 2.2.0
en de oorspronkelijke BgtVectorTileClient. De generator gebruikt uitsluitend
vaste lokale PBF-bestanden en maakt geen netwerkverzoeken.

26 tegelgevallen vergelijken 819 oorspronkelijke geometrieën en 505 losse
ondergronddelen: lijnen, meerdere lijnen, vlakken/gaten, meerdere vlakken,
tegelbuffers, extents 0/1/256/4096/8192, Unicode-laagnamen, alle zeven protobuf-
waardetypen, identifierfallbacks, ontbrekende ClosePath, uitgepakte herhaalde
velden, onbekende extensies en herhaalde laagnamen. 360 matrixvragen controleren
de oorspronkelijke epsilon, randgevallen en volgorde van rijen/kolommen.

De echte PDOK-snapshot bevat 226.591 bytes, 518 geometrieën en 330 ondergronddelen.
Hij is op 19-10-2026 vastgelegd via [de openbare PDOK-tegelbron](https://api.pdok.nl/lv/bgt/ogc/v1/tiles/NetherlandsRDNewQuad/12/2420/2075?f=mvt),
nabij Eindhoven. De herkomst staat ook in reference.json. PBF-bestanden krijgen
geen tekst-/regeleindeconversie bij checkout.

De decoder volgt de [officiële MVT 2.1-protobufvelden](https://github.com/mapbox/vector-tile-spec/blob/master/2.1/vector_tile.proto)
en de oorspronkelijke decoder voor Y-omkering, ringvolgorde en waardekeuze.
Geometrie en identifiers worden rechtstreeks naar dezelfde RD-gegevens omgezet.
Dictionarywaarden worden gedeeld; de cache bewaart gedeelde uitgelezen geometrie
in plaats van die bij iedere opvraag opnieuw te maken. De parser begrenst
bytes, velden/woorden, features, vertices, teksten en groepsdiepte en controleert
annulering. Onvolledige/beschadigde gegevens leveren een fout op.

De native HTTP-client gebruikt een gedeelde verbindingenpool, begrensde
parallelle taken, maximaal vier pogingen met annuleerbare wachttijd en een
LRU-cache met aparte limieten voor aantallen en geschatte bytes. Ruwe én
uitgepakte gzip-antwoorden zijn begrensd; meerdere gzip-leden worden gelezen.
Verkeerde/kapotte/grote antwoorden komen niet in de cache. Batchfouten, annuleren,
een gestopte worker en het overschrijden van het resultaatbudget geven geen
gedeeltelijke batch terug. Een nieuwe taak kan dezelfde client blijven gebruiken.

Standaard: vier taken per batch, 45 seconden HTTP-timeout, 128 cache-items met
64 MiB conservatief geschat cachebudget en 256 MiB resultaatbudget. Een tegel
is begrensd op 24 MiB invoer/uitgepakte bytes, 64 MiB geschatte gegevens,
256 lagen, 100.000 features, twee miljoen vertices en zes miljoen woorden.
Een batch bevat maximaal 4096 tegels. Deze grenzen zijn geen procespiekmeting.

Dit zijn **ruwe tegeldelen**. Bronfeatures over tegelgrenzen samenvoegen,
topologie herstellen, op de exportgrens clippen en de volledige template-/UI-
workflow blijven afzonderlijk te porteren. De voorbereide profieloppervlaktekern
mag pas na die stappen op deze delen worden toegepast.

```powershell
python tools/generate_bgt_tile_fixtures.py
cargo test --manifest-path rust/Cargo.toml --test bgt_tiles --test bgt_tile_client
cargo build --release --manifest-path rust/Cargo.toml --example export_bgt_tiles --example benchmark_bgt_tiles
rust/target/release/examples/export_bgt_tiles.exe C:/temp/bgt-tiles-audit
python tools/verify_native_bgt_tiles.py C:/temp/bgt-tiles-audit
python tools/benchmark_bgt_tiles.py rust/target/release/examples/benchmark_bgt_tiles.exe rust/benchmarks/bgt-tiles.json
```

Een afzonderlijke live-controle bewaart de werkelijk ontvangen antwoordbytes.
De oorspronkelijke Python-code controleert daarna exact die bytes, zonder een
tweede mogelijk gewijzigde serverrespons. Een tweede native opvraag moet
dezelfde geometrie delen en geen extra HTTP-verzoek doen:

```powershell
rust/target/release/examples/export_bgt_tiles.exe C:/temp/bgt-tiles-live --live
python tools/verify_native_bgt_tiles.py C:/temp/bgt-tiles-live --live
```

De benchmark meet twintig koude uitlezingen/RD-voorbereidingen van dezelfde
echte tegel, in drie afzonderlijke native processen. Aantallen geometrieën en
ondergronddelen worden aan beide kanten gecontroleerd; de volledige inhoud
wordt door de aparte audits vergeleken. Native cachetoegang wordt apart gemeten
met 20.000 opvragen en gedeelde geometrie. Opstart, bestands-/netwerk-I/O,
union/herstel/clipping, profielkeuze, GUI/DXF en procespiekgeheugen zijn uitgesloten.
Het byteveld is een conservatieve dataschatting, geen meting van procespiekgeheugen.
