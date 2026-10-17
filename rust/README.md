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
- Rust filtert actieve jobs rechtstreeks uit de in de Rust-UI ingeladen objectgegevens en GeoTIFF-sidecars; een compatibiliteitsparser voor de Python-heartbeat blijft apart beschikbaar.

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

## Offline KickTheMap jobcache

- Rust leest bestaande lokale Python-SleufBase `jobs_cache/<email>.json` bestanden zonder wachtwoorden/cookies te lezen.
- Zoek en filter op jobs die in de Rust-app zelf zijn geladen (lokaal ingeladen objectbestanden en TIFF-sidecars).
- De cachestand is expliciet **niet live**; de online KickTheMap login, jobs API en downloaden moeten nog native worden overgezet.

## Kaartopmerkingen

- Plaats opmerkingen met RD X/Y en tekst, geef ze op de kaart weer en verwijder ze in de zijbalk.
- Opmerkingen worden opgenomen in het eigen Rust-project en herstellen bij openen van dat project.
- ASCII DXF POLYLINE-geometrie respecteert gesloten polyline-bit uit de POLYLINE-header.

## Instellingen veilig overzetten

- Via **Import oude voorkeuren** kan een lokaal oud SleufBase instellingen-JSON gekozen worden.
- Alleen expliciet ondersteunde niet-gevoelige sleutels (BGT-/Excelopties, materiaal- en profielenlijst, MarXact-mappings) worden gelezen en opgeslagen.
- KickTheMap-wachtwoorden, sessiegegevens en tokens worden bewust NIET gekopieerd.
- In de Rust-instellingen kun je automatische back-ups instellen (1–1440 minuten, 1–200 bestanden). De opties voor volledige DXF-sjablonen zijn voorbereid maar pas functioneel wanneer die uitvoer ook naar Rust is overgezet.

## Native KickTheMap-login en live jobs (eerste werkende koppeling)

- Rust HTTPS-client met uitsluitend vaste KickTheMap-endpoints, cookiejar **in geheugen**, eigen CSRF-login en maximaal 3 API-pogingen.
- Live jobs-payload kan rechtstreeks worden gelezen; toont actieve jobtitel, ID, adres, gemeente, datum en status.
- De login en jobs-opvraag draaien op een achtergrondthread; geen wachtwoorden, cookies of tokens in het projectbestand.
- Uitloggen verwijdert de sessie uit geheugen. De inlogvelden wissen het wachtwoord zodra verbinding wordt gestart.
- De bestaande KickTheMap-authenticatie gebruikt soms interactieve reCAPTCHA/WebView2: dit pad geeft nu een expliciete foutmelding; de browserlogin is nog NIET gemigreerd.
- De live endpointcode heeft unit-tests voor HTML tokens en API JSON, maar is nog NIET getest tegen een echt ingelogd account; er zijn geen gebruikersinloggegevens opgehaald of opgeslagen.
- Er zijn nu knoppen voor **GeoTIFF** en **Objecten** bij live jobs. De app vraagt een gesigneerde URL aan via `/jobs/get-file-url` in multipart/form/json, laadt zonder doorgeven van de KickTheMap-sessiecookies en controleert de inhoud.
- Downloads krijgen een tijdelijke bestandsnaam en worden pas na formaatcontrole zonder overschrijven opgeslagen onder `%LOCALAPPDATA%/SleufBase/KickTheMap/Rust`.
- Alleen HTTPS-hosts van bekende publieke clouddownloadservices worden geaccepteerd. De bronendpoint/inhoud zijn nog niet met een live account gevalideerd. Coord-files, AWS fallback en browserherstel ontbreken nog.

## PDOK BGT wegdelen en terreinranden

- Native OGC API-client voor BGT `wegdeel` en `onbegroeidterreindeel` met RD (EPSG:28992) `bbox` en `crs` parameters.
- GeoJSON LineString/Polygon/MultiPolygon, gesloten rings, grenzen en vervolgpagina's (maximaal 200); veilige controle op PDOK URL's.
- Achtergrondladen, aparte zichtbare BGT-lagen en bewaren in het eigen Rust-project.
- Kleine RD-uitsnedes tot 2×2 km; uitgebreide caches, vector tiles en alle BGT themalagen staan nog open.

## AHN 0,5m maaiveldhoogte (basis)

- Native PDOK AHN WCS 2.0.1 `dtm_05m` in RD EPSG:28992, met 2 m / 5 px, 8 m / 9 px en 24 m / 13 px fallback-vensters.
- Dichtstbijzijnde geldige rastercel rond het midden van het kaartgebied, teruggegeven in m NAP op millimeterniveau.
- Asynchrone HTTP-download en TIFF-decoder in Rust; geen Python/numpy.
- Nog geen AHN op klikpositie of volledige hoogteprofielen.

## MarXact: eerste native 3D DXF-import

- Via **MarXact importeren** kan een ASCII DXF met `MULTILEADER` labels, gesloten `POLYLINE` grenzen en `INSERT` meetpunten rechtstreeks worden geladen.
- Controleert het `Name` op iedere grensvertex, koppelt punten op maximaal 8 cm aan labels en wijst overlappende punten toe aan de kleinste gemeten grens.
- Bepaalt middenlijn en maaiveld-Z van de gemeten 3D-grens (met voorkeur voor meetpunt-richting), neemt laag-naar-discipline mapping over en bewaart de oorspronkelijke 3D-omtrek.
- Toont de gemeten contour op de kaart, neemt objecten mee in de basis-DXF/Excel en bewaart alles in een Rust-project.
- **Nog niet volledig:** echte MarXact DXF kan alternatieve geneste `MULTILEADER` opslag gebruiken. Daardoor zijn nog representatieve exportbestanden en vervolgtests nodig. Lokale dwarsprofielen, maatvoering, gekleurde en geclipte profielweergave en overige MarXact-workflows zijn nog niet volledig overgezet.

