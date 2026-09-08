# SleufBase bytecode-migratie

## Doel

De legacy Python 3.11-bytecode in `_bytecode/` wordt stapsgewijs vervangen door normale, testbare Python-broncode. De migratie is bewust geen big-bang rewrite: iedere module krijgt eerst een vastgelegd referentiecontract, daarna een broncode-implementatie, vervolgens dual-mode tests en pas daarna verwijdering van de `.pyc`.

## Veiligheidsregels

1. Een module blijft standaard `legacy` gebruiken totdat de betreffende broncode-implementatie characterization-, regressie- en relevante Windows-smoke-tests doorstaat.
2. Er is geen automatische fallback van `source` naar `legacy`. Een fout in source-mode moet zichtbaar blijven.
3. Een legacy `.pyc` mag alleen bewust worden vervangen; `tests/characterization/legacy_bytecode_contract.json` bevriest de huidige bytes op grootte en Git blob-hash.
4. Iedere resterende legacy `.pyc` wordt vóór `marshal`/`exec` runtime gevalideerd tegen `_bytecode/legacy_bytecode_manifest.json` op bestandsgrootte en SHA-256. Een ontbrekend of afwijkend manifest faalt gesloten.
5. Nieuwe functionaliteit wordt niet gecombineerd met een bytecode-naar-source omzetting. Eerst gedragspariteit, daarna refactoring.
6. `app.py` wordt als laatste gemigreerd. De kleinere StreetSmart-modules bewijzen eerst het proces.

## Migratieschakelaars

De tijdelijke switches zijn environment variables. Geldige waarden zijn `legacy` en `source` (`bytecode` en `python` zijn aliases):

- `SLEUFBASE_MIGRATION_SETTINGS`
- `SLEUFBASE_MIGRATION_STREETSMART`
- `SLEUFBASE_MIGRATION_STREETSMART_BROWSER`
- `SLEUFBASE_MIGRATION_STREETSMART_PANEL`
- `SLEUFBASE_MIGRATION_APP`

Niet ingestelde variabelen gebruiken de per-module default uit `source_migration.DEFAULT_IMPLEMENTATIONS`. `streetsmart` gebruikt inmiddels standaard `source`; de overige migratiedoelen blijven voorlopig `legacy`.

`source_migration.py` bevat de centrale selectie. Een wrapper die nog geen broncode-implementatie heeft, faalt expliciet als `source` wordt geselecteerd. Ook `app.py` gebruikt inmiddels deze centrale route voor zijn nog-legacy implementatie; een losse app-specifieke `marshal`-loader is niet meer toegestaan.

## Characterization-baseline

De huidige baseline bestaat uit vijf Python 3.11-bestanden:

| Module | Bestand | Grootte |
| --- | --- | ---: |
| app | `_bytecode/app.cpython-311.pyc` | 731007 bytes |
| settings | `_bytecode/settings.cpython-311.pyc` | 25683 bytes |
| streetsmart | `_bytecode/streetsmart.cpython-311.pyc` | 24570 bytes |
| streetsmart_browser | `_bytecode/streetsmart_browser.cpython-311.pyc` | 8667 bytes |
| streetsmart_panel | `_bytecode/streetsmart_panel.cpython-311.pyc` | 25263 bytes |

Controleer de baseline met:

```bash
python tools/snapshot_legacy_api.py --check
```

Dezelfde bestanden staan voor runtimegebruik met hun SHA-256 en grootte in `_bytecode/legacy_bytecode_manifest.json`. Omdat de snapshottool via `legacy_bytecode.py` leest, faalt de contractcheck ook wanneer een runtime-hash niet meer overeenkomt.

Een volledige statische snapshot van code-objecten, namen, signatures en SHA256 kan zonder uitvoering van de applicatie worden geschreven met:

```bash
python tools/snapshot_legacy_api.py --output legacy-api-snapshot.json
```

Deze snapshot is bedoeld als hulpmiddel bij reconstructie; de regressie- en characterization-tests blijven de functionele waarheid.

## Fasen

### Fase 0 — baseline en migratie-infrastructuur

Status: **afgerond**.

- bytecodecontract vastgelegd;
- statische snapshottool toegevoegd;
- expliciete legacy/source-selectie toegevoegd;
- fail-closed source-mode getest;
- wrappers voorbereid op dual-mode tests;
- runtime SHA-256-integriteitsmanifest toegevoegd voor alle resterende legacy-bytecode;
- `app.py` gebruikt dezelfde centrale legacy/source-loader in plaats van een eigen `marshal`-loader;
- CI blokkeert nieuwe losse bytecode-loaders;
- de volledige regressiesuite draait zowel in de actuele productie-migratiemodus als in volledige legacy-rollbackmodus;
- CI controleert dat de baseline niet ongemerkt verandert.

