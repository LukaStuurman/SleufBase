# SleufBase bytecode -> broncode migratie

## Doel

De vijf Python 3.11-bytecodemodules in `_bytecode/` gecontroleerd vervangen door
reviewbare Python-broncode zonder big-bang rewrite en zonder stille regressies.

Doelmodules, in migratievolgorde:

1. `streetsmart`
2. `streetsmart_browser`
3. `streetsmart_panel`
4. `settings`
5. `app`

De productie-default blijft legacy-bytecode totdat een afzonderlijke module zijn
broncode-contract, regressietests en Windows smoke-test heeft gehaald.

## Veiligheidsregels

- Geen automatische source -> bytecode fallback. Een aangezette bronimplementatie
  moet hard falen als zij ontbreekt of niet importeert.
- Geen functionele vernieuwingen combineren met reconstructie van een module.
- Gegenereerde/degecompileerde bestanden onder `migration/recovered/` zijn alleen
  bewijsmateriaal en mogen nooit rechtstreeks door productie worden geïmporteerd.
- De `_source/`-variant van een module wordt pas toegevoegd na review en tests.
- `_bytecode/<module>.pyc` wordt pas verwijderd nadat source minimaal één volledige
  Windows build/smoke-test heeft doorlopen en rollback naar de vorige release kan.

## Tijdelijke source selector

`source_migration.py` gebruikt:

```text
SLEUFBASE_MIGRATION_SOURCE_MODULES=streetsmart
SLEUFBASE_MIGRATION_SOURCE_MODULES=streetsmart,streetsmart_browser
SLEUFBASE_MIGRATION_SOURCE_MODULES=all
```

Zonder variabele gebruikt SleufBase uitsluitend de bestaande legacy-bytecode.
Broncode wordt gezocht als `SleufBase._source.<module>`.

`all` is uitsluitend bedoeld voor de laatste integratiefase. Een ontbrekende
source-module geeft bewust `SourceMigrationError` en valt niet terug op bytecode.

## Fase 0 - characterization baseline

Status: **in uitvoering op `migration/source-bytecode-kernel`**

- [x] expliciete lijst van vijf bytecodemodules
- [x] source/legacy selector met legacy als default
- [x] tests voor selectie, onbekende modules en no-fallback gedrag
- [x] `tools/snapshot_legacy_api.py` voor API/signature/class contract
- [x] bytecode SHA-256/size inventory in dezelfde snapshot
- [x] deterministic nested-code disassembly als recovery floor
- [x] gepinde `depyf==0.20.0` recovery candidate generator
- [x] aparte GitHub Actions recoveryjob onder Python 3.11
- [ ] eerste volledige legacy API snapshot groen
- [ ] eerste recovery evidence commit groen

Exitcriterium: `migration/contracts/legacy_api.json` en recovery evidence zijn
reproduceerbaar en de migration-switchtests zijn groen.

## Fase 1 - StreetSmart core

Status: **voorbereid**

- [x] `streetsmart.py` door de migration selector laten laden
- [ ] recovered candidate reviewen
- [ ] gedragstests voor state load/save, URL's en foutpaden aanvullen
- [ ] reviewed implementatie naar `_source/streetsmart.py`
- [ ] legacy/source contractdiff groen
- [ ] source-mode Windows smoke-test groen
- [ ] source default maken
- [ ] na stabiliteitsrelease `streetsmart.cpython-311.pyc` verwijderen

## Fase 2 - StreetSmart browser

Status: **voorbereid**

- [x] `streetsmart_browser.py` door de migration selector laten laden
- [ ] recovered candidate reviewen
- [ ] bearer-capture/browser bootstrap contracttests
- [ ] reviewed implementatie naar `_source/streetsmart_browser.py`
- [ ] legacy/source contractdiff groen
- [ ] Windows browser smoke-test groen
- [ ] bytecode verwijderen na stabiliteitsrelease

## Fase 3 - StreetSmart panel

Status: **voorbereid**

- [x] `streetsmart_panel.py` door de migration selector laten laden
- [ ] recovered candidate reviewen
- [ ] panel lifecycle/state tests
- [ ] reviewed implementatie naar `_source/streetsmart_panel.py`
- [ ] legacy/source contractdiff groen
- [ ] bytecode verwijderen na stabiliteitsrelease

## Fase 4 - settings

Status: **voorbereid**

- [x] `settings.py` door de migration selector laten laden
- [ ] volledig `AppSettings`-contract vastleggen
- [ ] fixtures van oude settingsbestanden toevoegen
- [ ] reviewed source-model/storage implementatie maken
- [ ] `schema_version` + expliciete settingsmigraties toevoegen
- [ ] atomisch schrijven via tempbestand + `os.replace`
- [ ] load/save roundtrip voor legacy en source vergelijken
- [ ] source default maken
- [ ] settings-bytecode verwijderen

## Fase 5 - app foundation

Status: **scaffold voorbereid**

`tools/apply_core_migration_scaffold.py` vervangt exact de losse `marshal.loads`
loader in `app.py` door dezelfde centrale migration selector. De patch weigert te
werken als het verwachte anker gewijzigd is.

- [x] exact transformer-script
- [x] launcher kan legacy-bytecodevalidatie overslaan wanneer `app` source-mode
  expliciet geselecteerd is
- [ ] `KlicViewerApp` public/private contract snapshot vastleggen
- [ ] helpers/controller/UI/lifecycle in aparte source-subsystemen reconstrueren
- [ ] semantische legacy/source exportvergelijking
- [ ] Windows smoke-test source app
- [ ] source default maken

## Fase 6 - definitieve cut-over

Pas uitvoeren nadat alle vijf source-implementaties volledig groen zijn:

- [ ] `SLEUFBASE_MIGRATION_SOURCE_MODULES` en tijdelijke selector verwijderen
- [ ] `legacy_bytecode.py` verwijderen
- [ ] `_bytecode/` verwijderen
- [ ] launcher legacy preflight verwijderen
- [ ] quality gate laten falen op iedere productie-`marshal.loads`
- [ ] Python 3.12/3.13 matrix toevoegen
- [ ] `requirements-windows.txt` opmerking over Python 3.11-bytecode verwijderen
- [ ] `RELIABILITY_HARDENING.md` op DONE zetten

## Definition of Done per module

Een `.pyc` mag pas verdwijnen wanneer:

1. zichtbaar API-contract gelijk of bewust gemigreerd is;
2. characterizationtests groen zijn;
3. bestaande SleufBase regressietests groen zijn;
4. source-mode zonder legacy fallback draait;
5. frozen Windows build groen is;
6. relevante end-to-end/smoke-test groen is;
7. rollback naar de vorige release beschikbaar blijft.

Voor `app` gelden daarnaast expliciete regressiechecks voor DXF/template-export,
MarXact, beginpunten, autosave, KickTheMap en StreetSmart/Cyclomedia.