## MarXact-richting en uitlijning

- Import en **Automatisch uitlijnen** gebruiken de Python-regels: unieke objectposities, robuuste PCA met mediaan/MAD, stabilisatie op gemeten grensranden binnen 12,5° en snijpunten rond de objecten bij holle grenzen.
- De automatische begin-/eindpunten komen op de middens van de geraakte randen, met geïnterpoleerde Z uit de oorspronkelijke 3D-grens.
- Onder iedere MarXact-proefsleuf staat een **Rotatie °**-regelaar (-90° tot +90°). De correctie is absoluut ten opzichte van de automatische as; herhaald bijdraaien stapelt geen afrondingen op. Handmatige eindpunten liggen op de exacte snijpunten met de grens.
- **Automatisch uitlijnen** berekent de middenlijn opnieuw en wist de correctie. Een handmatige correctie van 0° behoudt de automatische richting, maar gebruikt zoals Python de objectgemiddelde positie en exacte snijpunten; gebruik de knop om ook de grensmiddens terug te zetten.
- De kaart toont de actuele as met een pijl en de gemeten objectpunten. De basis-DXF schrijft de 3D-as op `VIRTUAL_TRENCH_AXIS`; de gemeten contour en objectcoördinaten blijven behouden.
- Projecten en back-ups bewaren de automatische basis en de correctie. Oudere Rust-projecten zonder deze velden blijven leesbaar; de bestaande as wordt bij de eerste handmatige correctie de basis, of kies **Automatisch uitlijnen** om de nieuwe regels toe te passen.
- Regressietests vergelijken tien synthetische gevallen en zeventig handmatige correcties rechtstreeks met bevroren uitvoer van de Python-geometriefuncties. Fixturegenerator: `python tools/generate_marxact_alignment_fixtures.py` vanuit de repository. Dit bewijst geen volledige MarXact- of DXF-sjabloonpariteit op productiedata.

## MarXact- en virtuele-proefsleufgrenzen in DXF

- De basis-DXF-export schrijft nu ook gesloten **3D POLYLINE**-grenzen (R12) met de oorspronkelijke gemeten MarXact-Z per vertex, of de rechthoekige grens bij handmatige proefsleuven.
- De polylines krijgen de laag `VIRTUAL_TRENCH_3D` en een eenvoudig `TEXT`-label; origineel blijft in het Rust-project beschikbaar.
- Dit is nog **niet** de volledige AutoCAD-sjabloon/DXF-pariteit, maar voorkomt dat de gemeten MarXact-grens volledig uit de basisexport verdwijnt.

## PDOK Kadaster WFS (eerste native versie)

- WFS 2.0.0 `GetFeature` in RD EPSG:28992 met vijf bestaande SleufBase-categorieën: perceel, kadastrale grens, bebouwing, straatnaam en huisnummer.
- Gecombineerde kleine tegelverzoeken bij grotere gebieden; beperkte paginering, feature-ID-deduplicatie, HTTP-timeouts en grenzen voor geheugen-/downloadgrootte.
- Perceel- en bebouwingslijnen en de tekstlabels worden als afzonderlijk zichtbare laag op de kaart getoond en in het Rust-project bewaard.
- De labelhoek wordt bewaard maar **nog niet geroteerd** getoond in de Rust-kaart. Ook export van kadastrale lijnen en tekst naar de volledige DXF-sjablonen ontbreekt nog.

## Kadastrale en BGT-kaartlagen in basis-DXF

- De **DXF objecten**-export kan nu tevens de zichtbare Kadaster-lijnen, tekstlabels (inclusief rotatiehoek) en BGT-lijnen opnemen op afzonderlijke CAD-lagen.
- Export gebruikt het eigen basale R12-model, niet de uitgebreide SleufBase-sjabloon-DXF of speciale dynamische blokken.
- Alleen de zichtbare WFS/BGT-lagen worden meegenomen; grote datasetlimieten en daadwerkelijke CAD-validatie blijven nodig.

## KickTheMap-downloads: bronidentiteit behouden

- Bij een gesigneerde GeoTIFF- of objecten-JSON-download schrijft Rust nu een eigen `*.tiff.job.json` / `*.json.job.json` sidecar met job-ID en titel.
- De downloader maakt beide bestanden eerst tijdelijk aan en rolt een onvolledig nieuw bestand terug als de sidecar niet kan worden opgeslagen.
- Lokale objecten-JSON haalt de job-ID eerst uit de sidecar of bestandsnaam (`Naam_123_jobFeatures.json`) en gebruikt de oude mapnaam alleen als fallback.
- Daardoor werkt de 'alleen geladen jobs'-filter ook voor lokaal in één gedeelde Rust-map gedownloade jobs.

## Native MarXact batch-CLI

- `sleufbase-rust.exe --marxact nieuw-project.json meting1.dxf meting2.dxf` maakt een Rust-project zonder GUI.
- `sleufbase-rust.exe --marxact-dxf uitvoer.dxf meting1.dxf meting2.dxf` exporteert de gemeten 3D-grenzen en objecten in de basis-R12 DXF.
- De conversie leest de bestaande niet-gevoelige MarXact-mappings uit de Rust-instellingen en weigert bestaande uitvoerbestanden te overschrijven.
- Omdat de DXF-invoer nog geen volledige MarXact-vendordekking heeft, controleer conversies met echte MarXact-bestanden voordat je er ontwerpbeslissingen op baseert.

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
