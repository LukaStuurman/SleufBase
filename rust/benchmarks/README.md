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
