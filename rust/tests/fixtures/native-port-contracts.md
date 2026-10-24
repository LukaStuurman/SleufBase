# Kaart, AHN, sjablonen en projectuitwisseling

Deze aanvullende controles vergelijken native gedrag met de oorspronkelijke
Python-functies. De generators starten geen app en lezen geen accounts of
gebruikersinstellingen. De live MarXact-renderpatch wordt niet geïnstalleerd.

| Referentie | Bewijs |
| --- | --- |
| `ahn` | 96 oorspronkelijke TIFF/NoData/nearest-pixelgevallen, binaire afronding en cachesleutels |
| `virtual-tools` | 48 astransformaties/kopieën, 384 dichtstbijzijnde-puntkeuzes en 48 echte oorspronkelijke AHN-wachtrijupdates met nepapp |
| `virtual-raster` | Alle 26.495.653 RGBA-pixels van 16 renders, lokale doorsneden, maskers, puntkleuren en overlappende dekbanden; 13 kleurconversies |
| `dekband` | Beide profielrichtingen, ongelijke diepten, clipping en naamgeving na markeruitbreiding van de oorspronkelijke profielas |
| `template-branding` | 10 logo's op oorspronkelijke layouts en exacte adresreparatie; native productie-export voegt twee gebruikte papierbladen toe |
| `template-underlays` | 60 PDF/DGN/DWF-underlays na drie bladuitbreidingen, inclusief gedeelde definities en wederzijdse reactors |
| `preferences.json` | 34 oorspronkelijke voorkeurtoestanden en Unicode-/lijst-/mappingnormalisatie |
| `live-jobs.json` | Vijf oorspronkelijke jobparsergevallen en twaalf inline-fallbackcontracten |

Reproduceren met Python 3.11, Pillow 11.3 en de vastgelegde ezdxf-versie:

```
python tools/generate_ahn_fixtures.py
python tools/generate_virtual_tool_fixtures.py
python tools/generate_virtual_raster_fixtures.py
python tools/generate_dekband_fixtures.py
python tools/generate_template_branding_fixtures.py
python tools/generate_template_underlay_fixtures.py
python tools/generate_preferences_fixture.py
python tools/generate_live_jobs_fixture.py
```

Gebruik de `SLEUFBASE_*_FIXTURES`-uitvoermappen uit de generators om naar een
aparte map te schrijven tijdens een build. Voorkeuren en live jobs gebruiken
respectievelijk `SLEUFBASE_PREFERENCES_FIXTURE` en `SLEUFBASE_LIVE_JOBS_FIXTURE`.
Een verse vergelijking op 23 oktober 2026 leverde 124 identieke bestanden op.

`export_template_branding` en `export_template_underlays` maken echte native
DXF's voor de gelijknamige onafhankelijke verifiers. De uitgebreide
`export_paired_templates`/`verify_native_paired_templates.py` vergelijken ook
daadwerkelijk geschreven oorspronkelijke dekbandprofielen, beide richtingen,
met losse en gepaarde native uitvoer. De voorbeeldinvoer houdt bewust het oude
manifest zonder expliciete `Profile.virtual_trench` om compatibiliteit te testen.

De desktop-AHN-actie werkt in het geopende concept. Annuleren, een verouderd
antwoord, verbindingsverlies en het sluiten van de editor behouden het project.
Handmatige/gemeten hoogtes blijven beschermd; aan het beginmaaiveld gekoppelde
objecten behouden hun diepte. Dit is geen bewijs voor de volledige Gemma-/AI-route
of automatische verversing bij iedere kaartbewerking.

`--export-python nieuw.klicproject native.json` schrijft uitsluitend een nieuw
bestand. Onbekende metadata, ontbrekende/onleesbare bronverwijzingen en
oorspronkelijke puntvelden blijven behouden. Dubbelzinnige herkomst of een niet
verliesvrij ondersteunde bron geeft een fout. Losse jobdatasets, BGT/Kadaster en
gewijzigde Rust-sjabloonvoorkeuren/slotvolgorde zijn nog niet terug te converteren.
De native kaart bewaart en herstelt deze camera- en selectiegegevens; de onderstaande contracten onderscheiden dit van praktijkacceptatie.

Lokale HTTP-proeven dekken CSRF-verversing, sessieverlies, retries, annulering en
ingebedde joblijsten. Echte accounts, praktijkprojecten en AutoCAD-acceptatie
blijven verplicht volgens `rust/PARITY.md`.

## Projectweergave en Python-projectexport

Het native projectformaat is versie **5**. Bestanden uit versies 1 tot en met 4
worden bij openen opgewaardeerd in het geheugen; hun oorspronkelijke bytes worden
niet herschreven. `rust/tests/project_view.rs`, `project_schema.rs`,
`cad_properties.rs` en `template_order.rs` toetsen deze compatibiliteit en het
weigeren van onbekende native velden.

