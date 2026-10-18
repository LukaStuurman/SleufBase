# Gerichte kernmetingen

Opdracht vanaf de repository: `cargo run --release --manifest-path rust/Cargo.toml --example benchmark_core`.
De Windows-CI voert dezelfde benchmark uit en bewaart het CSV-artifact.

`2026-10-17-windows-gnu-core.csv` is gemeten op Windows x86_64 met Rust 1.99.1
(GNU-toolchain), Intel Core i9-12900K, 100.000 synthetische puntobjecten en twintig
herhalingen. Alle constructie van de dataset gebeurt vóór de meting. Er is
allocatie-instrumentatie actief; de tijden omvatten die extra overhead.

| Codepad | Tijd voor 20 herhalingen | Allocatieaanroepen |
|---|---:|---:|
| Oude kaartinvoer met jobkopie | 712,771 ms | 10.000.080 |
| Kaartinvoer met referentie | minder dan 1 µs gerapporteerd | 0 |
| Oude validatie met tijdelijke puntvectors | 100,030 ms | 2.000.000 |
| Nieuwe validatie | 4,548 ms | 0 |

De oude codepaden komen uit previewcommit `1de19f4`. De benchmark bevat ze
uitsluitend als vergelijkingsbasis, niet als productcode. De nieuwe kaartinvoer
en succesvolle validatie moeten in deze benchmark zonder tijdelijke allocaties
werken; het programma faalt als dat verandert.

De referentieoverdracht is te kort voor een zinvolle snelheidsfactor op basis van
microseconden. Er wordt geen uitspraak gedaan over FPS, piekgeheugen, laden van
echte grote projecten, externe diensten of de volledige Python-app. Daarvoor
blijven representatieve datasets en aanvullende metingen nodig.

## Gecachte CAD-grenzen

`2026-10-17-windows-gnu-cad-core.csv` herhaalt de kernmeting op dezelfde machine
en voegt een overlay met 100.000 LINE-entiteiten (200.000 vertices) toe. Dataset
en cache worden vóór de meting opgebouwd. De oude grensscan uit `2f14723` en de
nieuwe cache geven dezelfde grenzen. Twintig scans kosten 11,961 ms; twintig
cache-opvragen rapporteren minder dan 1 µs. Beide paden alloceren nul keer.

De grenscache wordt bij bron-/laagwijziging ververst. De benchmark meet alleen
het opvragen tijdens gewone frames; opbouw, laagwijzigingen, CAD-rendering en
projectladen zijn uitgesloten. De meetresolutie rechtvaardigt geen
snelheidsfactor of FPS-claim. CI neemt beide CAD-rijen in het kernartifact op.

## MarXact-labelzoekactie

`2026-10-17-windows-gnu-leader-core.csv` voegt 20.000 verspreide labels en
20.000 exacte meetvragen toe op dezelfde machine. Drie rondes oude lineaire
zoekacties uit `84c74ad` kosten samen 2.448,102 ms. Drie rondes indexopbouw plus
alle zoekvragen kosten 38,655 ms: circa 63× sneller voor deze bewerking.
De index deelt labels, dedupliceert gelijke XY-posities en gebruikt één
aaneengesloten kandidaatarray; opbouw vergt in deze meting drie allocaties per
ronde. De oude scan alloceert niet, maar vergelijkt per vraag alle labels.

Een checksum bewaakt dezelfde labeluitkomsten op deze ondubbelzinnige dataset.
Afzonderlijke Python-/exhaustieve fixtures dekken ties en gecorrigeerde
afrondingsmissers. De benchmark gebruikt geen echte DXF en meet geen volledige
importtijd, procespiek of vergelijking met de Python-index. Zeer dichte cellen
met veel verschillende posities kunnen nog steeds duur zijn. CI publiceert
dezelfde twee extra rijen als onderdeel van het kernartifact.

## TIFF-preview tegenover Python/Pillow

`2026-10-17-windows-gnu-raster.json` bevat drie metingen in aparte processen op
hetzelfde systeem als hierboven. Iedere ronde wisselt de uitvoervolgorde. Het
besturingssysteem wist de bestandscache niet. Python 3.11.9/Pillow 11.3.0 opent de
volledige TIFF, converteert naar RGBA en verkleint met nearest-neighbor; Rust
decodeert alleen benodigde strips en bemonstert direct naar dezelfde preview.
De driver weigert resultaten bij afwijkende afmetingen of pixelchecksums.

