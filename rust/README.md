# SleufBase Rust (0.1.0-preview)

Dit is een aparte native Rust-desktopapp naast de bestaande SleufBase v0.3.62 (Python).
De Rust-versie is nog niet productiegelijkwaardig aan SleufBase.

## Wat er in deze preview zit

- Rust Windows-desktopvenster met Techbase-oranje accenten (egui/eframe).
- Openen van meerdere lokaal beschikbare KickTheMap jobFeatures.json bestanden.
- Parser voor handmatig geplaatste objectpunten en objectpolylijnen, inclusief optionele Z-coordinaten.
- Schematisch kaartvoorbeeld (nog zonder PDOK/GeoTIFF en projectietransformatie).
- Disciplinetellingen per proefsleuf gebaseerd op Python standaardregels (inclusief aantallen per discipline).
- Excel (.xlsx) export met oranje kopregel, proefsleuf, aantal disciplines en totaal kabels/leidingen.
- Basis DXF R12 export met kruizen voor punten en LINE-segmenten voor polylijnen per discipline.
- Beveiliging tegen het overschrijven van bestaande uitvoerbestanden.
- Rust-unit-tests en Windows CI die een executable als build-artifact maakt.

## Bouwen en starten op Windows

Installeer Rust via https://rustup.rs en voer in PowerShell uit:

    cd rust
    cargo test
    cargo build --release
    .\target\release\sleufbase-rust.exe

Of download bij GitHub Actions in workflow 'Rust preview - Windows' het artifact
'SleufBase-Rust-Windows-preview' van een geslaagde build. Dit is geen stable release.

### CLI export

    .\target\release\sleufbase-rust.exe --xlsx disciplines.xlsx C:\jobs\PS1\jobFeatures.json
    .\target\release\sleufbase-rust.exe --dxf objecten.dxf C:\jobs\PS1\jobFeatures.json

Nieuwe uitvoerbestandsnaam verplicht. De GUI exporteert alle lokaal geladen JSON-bestanden.
Selectie van een job wijzigt alleen het kaartvoorbeeld en de details, niet de export.

## Wat nog ontbreekt

1. Online KickTheMap-login, live jobs-lijst, accountbeheer, downloads en sessieherstel.
2. GeoTIFF, PDOK, BGT, WFS, AHN, StreetSmart, caching en kaartreprojectie.
3. Volledige kadastrale DXF-sjabloonexport inclusief profielen, raster, foto's en blokken.
4. Virtuele proefsleuven, MarXact, projectopslag, backups en bestaande instellingen.
5. Gebruikersinstelbare discipline-regels, installatiepakket en volledige migratietests.

## Aanpak voor vervolgmigratie

1. Rust data-/exportkern en Windows CI: deze pull request.
2. GeoTIFF, RD-projectie, kaartweergave en sidecar-identificatie.
3. Read-only KickTheMap jobs-lijst en betrouwbare actieve-job filter.
4. Volledige template-/kadastrale DXF-pariteit met regressietests.
5. Projectformaat, instellingen, credential handling, installer en gebruikerstests.

De bestaande Python-app, build en releases worden in deze PR niet gewijzigd.
