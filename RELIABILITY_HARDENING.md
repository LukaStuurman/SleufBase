# SleufBase reliability hardening

## Doel

SleufBase moet zich gedragen als professionele bedrijfssoftware: voorspelbaar, herstelbaar bij externe storingen, veilig met grote/onbetrouwbare bestanden, snel zonder stale caches, en wijzigbaar zonder regressies bij eindgebruikers.

Deze hardening is bewust incrementeel. Grote herschrijvingen worden niet gecombineerd met productiebugfixes; elke fase krijgt eerst tests en een kwaliteitsgate.

## Fase 1 — direct gehard

### Netwerk en PDOK/WFS

- WFS-paginering heeft een harde paginalimiet.
- Een server die dezelfde pagina blijft retourneren wordt gedetecteerd; SleufBase stopt dan gecontroleerd in plaats van eindeloos door te gaan.
- De WFS-featurecache is begrensd (LRU) en thread-safe.
- Cachepruning geldt ook voor recursief opgesplitste zoekgebieden.
- Fouten worden als domeinspecifieke `CadastralWfsError` teruggegeven.

### Kaarttegels en parallel laden

- De in-memory tegelcache is begrensd en thread-safe.
- Tegelbestanden worden atomisch geschreven met `os.replace`, zodat een crash of gelijktijdige worker geen half PNG-bestand achterlaat.
- Corrupte/ongeldige cachebestanden worden verwijderd en opnieuw opgehaald.
- Tegels worden op formaat gevalideerd voordat ze in de cache komen.
- Bij netwerkproblemen blijft een bestaande stale tegel bruikbaar als fallback.

### GeoTIFF

- Pillow's decompression-bomb bescherming wordt niet meer wereldwijd uitgeschakeld.
- Er geldt een expliciete ruime limiet van 150 miljoen pixels per TIFF.
- Afbeeldingen worden gesloten als validatie tijdens het laden faalt.
- Ongeldige dimensies en lege geografische grenzen worden vroeg afgewezen.
- Singuliere GeoTIFF-transformaties geven een gecontroleerde fout in plaats van een onduidelijke NumPy-exceptie.

### Rendering en transformaties

- Viewports met nul/negatieve afmetingen of lege grenzen worden vroeg afgewezen.
- `DxfOverlay` heeft een expliciete methode om de native rendercache ongeldig te maken na geometriewijzigingen.

### Diagnostiek

- Crashrapporten krijgen microseconde, proces-ID en unieke suffix om overschrijven bij gelijktijdige fouten te voorkomen.
- Crashrapporten en systeemdiagnostiek worden atomisch geschreven.

### Engineering quality gate

- Een echte regressietest-suite is toegevoegd voor bovenstaande foutklassen.
- GitHub Actions compileert de Python-bron en draait de reliability-tests op Python 3.11.
- `.gitignore` voorkomt nieuwe gegenereerde Python-, build-, IDE- en runtimebestanden in commits.

## P0 technische schuld — bytecode naar broncode

De belangrijkste resterende architectuurbeperking is dat delen van de kern (`app.py`, `settings.py` en StreetSmart-gerelateerde code) gedrag uit `_bytecode/*.pyc` laden via `marshal` en `exec`.

Dit maakt volledige statische analyse, typechecking, security-review en normale Python-version upgrades onmogelijk. De huidige Windows-distributie is hierdoor bovendien hard gekoppeld aan Python 3.11.

De professionele eindtoestand is:

1. alle productiegedrag als leesbare `.py`-bron in versiebeheer;
2. `_bytecode` niet langer een primaire bron van applicatielogica;
3. unit- en integratietests vóór het verwijderen van de compatibiliteitslaag;
4. daarna pas Python 3.12/3.13 compatibility en een versie-matrix toevoegen.

Dit moet als gecontroleerde migratie gebeuren, niet als één grote rewrite.

## P1 volgende reliability-stappen

- native DXF-rendercache koppelen aan een expliciete geometrie-revisie zodat stale renderdata onmogelijk wordt;
- alle netwerkclients dezelfde retry/backoff/cancellation-policy geven;
- timeouts per connect/read-fase vastleggen in plaats van één generieke timeout;
- cachequota en periodieke disk-cache cleanup toevoegen;
- exportoperaties voorzien van cancellation en transactionele tijdelijke outputbestanden;
- credentialopslag volledig auditen zodra de verborgen settings/app-logica weer als bron beschikbaar is;
- foutmeldingen voorzien van stabiele foutcodes voor support/telemetrie.

## P2 build, release en supply chain

- runtime dependencies vastleggen in een gecontroleerde lock/constraints-set en geautomatiseerd updaten;
- dependency/security scanning toevoegen;
- Windows executables en installer digitaal ondertekenen;
- release artifacts voorzien van checksums/SBOM;
- branch protection instellen zodat `Quality Gate` en Windows smoke-test verplicht groen zijn voor merge/release;
- performance-baselines bewaren voor startup, kaartweergave en DXF-export en regressiedrempels instellen.

## Definitie van "professionele bedrijfssoftware"

Een release geldt pas als production-ready wanneer minimaal:

- alle verplichte CI-gates groen zijn;
- kritieke paden geautomatiseerd getest zijn;
- externe storingen niet tot oneindige loops of corrupte caches leiden;
- crashes een uniek diagnosebestand opleveren;
- builds reproduceerbaar en herleidbaar zijn;
- secrets/credentials aantoonbaar veilig opgeslagen worden;
- performance regressies meetbaar zijn;
- de primaire applicatielogica reviewbaar als broncode beschikbaar is.