`ProjectView` bewaart de zichtbare wereldcoördinaten van het kaartmidden, meters
per schermpixel, TIFF-dekking, achtergrond aan/uit, bron en geselecteerde lagen.
TIFF-/sleufselectie gebruikt bronpad of sleufidentiteit; DXF-selectie gebruikt het
bronpad. Oorspronkelijke Python-indices worden vóór de splitsing in native
raster- en sleuflijsten opgelost. Ze zijn afzonderlijk beschikbaar via
`ProjectUiState::from_archive`; exportindices verwijzen naar de daadwerkelijke
exportvolgorde. Dubbele of ontbrekende identiteiten kiezen geen andere laag.
Bekende oude Cyclomedia-voorkeuren worden actief naar PDOK omgezet; het originele
archief blijft ongewijzigd. Werkelijk onbekende bronnen blijven herkenbaar en
worden niet stil als PDOK geladen.

Projectvoorkeuren overschrijven alleen opgeslagen velden; ontbrekende velden
behouden de algemene gebruikersvoorkeur. Projectgebonden instellingen en
opgeslagen sjabloonkeuzes krijgen voorrang zonder algemene instellingenbestanden
te wijzigen. De UI-proeven in `ui_project_view.rs` gebruiken geen automatisch
netwerkverkeer. Ze toetsen onder andere selectie in echte widgets,
weergaveherstel, de wachtrij voor sluiten tijdens export en het blokkeren van
opslaan zolang de worker eigenaar is van de brongegevens.

De Python-exportworker verplaatst grote bronvectoren en geeft dezelfde
allocaties terug bij succes, fout, annulering of opgevangen paniek. Export maakt
uitsluitend een nieuw `.klicproject`. JSON wordt gecontroleerd naar een tijdelijk
bestand gestreamd, met maximaal 128 MiB inclusief de afsluitende newline. De
publicatiegrens laat annulering en publiceren niet allebei winnen. Tests in
`project_exchange_worker.rs`, `legacy_project.rs` en de writer-module toetsen dit;
de CLI-proef importeert, exporteert en importeert opnieuw, inclusief bescherming
van bron en bestaande uitvoer.

Losse jobdatasets, actieve BGT/Kadastergegevens en gewijzigde native
sjablooninstellingen/slotvolgorde blijven een expliciete conversiefout. Een
gewijzigde rastertransformatie zonder rotatie wordt eveneens geweigerd: de
originele Python-loader negeert de opgeslagen basismatrix bij een nulhoek.
Onbekende dekbandvelden blijven behouden bij een unieke ongewijzigde koppeling;
een gewijzigde of dubbelzinnige koppeling wordt geweigerd. Dit voorkomt dat een
ogenschijnlijk geslaagde conversie actuele gegevens stil laat verdwijnen.

## Annulering van Kadaster en BGT

`fetch_cadastral_with_cancel` en `request_bgt_with_cancel` behouden de bestaande
wrappers. Annulering wordt gecontroleerd bij wachten op een verzoek, tussen
pagina's en tegels, tijdens retries en wachttijden en bij verwerking van
features/paden. Een vastgelopen HTTP-header of antwoordbody houdt de caller niet
vast: die controleert annulering tijdens het wachten met stappen van 20 ms.

Kadaster, BGT en de AI-kaart-/profiel-/Gemma-aanvragen delen maximaal **vier** achtergrondverzoeken. Een reeds lopend
socketverzoek kan na annulering nog maximaal de ingestelde HTTP-timeout van
**45 seconden** voor Kadaster/BGT of standaard **180 seconden** voor Gemma voortbestaan; de caller wacht daar niet op. Ook wachten op een
vrij verzoekslot is annuleerbaar. Antwoordpagina's blijven begrensd op 24 MiB;
veilige vervolg-URL's, paginalimieten en geometriebudgetten blijven gelden.

Lokale HTTP-proeven in `cadastral.rs` en `bgt.rs` controleren geblokkeerde bodies,
annulering vóór een tweede retry, normale WFS-paginering en ontdubbeling, herstel
na een tijdelijke BGT-fout en weigering van onveilige vervolg-URL's. De
annuleringsgrens is één seconde tegenover een drie seconden geblokkeerde
nepserver; deze marge voorkomt afhankelijkheid van exacte planning in een zware
testrun. De testclients gebruiken uitsluitend localhost en omzeilen proxies.

De originele standalone overzichtsexport haalt verse Kadaster-lijnen én labels
op binnen de gezamenlijke TIFF-uitsnede met marge
`max(60, min(180, grootste_spanwijdte * 0.35))` meter. Optionele verse BGT-wegdelen
gebruiken dezelfde uitsnede en de laag `BGT_WEGDEEL`; deze optie staat oorspronkelijk
standaard uit. Alleen geladen gegevens gebruiken is een afzonderlijke expliciete
offlinekeuze. Grenzen van de ondersteunde aanvraaggrootte blijven zichtbare fouten.

Deze contracten bewijzen lokaal gedrag en vergelijking met oorspronkelijke
functies. Zij bewijzen geen recente beschikbaarheid van PDOK, werking met echte
KickTheMap-accounts, pixelpariteit van iedere achtergrondroute of correcte
praktijkuitvoer in AutoCAD. De Kadaster-WMS-bron gebruikt het oorspronkelijke
exportendpoint; de oorspronkelijke kaartpreview gebruikte WMTS. Live acceptatie
met representatieve projecten blijft afzonderlijk nodig.
