# SleufBase performancebeleid

SleufBase gebruikt standaard een hardware-afhankelijk resourceprofiel. Het doel is niet om zo veel mogelijk threads te starten, maar om per type werk de snelste veilige hoeveelheid parallelisme te kiezen.

## Automatische modus

`SLEUFBASE_RESOURCE_MODE=auto` is de standaard. Bij het opstarten worden het aantal logische CPU-threads en het beschikbare/aanwezige fysieke geheugen zonder extra dependency gedetecteerd.

De policy maakt onderscheid tussen:

- netwerkwerk (kaart- en PDOK-verzoeken);
- lokale geometrie- en padtaken;
- lichte kaart-/adresassets;
- normale template-assets;
- zware high-resolution raster/TIFF-taken;
- tile- en WMS-geheugencaches.

Zware rastertaken worden bewust veel conservatiever opgeschaald dan netwerkwerk. Een goedkope laptop houdt zo lage piek-RAM en blijft responsief, terwijl een workstation meerdere onafhankelijke exports tegelijk kan voorbereiden.

## Profielen

- `auto`: aanbevolen; schaalt automatisch op CPU en RAM.
- `eco`: minimaal achtergrondparallelisme en één zware rasterworker.
- `balanced`: begrensd parallelisme voor machines waarop tegelijk andere zware software draait.
- `performance`: agressievere lichte/netwerktaken; zware rastertaken blijven door RAM begrensd.

Voorbeeld op Windows PowerShell:

```powershell
$env:SLEUFBASE_RESOURCE_MODE = "performance"
.\SleufBase.exe
```

## Handmatige overrides

Voor diagnose, benchmarks en bijzondere werkstations zijn de automatische waarden afzonderlijk te begrenzen:

- `SLEUFBASE_MAX_WORKERS` — globale bovengrens voor alle workerpools;
- `SLEUFBASE_NETWORK_WORKERS` — netwerk-/tileworkers;
- `SLEUFBASE_GEOMETRY_WORKERS` — lokale geometrie/padworkers;
- `SLEUFBASE_MAP_WORKERS` — lichte templatekaartworkers;
- `SLEUFBASE_TEMPLATE_WORKERS` — normale template-assetworkers;
- `SLEUFBASE_RASTER_WORKERS` — zware high-resolution rasterworkers;
- `SLEUFBASE_TIFF_PIXEL_BUDGET` — maximaal geschat aantal bronpixels dat zware virtuele TIFF-taken gelijktijdig mogen reserveren;
- `SLEUFBASE_TILE_CACHE_ENTRIES` — aantal raw RGBA kaarttiles per tileclient;
- `SLEUFBASE_WMS_CACHE_MB` — WMS-cachebudget in MiB.

Expliciete overrides zijn bedoeld voor benchmarking/troubleshooting. Voor normale gebruikers hoort `auto` de beste combinatie van snelheid, geheugengebruik en UI-responsiviteit te leveren.

## Kaartweergave en achtergrondtaken

- Gelijktijdige aanvragen van dezelfde kaarttegel delen één download inclusief retries. Elke aanvrager krijgt een eigen beeld; onafhankelijke tegels blijven parallel laden.
- Tegel- en WMS-afmetingen worden vóór beeldconversie gecontroleerd. Een foutief serverbeeld wordt opnieuw geprobeerd en komt niet als geldig kaartbeeld in de cache.
- TIFF-lagen buiten het zichtbare kaartgebied worden vóór affine beeldbewerking overgeslagen. Hun extra native RGBA-cache wordt vrijgegeven bij het verschuiven van de kaart; zichtbare lagen behouden hun herbruikbare cache.
- De jobs-browser verwerkt achtergrondresultaten en voortgang op de UI-thread. Voortgang tussen twee UI-polls wordt samengevoegd tot de nieuwste melding; het sluiten van het venster verwijdert wachtende callbacks.
- KickTheMap-downloadworkers sluiten hun eigen netwerksessies na zowel succesvolle als mislukte downloads.

De regressietests meten het vermeden werk rechtstreeks: twee overlappende tegelverzoeken doen één netwerkdownload, acht affine TIFF-lagen buiten beeld doen nul beeldconversies en het TIFF-cachebudget blijft gelden tijdens het navigeren.

## Correctheidsgrenzen

Performance-optimalisaties mogen de inhoud van een export niet wijzigen. In het bijzonder blijven normale en reverse dwarsprofielen zelfstandig opgebouwd. De resourcepolicy verandert alleen scheduling, concurrency en cachebudgetten; niet de DXF-geometrie, maaiveldberekening, labels of reverse-semantiek.

## Ontwikkelregels

Nieuwe zware taken horen geen losse hard-coded `max_workers` te introduceren als hun optimale parallelisme hardware-afhankelijk is. Gebruik de centrale `resource_policy.py` en kies de workloadcategorie die het dichtst bij de nieuwe taak ligt. Voor geheugenintensieve rasteroperaties moet naast een workercap ook een geheugen/pixelbudget worden toegepast waar de tijdelijke buffers groot kunnen worden.
