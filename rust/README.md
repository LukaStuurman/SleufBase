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
- GeoTIFF-tags, affine RD-coordinaten, EPSG-key en KickTheMap sidecars lezen; kleine 8-bit TIFF rasters als gedraaide/gegeorefereerde kaartlaag tonen.
- Actieve KickTheMap-job-ID's read-only herkennen vanuit de Python-SleufBase heartbeat van maximaal 6 seconden oud.

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

Zie [PARITY.md](PARITY.md) voor de volledige functiepariteit-checklist en releasecriteria.

## Nieuw in deze migratiestap

- ASCII-DXF als aparte kaartlaag openen (LINE, LWPOLYLINE, POLYLINE, ARC, CIRCLE en INSERT-marker); binary DXF en veel advanced entiteiten nog niet.
- Kaart verschuiven met slepen en zoomen met muiswiel.
- Eigen Rust-project opslaan/openen als `*.sleufbase-rust.json`, met brondata, GeoTIFF-metadata en DXF-lagen.
- Automatische roterende back-ups in een **aparte** Rust-map. Het bestaande Python-projectformaat wordt niet overschreven.
- Instellingen voor deze back-ups lezen, met dezelfde defaults 10 minuten en maximaal 20 back-ups.

## Uitbreiding: virtuele proefsleuven

- Rust-datamodel voor virtuele proefsleuven met startpunt, eindpunt, breedte en optionele objecten.
- Zelfstandig aanmaken in de desktopinterface met RD X/Y-velden; render als contour en sla op met het Rust-project.
- Chainage, projectie van punten, dwarsprofielfragmenten en basis-DXF/Excel in dezelfde bronset.
- Uitvoer blijft **niet gelijk** aan Python-sjabloonprofielen, foto's en AutoCAD-blokdefinities.

## PDOK direct in Rust

- WMS 1.1.1 luchtfoto op vaste PDOK service URL, EPSG:28992, inclusief correcte BBOX/afmetingen.
- Verzoek in een aparte achtergrondthread: de UI blijft reageren tijdens ophalen.
- Veiligheidslimieten: maximaal 2048x2048 PNG, 24 MiB antwoord, 30 seconden netwerktimeout.
- De ingebouwde luchtfotolaag wordt op juiste geografische locatie als kaartondergrond getoond.
- **Nog geen** volledig tegelsysteem, WMTS, BGT, offline cache of schaalafhankelijke WMS-tiling.

## Wat nog ontbreekt

1. Online KickTheMap-login, live jobs-lijst, accountbeheer, downloads en sessieherstel.
2. Volledige GeoTIFF-rasterweergave (grote bestanden/andere bits/kleurtypes), PDOK WMTS/offline-cache, BGT, WFS, AHN, StreetSmart en kaartreprojectie.
3. Volledige kadastrale DXF-sjabloonexport inclusief profielen, raster, foto's en blokken.
4. Volledig uitgewerkte virtuele proefsleufprofielen (de geometriebasis is aanwezig), MarXact, **compatibiliteit met Python-projectopslag**, migratie van bestaande instellingen en volledig back-upherstel.
5. Gebruikersinstelbare discipline-regels, installatiepakket en volledige migratietests.

## Aanpak voor vervolgmigratie

1. Rust data-/exportkern en Windows CI: deze pull request.
2. GeoTIFF, RD-projectie, kaartweergave en sidecar-identificatie.
3. Read-only KickTheMap jobs-lijst en betrouwbare actieve-job filter.
4. Volledige template-/kadastrale DXF-pariteit met regressietests.
5. Projectformaat, instellingen, credential handling, installer en gebruikerstests.

De bestaande Python-app, build en releases worden in deze PR niet gewijzigd.
