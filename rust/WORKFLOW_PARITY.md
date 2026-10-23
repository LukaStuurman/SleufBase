# Oorspronkelijke gebruikersfuncties als werkvolgorde

De volledige doelstelling en releasevoorwaarden staan in [PARITY.md](PARITY.md).
Deze inventaris koppelt het werk nadrukkelijk aan bedieningen van de bestaande
app. Componenttests en extra CAD-mogelijkheden zijn geen bewijs dat de volledige
gebruikersworkflow af is.

[Het oorspronkelijke menu](tests/fixtures/workflows/original.json) komt uit de
hash-gecontroleerde Python 3.11-code van `KlicViewerApp._build_menu`, uitgevoerd
met alleen nepwidgets en callbacknamen. De app, accounts en instellingen worden
niet gestart. De drie aanvullende menu-hooks in `app.py` zijn statisch opgenomen.
Opstartpatches in andere modules en niet-menu-interacties blijven afzonderlijk
te controleren; deze inventaris pretendeert geen volledige acceptatie.

| Bestaande bediening | Native koppeling en nog vereist |
| --- | --- |
| Project laden/opslaan/opslaan als | Native worker, gewijzigde-data-beveiliging en atomaire opslag; volledige Python-voorkeuren, terugschrijven en echte projectacceptatie blijven open |
| Laad GeoTIFF(s), laad KLIC DXF, zoom naar alles | Native workers en kaart; fysieke TIFFs kunnen links/rechts, met eigen hoek, reset of via de oorspronkelijke rotatiecirkel worden gedraaid. Basis/hoek blijven in projecten; pixels/texturen en meetpunten blijven behouden. Oorspronkelijke affine-/ring-/pointercontracten en echte egui-/project-/exportproeven controleren de koppeling. Knoppen blijven bij 900/1200/1500 px bereikbaar. Volledige rastermodi/resoluties, virtuele kaarttools en praktijkacceptatie blijven open |
| Locatie | Adres-/plaats-/wegzoeken, RD/GPS, suggesties, toetsenbord, tijdelijke markering, typegebonden schaal en luchtfoto zijn aangesloten; onafhankelijke coördinaat-/provider- en echte egui-proeven |
| Toon TIFF-adressen | Native overzicht met bronvolgorde, eerste adres/afstand, fout-/leegmeldingen, voortgang/annulering, vernieuwen en beide oorspronkelijke kopieeracties; oorspronkelijke rij-/clipboardcontracten en echte GeoTIFF-/egui-/projectwisselproeven controleren de koppeling |
| Export kaarten | Zelfstandig exportvenster voor alle geladen TIFFs, oorspronkelijke A4/RGB/DPI/70%-dekking, PDOK/OSM/Kadaster/offline, CAD-keuze, voortgang/annulering, hergebruikte worker en bronteruggave; dubbele namen en late fouten beschermd. Kadaster gebruikt de oorspronkelijke WMTS-zoom/uitsnede en alle beeldpixels worden onafhankelijk vergeleken; echte egui-keuze, lokale HTTP-/cache-/annuleerproeven en daadwerkelijke A4-export/foutgevallen toetsen de aansluiting. Volledige provider/cache-/rasterbediening en praktijkacceptatie blijven open |
| Kadastrale export DXF | Native object-/extra-lagenexport en sjabloonpaden; volledige oorspronkelijke keuzecombinaties en praktijkacceptatie blijven open |
| Proefsleuven sjabloon DXF | Beide native planners, profielen, Normaal/Reverse, datum/projectnaam en rasterbundels zijn aangesloten; oorspronkelijke fotoannotaties/maskers/logo/contact en volledige acceptatie blijven open |
| Wis alle referentiepunten / laad alle beginpunten | GeoTIFF-selectie, referentie plaatsen/verplaatsen op de echte kaart, gemeten beginpunt kiezen, alle referenties wissen en beginpunten laden zijn aangesloten. Objectdata wordt lokaal hergebruikt of via de bestaande ingelogde downloadwachtrij geladen; oorspronkelijke BRT/BGT-context bepaalt de richting. Projectmetadata en beide planners gebruiken deze punten. Oorspronkelijke punt-/providercontracten en echte egui-/workerproeven controleren de koppeling; virtuele referentiebewerking, alle echte accountvarianten en praktijkacceptatie blijven open |
| Wis opmerkingen / wis alles | Beide kaartacties zijn aangesloten. Wissen omvat vrije kaart- én TIFF-opmerkingen; TIFF-notities kunnen worden bewerkt/verwijderd en gaan mee naar beide sjabloonroutes. Wis alles behoudt projectidentiteit/instellingen/aanmelding, leegt geladen data en sluit aan op onopgeslagen-werk-/workerbewaking. Oorspronkelijke comment-/resetcontracten, echte egui-klikken, projectbehoud, geschreven gepaarde uitvoer en late achtergrondresultaten worden gecontroleerd. Vrije opmerkingen bewerken/slepen op de kaart en volledige praktijkacceptatie blijven open |
| KickTheMap inloggen/jobs/objectexport/browser/uitloggen | Native manager, geïsoleerde browser en downloads bestaan; alle echte provider-/fallback-/accountvarianten en gebruiksacceptatie blijven open |
| StreetSmart inloggen/browser/uitloggen/selector | Op gebruikersverzoek van 22 oktober 2026 vervallen voor Rust; geen venster of port vereist |
| Open instellingen | Native regels/autosave/exportopties bestaan; alle overige voorkeuren en behoud bij migratie blijven open |
| Maaiveld via Gemma | AHN-hoogteproef bestaat; oorspronkelijke AI-/profielworkflow blijft open |

De daadwerkelijke oorspronkelijke DXF-kaartadapter in `dxf.py` levert alleen
LINE, LWPOLYLINE, POLYLINE, ARC, CIRCLE, ELLIPSE, SPLINE en INSERT-markeringen.
TEXT/ATTRIB/MTEXT-contouren en mesh-randen in de native kaart zijn uitbreidingen
boven die oorspronkelijke adapter. Sjabloonbehoud en het kopiëren van CAD-objecten
vormen een afzonderlijk, ruimer contract en blijven onverminderd verplicht.
Deze broncontrole bepaalt de werkvolgorde: ontbrekende bestaande gebruikersacties
krijgen voorrang. Er zijn geen bestaande functies of releasevoorwaarden afgevinkt
op grond van deze statische inventaris.

Reproduceren zonder appopstart:

```
python tools/generate_workflow_contract.py
git diff --exit-code -- rust/tests/fixtures/workflows/original.json
```