| Pad | Mediane decodeer-/previewtijd | Hoogste procespiek over drie rondes |
|---|---:|---:|
| Rust begrensde stripdecoder | 48,653 ms | 29.614.080 bytes (28,2 MiB) |
| Python/Pillow volledige RGBA | 113,114 ms | 217.309.184 bytes (207,2 MiB) |

De synthetische bron is 4097 × 4097 RGB8 (16.785.409 pixels), DEFLATE, 16 rijen
per strip. Beide previews zijn 2048 × 2048 met RGBA CRC32 `1178218325`.
Rust bewaart maximaal 196.656 bytes gedecodeerde stripdata tegelijk en slaat één
strip zonder benodigde previewpixels over. In deze meting is Rust circa 2,3×
sneller en is de gemeten procespiek circa 86% lager. Dit is uitsluitend een
vergelijking van deze decodeer-/previewbewerking: startup, GUI, GPU, live
kaartinteractie, koude schijf en andere rasterformaten zijn niet gemeten.
De Python-procespiek omvat ook de Python/Pillow-runtime.

Reproduceren vanuit de repository (benchmarkbenodigdheden zijn alleen voor dev):

```powershell
python -m pip install Pillow==11.3.0 numpy==2.2.6 tifffile==2025.5.10
python tools/generate_raster_fixtures.py
cargo build --release --manifest-path rust/Cargo.toml --example benchmark_raster
python tools/benchmark_raster.py --native rust/target/release/examples/benchmark_raster.exe --rounds 3
```

De Windows-CI bewaart ook een native rastermeting van dezelfde fixture. De
Pillow-vergelijking hierboven is lokaal uitgevoerd; CI-tijden zijn geen
rechtstreekse vergelijking met deze machine.

## Object-DXF: exact dezelfde bytes bufferen of streamen

`object-export-windows-gnu-2026-10-18.csv` bevat drie gepaarde rondes met Rust
1.99.1/GNU in release. Eén synthetische job heeft 100.000 objecten: 50.000
meetpunten en 50.000 polylijnen van drie vertices. De twee paden gebruiken
dezelfde R2018-encoder met dezelfde gegevens en CRC32-berekening. Het referentiepad
bewaart de volledige uitvoer in een `Vec<u8>`; het streamingpad voert die naar
een tellende sink af. De volgorde is per ronde buffering gevolgd door streaming,
zonder aparte warm-up. Elke ronde controleert exact 61.846.263 bytes en CRC32
`85e78128` voor beide paden.

| Pad | Mediane encodeertijd | Hoogste extra getraceerde Rust-heap |
|---|---:|---:|
| Volledige uitvoerbuffer | 201,945 ms | 67.159.258 bytes (64,0 MiB) |
| Streaming met 64 KiB schrijfbuffer | 177,037 ms | 66.778 bytes (65,2 KiB) |

Dat is circa 12% minder tijd en 99,90% minder **extra getraceerde heap** voor deze
encodeerbewerking. De input bestaat vóór de meting en is uitgesloten; startup,
schijf-I/O, GUI, Python en OS-/GPU-geheugen zijn niet gemeten. De allocatietracker
telt levende Rust-allocaties; bij `realloc` telt hij het netto grootteverschil,
geen eventuele tijdelijke interne allocator-/OS-piek. Dit is geen totale
proces-RSS-meting of vergelijking met de oorspronkelijke Python-exporter.
Normale stringnormalisatie doet nog tijdelijke allocaties; streaming betekent
geen nul-allocatiegarantie. De GUI leent gewone geladen jobs voor DXF-export;
virtuele jobs en hun profielmarkeringen worden nog afzonderlijk voorbereid.

Reproduceren:

    cargo run --release --manifest-path rust/Cargo.toml --example benchmark_object_export

CI publiceert deze CSV en controleert dezelfde byte-/CRC-gelijkheid en een
lagere extra heap voor streaming. Functionele gelijkheid met Python wordt apart
getest op de [onafhankelijke exportreferenties](../tests/fixtures/object-export/README.md).
