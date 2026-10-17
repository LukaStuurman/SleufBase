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
