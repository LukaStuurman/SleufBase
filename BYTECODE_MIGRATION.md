# SleufBase bytecode-migratie

## Doel

De legacy Python 3.11-bytecode in `_bytecode/` wordt stapsgewijs vervangen door normale, testbare Python-broncode. De migratie is bewust geen big-bang rewrite: iedere module krijgt eerst een vastgelegd referentiecontract, daarna een broncode-implementatie, vervolgens dual-mode tests en pas daarna verwijdering van de `.pyc`.

## Veiligheidsregels

1. Productie blijft standaard `legacy` gebruiken totdat de betreffende broncode-implementatie characterization- en regressietests doorstaat.
2. Er is geen automatische fallback van `source` naar `legacy`. Een fout in source-mode moet zichtbaar blijven.
3. Een legacy `.pyc` mag alleen bewust worden vervangen; `tests/characterization/legacy_bytecode_contract.json` bevriest de huidige bytes op grootte en Git blob-hash.
4. Nieuwe functionaliteit wordt niet gecombineerd met een bytecode-naar-source omzetting. Eerst gedragspariteit, daarna refactoring.
5. `app.py` wordt als laatste gemigreerd. De kleinere StreetSmart-modules bewijzen eerst het proces.

## Migratieschakelaars

De tijdelijke switches zijn environment variables. Geldige waarden zijn `legacy` en `source` (`bytecode` en `python` zijn aliases):

- `SLEUFBASE_MIGRATION_SETTINGS`
- `SLEUFBASE_MIGRATION_STREETSMART`
- `SLEUFBASE_MIGRATION_STREETSMART_BROWSER`
- `SLEUFBASE_MIGRATION_STREETSMART_PANEL`
- `SLEUFBASE_MIGRATION_APP`

Niet ingestelde variabelen gebruiken altijd `legacy`.

`source_migration.py` bevat de centrale selectie. Een wrapper die nog geen broncode-implementatie heeft, faalt expliciet als `source` wordt geselecteerd.

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

Een volledige statische snapshot van code-objecten, namen, signatures en SHA256 kan zonder uitvoering van de applicatie worden geschreven met:

```bash
python tools/snapshot_legacy_api.py --output legacy-api-snapshot.json
```

Deze snapshot is bedoeld als hulpmiddel bij reconstructie; de regressie- en characterization-tests blijven de functionele waarheid.

## Fasen

### Fase 0 — baseline en migratie-infrastructuur

Status: **in uitvoering / foundation aanwezig**.

- bytecodecontract vastleggen;
- statische snapshottool toevoegen;
- expliciete legacy/source-selectie toevoegen;
- fail-closed source-mode testen;
- wrappers voorbereiden op dual-mode tests;
- CI laten controleren dat de baseline niet ongemerkt verandert.

Exitcriterium: productiegedrag is ongewijzigd en alle bestaande tests blijven groen.

### Fase 1 — `streetsmart.py`

1. Leg de publieke exports en state/save/load-contracten vast.
2. Reconstrueer dezelfde functionaliteit in normale Python-broncode.
3. Geef de wrapper een `source_loader`.
4. Draai characterization-tests in `legacy` en `source`.
5. Maak `source` de standaard nadat Windows smoke-tests groen zijn.
6. Houd legacy nog één release als expliciete rollback.
7. Verwijder daarna `streetsmart.cpython-311.pyc` en de tijdelijke switch voor deze module.

### Fase 2 — `streetsmart_browser.py`

Zelfde patroon. Extra regressies: browser bootstrap, bearer-token capture, login/reopen en shutdown.

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
- maak CI streng: iedere toekomstige `marshal.loads` of getrackte `.pyc` buiten normale buildcache is een fout.

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

Na deze foundation is `streetsmart.py` de eerste echte omzetting. `app.py` blijft bewust legacy totdat de kleinere modules het dual-mode proces succesvol hebben doorlopen.