### Fase 1 — `streetsmart.py`

Status: **source actief; legacy rollback tijdelijk behouden**.

- [x] Publieke exports en state/save/load-contracten vastgelegd uit de echte Python 3.11-bytecode.
- [x] Functionaliteit gereconstrueerd in normale Python-broncode (`streetsmart_source.py`).
- [x] Wrapper voorzien van een expliciete `source_loader`.
- [x] Legacy en source rechtstreeks vergeleken op signatures, constants, state, opslagpaden, cleanup, selectie-URL's en login-JavaScript.
- [x] `source` is de standaard voor `streetsmart`.
- [x] Quality Gate, Windows frozen executables en installer-smoke-tests zijn groen met de source-default.
- [ ] Houd `streetsmart.cpython-311.pyc` nog één stabiele release als expliciete rollback.
- [ ] Verwijder daarna de StreetSmart `.pyc` en de tijdelijke switch voor deze module.

Rollback tijdens deze overgang:

```text
SLEUFBASE_MIGRATION_STREETSMART=legacy
```

### Fase 2 — `streetsmart_browser.py`

Volgende implementatiestap. Zelfde patroon. Extra regressies: browser bootstrap, bearer-token capture, login/reopen en shutdown.

### Fase 3 — `streetsmart_panel.py`

Zelfde patroon. Extra regressies: open/sluit, selectie, state restore, resize en refresh.

### Fase 4 — `settings.py`

Tijdens gedragspariteit wordt alle huidige settingsfunctionaliteit eerst gereproduceerd. Daarna kan de broncode intern worden opgesplitst in `settings/model.py`, `settings/storage.py`, `settings/migrations.py` en `settings/defaults.py`.

Belangrijke regressies:

- oude settingsbestanden laden;
- onbekende keys behouden/negeren volgens huidig gedrag;
- corrupte JSON;
- KickTheMap-keuzes;
- MarXact mappings;
- BGT-opties;
- save/load roundtrip.

### Fase 5 — `app.py` contract inventariseren

Maak vóór reconstructie een machineleesbaar contract van `KlicViewerApp`, inclusief methods waar huidige patchmodules op vertrouwen. Verplaats nog geen gedrag.

### Fase 6 — `app.py` per subsystem naar broncode

Aanbevolen volgorde:

1. pure helpers/constants;
2. file/load-logica;
3. export orchestration;
4. externe integraties;
5. GUI;
6. startup/shutdown/threads.

De gereconstrueerde code mag niet opnieuw één gigantisch bestand worden. Splits tijdens de omzetting in expliciete controllers/services/modules, maar behoud eerst het bestaande externe gedrag.

### Fase 7 — dual-mode app-validatie

Vergelijk legacy- en source-uitvoer semantisch op vaste fixtures: layers, blocks, entities, coördinaten, metadata, XDATA, dynamic-block-data, MarXact-geometrie en maaiveldwaarden.

### Fase 8 — definitieve cut-over

Pas nadat alle source-implementaties stabiel zijn:

- verwijder `_bytecode/`;
- verwijder `legacy_bytecode.py`;
- verwijder tijdelijke migratieschakelaars;
- verwijder launcher-validatie voor legacy app-bytecode;
- behoud de CI-regel dat iedere toekomstige losse `marshal.load`/`marshal.loads` of getrackte `.pyc` buiten normale buildcache een fout is.

## Definition of Done per module

Een `.pyc` mag pas weg als:

1. relevante exports zijn gereproduceerd;
2. characterization-tests groen zijn;
3. bestaande SleufBase-tests groen zijn;
4. source-mode zonder legacy fallback draait;
5. Windows frozen build werkt;
6. relevante smoke/workflowtests werken;
7. rollback naar de vorige release mogelijk blijft.

Voor `app.py` gelden daarnaast expliciete regressiechecks voor DXF/MarXact/template-export, startpunten, autosave/recovery, KickTheMap en StreetSmart/Cyclomedia.

## Eerstvolgende implementatiestap

Na deze CI- en integriteitshardening is `streetsmart_browser.py` het volgende bytecodebestand. `app.py` blijft bewust legacy totdat de kleinere modules het dual-mode proces succesvol hebben doorlopen.
