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
Camera-/selectiegegevens worden voor uitwisseling bewaard; volledige toepassing
van die voorkeuren op de native kaart blijft afzonderlijk werk.

Lokale HTTP-proeven dekken CSRF-verversing, sessieverlies, retries, annulering en
ingebedde joblijsten. Echte accounts, praktijkprojecten en AutoCAD-acceptatie
blijven verplicht volgens `rust/PARITY.md`.
