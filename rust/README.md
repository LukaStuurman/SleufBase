# SleufBase Rust (0.1.0-preview)

Dit is een aparte native Rust-desktopapp naast de bestaande SleufBase v0.3.62 (Python).
De Rust-versie is nog niet productiegelijkwaardig aan SleufBase.

## Wat er in deze preview zit

- Rust Windows-desktopvenster met Techbase-oranje accenten (egui/eframe).
- Openen van meerdere lokaal beschikbare KickTheMap jobFeatures.json bestanden.
- Parser voor handmatig geplaatste objectpunten en objectpolylijnen, inclusief optionele Z-coordinaten.
- Kaartvoorbeeld met RD-objecten, gegeorefereerde GeoTIFFs en asynchrone PDOK-luchtfoto.
- Instelbare objectregels voor kaartkleuren, disciplineoverzicht, Excel en DXF; standaardvolgorde en kleuren uit Python.
- Excel (.xlsx) export met oranje opmaak, vaste koprij, filter en totaalregel; gedeelde disciplinenamen tellen samen.
- R2018-object-DXF met gerichte puntlijnen, echte 3D-polylijnen, joblabels, discipline-lagen en oorspronkelijke XDATA.
- Profielrekenkern met 224 oorspronkelijke Python-referenties voor richting, hoogtes, kettingmaat, markerbreedtes en disciplinebeschrijvingen; de volledige profieltekening en sjabloonworkflow zijn nog niet aangesloten. Zie [de vergelijkingsgevallen](tests/fixtures/profiles/README.md).
- Native profielkader met automatische/handmatige schaal, titel-/referentieblokken, gekoppelde afstands-/hoogtebanden en verbindingslijnen. 24 oorspronkelijke tekengevallen vergelijken 928 echte DXF-objecten en alle attributen; 100 extra bandverdelingen, Unicode-nummers en afronding zijn vergeleken. Meetvertices blijven geleend en de stabiele plaatsingsvolgorde wordt hergebruikt. Maaiveld/markeringen/dynamische leaders en de volledige sjabloonworkflow blijven open. Zie [de profieltekencontroles](tests/fixtures/profile-drawing/README.md).
- Sjabloonbouwstenen bewaren het originele moderne ASCII-DXF-document, herkennen dezelfde 32 proefsleufvakken en bewerken alleen bedoelde teksttags. Alle overige tags worden onafhankelijk vergeleken; pagina-/profiel-/assetexport en UI-koppeling ontbreken. Zie [de sjablooncontroles](tests/fixtures/templates/README.md).
- Native sjabloonrasters met correcte IMAGE-definities/reactors en permanente assetbundles; 91 oorspronkelijke plaatsingen inclusief papierlogo's en alle PNG-pixels onafhankelijk gecontroleerd. Oude bundels blijven bereikbaar na vervanging; fouten/annulering beschermen eerdere uitvoer. Rastervoorbereiding en volledige sjabloon-/UI-export blijven open. Zie [de raster-/bundelcontroles](tests/fixtures/template-images/README.md).
- Native sjabloontekenlaag voor polylijnen, cirkels, tekst en bestaande profielblokken met automatische attributen. Brondefinities en ingelezen attribuutmetadata worden gedeeld; 89 originele object-/blokplaatsingen worden onafhankelijk gecontroleerd. Meerdere tekenbatches en een rasterbatch delen lagen/handles zonder documentherlezing. De volledige profielindeling en dynamische leaders blijven open. Zie [de tekencontroles](tests/fixtures/template-geometry/README.md).
- Titelblokkern vult projectnaam en lokale exportdatum op gebruikte bladen, met behoud van latere revisies. Onafhankelijke DXF-controle vergelijkt 78 titelvelden en alle overige tags. De bestaande Python-exportdialoog heeft het projectnaamveld al; het complete native exportvenster volgt binnen de migratie. Zie [de titelblokcontroles](tests/fixtures/template-metadata/README.md).
- Beveiliging tegen het overschrijven van bestaande uitvoerbestanden.
- Rust-unit-tests en Windows CI die een executable als build-artifact maakt.
- GeoTIFF-tags, affine RD-coordinaten, EPSG-key en KickTheMap sidecars lezen; TIFF-preview via begrensde strips/tegels als gedraaide/gegeorefereerde kaartlaag tonen.
- Rust filtert actieve jobs rechtstreeks uit de in de Rust-UI ingeladen objectgegevens en GeoTIFF-sidecars; een compatibiliteitsparser voor de Python-heartbeat blijft apart beschikbaar.

## GeoTIFF-preview zonder volledige bronkopie

- Een aparte decoder verwerkt strips/tegels achtereenvolgens naar een aspectgetrouwe preview (standaard maximaal 2048 × 2048). De bestaande bronlimiet is 150 miljoen pixels; bestanden boven de oude native limiet van 16 miljoen kunnen nu worden weergegeven.
- RGB(A), grijswaarden, paletten, CMYK en niet-gesubsamplede YCbCr zijn afgedekt met 47 synthetische fixtures, inclusief packed 1/2/4 bits, 16-bit, signed/float, planar, BigTIFF, oriëntaties en LZW/DEFLATE/PackBits/JPEG. JPEG heeft maximaal twee bytewaarden tolerantie; de andere ondersteunde referentiegevallen vergelijken exacte pixels.
- De referentie gebruikt Pillow 11.3.0, aangevuld met expliciete numerieke verwachtingen voor formaten die Pillow niet opent of verkeerd uitleest. Bij planar RGB16 gebruikt Rust de juiste samples waar Pillow bytes als pixels leest. Niet-eindige/NoData-pixels worden transparant.
- Het gedecodeerde chunkbudget is standaard 32 MiB. Een te grote afzonderlijke strip/tegel geeft een foutmelding. Dit budget begrenst geen totale proces- of GPU-geheugenpiek.
- Rasterpixels laden op één achtergrondthread. Projectwissels annuleren oude taken; late resultaten komen niet in een ander project. Fouten blijven per bestand zichtbaar en houden andere bestanden niet tegen.
- Bij veel rasters verdeelt de app een geschat RGBA-textuurbudget van 256 MiB over de previews. Dit is geen harde totale geheugenlimiet: decoderbuffers, tijdelijke uploads en drivergeheugen komen erbij.
- De lokale releasebenchmark op een RGB8-bron van 16,8 miljoen pixels levert dezelfde preview met circa 2,3× hogere snelheid en 86% lagere procespiek dan volledige Pillow-conversie. Methode en alle meetrondes staan in [benchmarks/README.md](benchmarks/README.md).
- Volledige resolutie bij inzoomen, overviews/pyramides, gesubsamplede YCbCr, overige kleurmodi en acceptatie op grote echte GeoTIFFs blijven open.

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

Nieuwe uitvoerbestandsnaam verplicht. De DXF-dialog laat geladen jobs en virtuele proefsleuven afzonderlijk selecteren.
De jobselectie in de gewone zijbalk wijzigt alleen kaartvoorbeeld en details. Excel gebruikt alle geladen jobs.

Zie [PARITY.md](PARITY.md) voor de volledige functiepariteit-checklist en releasecriteria.

## Nieuw in deze migratiestap

- ASCII- en binaire DXF als aparte kaartlaag openen (LINE, LWPOLYLINE, POLYLINE, ARC, CIRCLE en INSERT-marker); volledige blokken en overige entiteiten nog niet.
- Kaart verschuiven met slepen en zoomen met muiswiel.
- Eigen Rust-project opslaan/openen als `*.sleufbase-rust.json`, met brondata, GeoTIFF-metadata en DXF-lagen.
- Automatische roterende back-ups in een **aparte** Rust-map. Het bestaande Python-projectformaat wordt niet overschreven.
- Instellingen voor deze back-ups lezen, met dezelfde defaults 10 minuten en maximaal 20 back-ups.

## KickTheMap-objecten naar R2018-DXF

- De bestaande GUI-knop **DXF objecten** en CLI `--dxf` gebruiken dezelfde native R2018-encoder, in meters. Puntlijnen staan loodrecht op de richting van het eerste naar het laatste afwijkende meetpunt; polylijnen behouden één 3D-POLYLINE met oorspronkelijke Z, kleur, laag en `KTM_DXF_EXPORT`-metadata.
- Joblabels volgen de oorspronkelijke PS-/getalfallback, inclusief Unicode-decimale cijfers en woordgrenzen. Plaatsing ontwijkt andere jobs en eerdere labels volgens dezelfde kandidaatvolgorde. Labelhoogte, witte true-color, uitlijning en `LiberationMono-Regular.ttf`-stijl volgen de Python-export.
- Laagtabellen declareren de gevraagde disciplinekleur ook bij de eerste aanmaak. Dit herstelt een aangetoonde kleurafwijking in het Python-/ezdxf-aanroeppad; de objectkleuren blijven gelijk. Virtuele jobs sorteren meetobjecten stabiel tussen begin- en eindpunt, zodat de richting correct blijft.
- De encoder schrijft met een buffer van 64 KiB rechtstreeks naar een tijdelijk bestand. De GUI leent gewone geladen jobs zonder volledige objectkopie. Virtuele jobs en profielmarkeringen worden nog apart voorbereid. Publicatie is atomair en overschrijft geen bestaande uitvoer, ook bij races.
- Niet-eindige coördinaten, ongeldige laagnamen, besturingstekens/injectie en te lange XDATA geven een fout. XDATA-strings blijven binnen 255 UTF-8-bytes; de uitvoer is maximaal 256 MiB, met punt-/entiteitenbudgetten. Coördinaten worden niet naar acht decimalen afgerond. Dit is geen harde totale RAM-limiet.
- Veertien volledige Python-exportscenario’s, 87 labelgevallen, f64-rondgangen, fout-/opslagregressies en een onafhankelijke ezdxf-audit testen de uitvoer. CI controleert ook de echte native CLI en aanvullende proefsleuf-/Kadaster-/BGT-lagen zonder DXF-reparaties. Zie de [referenties](tests/fixtures/object-export/README.md).
- Een releasebenchmark met 100.000 objecten produceert exact gelijke DXF-bytes via buffering en streaming: de extra getraceerde Rust-heap daalt van circa 64 MiB naar 65 KiB. Metingen en precieze grenzen staan in [benchmarks/README.md](benchmarks/README.md). Dit meet de encoder, geen volledige GUI-/schijf- of Python-workflow.
- De standaard objectexport is verder geport. Overige exportopties in de UI, volledige bron-/groepsfilteracceptatie, sjabloon-/profieltekeningen en raster-/fotobundels blijven open. De genoemde fontstijl verwijst naar de fontnaam; dit voegt geen fontinstallatie toe.

## Objectexport: bronnen kiezen en overlay laden

- **DXF objecten** opent een doorzoekbare bronlijst met afzonderlijke selectie voor geladen jobs en virtuele proefsleuven. Nieuwe bronnen die tijdens de dialoog binnenkomen worden niet automatisch geselecteerd. Een projectwissel sluit de oude exportselectie.
- Labelafstand en puntlijnlengte zijn voor deze export instelbaar. De opgeslagen labelafstand staat onder **Instellingen** en wordt ook door de CLI gebruikt; standaard 3,5 meter, net als de oorspronkelijke app. De losse bibliotheekencoder houdt zijn eigen oorspronkelijke standaard van 6 meter.
- Begin- en eindpunten van virtuele proefsleuven zijn een expliciete keuze, standaard aan zoals in de oorspronkelijke Python-aanroep. De oude Python-dialogtekst meldde ten onrechte dat ze altijd werden weggelaten. Zonder eindpunten worden lege virtuele bronnen overgeslagen en gemeld.
- Aanvullende sleufgrenzen, dwarsmarkeringen en zichtbare Kadaster-/BGT-lagen zijn apart in te schakelen. Deze optie neemt alle aanvullende lagen mee; de bronselectie bepaalt de objectjobs. Alleen aanvullende geometrie exporteren blijft mogelijk.
- **Exporteren en laden als overlay** publiceert eerst de DXF en leest daarna het werkelijke bestand via de bestaande achtergrondlader. Annuleren van het laden verwijdert de opgeslagen DXF niet. Een mislukte export houdt de dialoog/selectie beschikbaar en vervangt geen bestaand bestand.
- GUI-exports voor DXF en Excel lopen op één afzonderlijke worker met een voortgangsscherm en **Annuleren** / Escape. Gewone datasets, virtuele proefsleuven en aanvullende kaartlagen worden tijdelijk overgedragen en ongewijzigd teruggegeven. Het overdrachtspad kopieert geen objectvectors; alleen geselecteerde virtuele jobs worden op de worker opgebouwd. Bewerken en het toepassen van netwerkresultaten wachten tot de gegevens terug zijn.
- Annulering wordt gecontroleerd bij object-/puntbatches en schrijfacties. Het tijdelijke bestand wordt pas na flush en synchronisatie gepubliceerd. Een atomische eindcontrole bepaalt of annuleren of publiceren wint; zodra definitief opslaan is begonnen, rondt Rust die actie af. Een systeemaanroep, sortering, label-/doorsnedefase of XML-/compressiefase kan nog aflopen. Dit is geen onmiddellijke stopgarantie of totale RAM-limiet.
- Sluiten en projectwissels tijdens export worden uitgesteld. Na terugkeer van de gegevens volgt de bestaande keuze voor onopgeslagen werk. Opslaan/back-ups schrijven geen tijdelijk leeg project terwijl de worker de brongegevens bezit. Fouten, afgevangen exportpanics en een geweigerde tweede taak behouden de brondata en vorige uitvoer; dezelfde worker kan opnieuw worden gebruikt.
- Online jobs rechtstreeks vanuit deze exportdialog kiezen/downloaden, volledige sjabloon-/profielworkflows en daadwerkelijke acceptatie op grote echte projecten blijven open.

## Objectregels en disciplineoverzicht

- Open **Instellingen → Objectregels en disciplinekleuren** om coderingen, CAD-laag, ACI-kleur en disciplinenaam te bewerken, regels te verplaatsen/verwijderen of standaardregels te herstellen. De eerste passende regel wint. Een lege lijst schakelt regeltoewijzing uit. Lege disciplinenamen gebruiken de eerste codering als label.
- Wijzigingen blijven een concept tot **Opslaan en toepassen** slaagt. Een ongeldige regel of opslagfout vervangt de actieve regels niet. Dezelfde opgeslagen regels sturen de kaart (ook MarXact-markeringen), het overzicht en de losse Excel-/DXF-export.
- **Import oude voorkeuren** neemt `kickthemap_object_layer_rules` over. Oudere Rust-instellingen zonder dit veld krijgen de oorspronkelijke app-defaults: dertien regels inclusief warmtenet en hoogspanning, met `ONBEKEND` voor monitoring. Expliciet opgeslagen regels blijven behouden. Instellingen zijn begrensd op 2 MiB; maximaal 256 regels en 32 KiB coderingen/label per regel. Ongeldige laag-/kleurnamen worden gemeld. De CLI meldt ongeldige instellingen als fout; de GUI toont een laadmelding en gebruikt dan tijdelijke standaardwaarden.
- De CLI gebruikt dezelfde eigen Rust-instellingen. Een afzonderlijk bestand is optioneel: `sleufbase-rust.exe --settings instellingen.json --xlsx overzicht.xlsx jobFeatures.json`. Dit bestand gebruikt het native Rust-instellingenschema; import van oude voorkeuren loopt via de importactie.
- Excel behoudt de oorspronkelijke disciplinevolgorde, kolombreedtes, rijhoogtes, kleuren, letteropmaak, vaste koprij, filter en totaalregel. Labels die volgens Python `casefold` gelijk zijn krijgen één kolom; alle aantallen worden daarin samengevoegd. Dit herstelt een Python-fout waarbij latere schrijfwijzen uit de totalen verdwenen. Tekst zoals `=1+1` blijft letterlijk.
- De Excel-export leent geladen jobs en berekent kolomgroepen eenmaal per export. Alleen virtuele jobs worden afzonderlijk opgebouwd. Unicode-hoofdletters en kolomgroepering volgen vaste Python 3.11/Unicode 14-tabellen.
- Tien onafhankelijke oorspronkelijke rapporten worden via de echte Rust-CLI vergeleken op cellen, opmaak, breedtes, filter en totalen; aangepaste DXF-lagen/-kleuren krijgen tevens een onafhankelijke CAD-audit. Zie de [disciplinereferenties](tests/fixtures/disciplines/README.md). Automatische Excel tijdens template-export, alle overige instellingen en volledige project-/Excel-renderacceptatie blijven open.

## Projectgegevens behouden

- De objectparser volgt de oorspronkelijke groepsfilters en Python-waardeconversie: selectie vóór trimmen, ontbrekend versus `null`, booleans/getallen en samengestelde eigenschappen met oorspronkelijke veldvolgorde. Unicode-decimale coördinaten en geldige numerieke underscores worden gelezen; niet-eindige XY vervalt en niet-eindige Z wordt onbekend.
- Objectregels ondersteunen Unicode-tekens, `*`, `?`, tekenklassen en de oorspronkelijke verwerking van ongeldige bereiken. De eerste passende regel blijft leidend. Standaard ASCII-regels normaliseren de bron niet meer voor elke regel. 107 parserdocumenten en 149.112 oorspronkelijke regelbeslissingen staan in de [parserreferenties](tests/fixtures/object-parser/README.md).
- De parser controleert object-/puntbudgetten en annulering tussen batches; één JSON-decodeerfase kan nog aflopen. Eigenschappen zijn maximaal 1 MiB. Dit bewijst nog geen volledige instellingen-/Excel-pariteit of acceptatie met echte projecten.

- Project openen/herstellen, Python-import, DXF toevoegen/herladen/herkoppelen, MarXact, jobFeatures en GeoTIFF-metadata laden via één achtergrondworker. Ook gedownloade bestanden gebruiken dit pad. Een voortgangsscherm houdt de interface beschikbaar voor annuleren; bewerken en het toepassen van netwerkresultaten wachten tot deze laadactie klaar is.
- Een laadactie bereidt gegevens, CAD-caches en project-/MarXact-dwarsmarkeringen voor voordat zij het project wijzigt. Annuleren verwerpt de hele batch; na een afgeronde batch worden geslaagde bestanden toegevoegd en fouten per bestand gemeld. Een mislukt project of DXF-herladen behoudt de huidige gegevens. Taaknummer én projectgeneratie weren late resultaten.
- De invoerwachtrij is begrensd en hergebruikt één worker. Annuleren wordt gecontroleerd bij leesblokken en parser-/objectgrenzen; een lopende systeemaanroep, JSON-decodering of afzonderlijke geometrie-/cachefase kan nog aflopen. Annuleren stopt de UI-laadactie direct, maar start geen extra lezer zolang de vorige worker nog bezig is. Projectopslag en back-ups zijn nog synchroon; GUI-DXF/Excel gebruiken een aparte worker; dit is geen totale RAM-limiet of volledige workflowacceptatie.
- Nieuw/openen/herstellen/sluiten vraagt bij onopgeslagen werk om opslaan, doorgaan zonder opslaan of annuleren. Een mislukte of geannuleerde opslag voert de projectwissel niet uit.
- Projectwissels koppelen oude PDOK/BGT/Kadaster/AHN-resultaten los. Een eerder gestarte KickTheMap-download blijft op schijf beschikbaar, maar wordt niet aan een inmiddels ander project toegevoegd.
- Projecten, back-ups, Excel en DXF worden eerst volledig naar een uniek tijdelijk bestand in dezelfde map geschreven. Export en CLI-conversie publiceren zonder overschrijven, ook als een ander proces tegelijkertijd dezelfde uitvoernaam kiest.
- Projectopslag controleert alle kaartgeometrie, labels, opmerkingen en rastertransformaties. Ongeldige gegevens vervangen geen bestaand bestand. Lezen én opslaan respecteren de projectlimiet van 128 MiB.
- jobFeatures-JSON en DXF-bronnen zijn maximaal 256 MiB. De gedeelde lezer controleert ook het werkelijk gelezen aantal bytes, zodat groei na de bestandscontrole de grens niet omzeilt; onderbroken systeemlezingen worden opnieuw geprobeerd.
- Onbekende velden in de objectstructuren van het native projectformaat worden geweigerd zodat deze preview ze niet stilzwijgend verwijdert bij opnieuw opslaan. De aparte Python-import bewaart het bron-JSON integraal.

## Bestaande Python-projecten importeren

- **Import Python-project** leest `.klicproject` versie 1 rechtstreeks in Rust, zonder Python-runtime. De import opent als onopgeslagen nieuw project; de bron blijft behouden.
- Virtuele en MarXact-proefsleuven, objectattributen, gemeten 3D-grenzen, automatische uitlijningsbasis, rotatie en job-ID's worden overgenomen. Ook opmerkingen, beschikbare GeoTIFFs met rasterrotatie en DXF-zichtbaarheid worden gelezen. Relatieve bronpaden worden vanaf de projectmap opgelost.
- Het **volledige oorspronkelijke JSON** blijft bewaard in het native project en de back-ups, inclusief onbekende metadata, sjablooninstellingen en slotvolgorde. Grote bronarchieven worden bij het maken van een snapshot gedeeld in geheugen.
- De zijbalk toont blijvende importmeldingen voor ontbrekende/niet leesbare assets en nog niet toegepaste UI-/sjablooninstellingen. Dit is **nog geen volledige Python-projectpariteit** en er is nog geen export terug naar het Python-formaat.
- De synthetische regressie-fixture is gemaakt met de gevalideerde oorspronkelijke Python-serializer, niet handmatig als vervangend schema. Opnieuw genereren: `py -3.11 tools/generate_legacy_project_fixture.py`.
- Batchconversie: `sleufbase-rust.exe --import-python nieuw-project.json oud-project.klicproject`. Bestaande uitvoer wordt ook bij gelijktijdige conversies niet overschreven.

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
- De cachestand is expliciet **niet live**. De eerste native online koppeling staat hieronder beschreven; browserlogin en volledig sessieherstel blijven open.

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

- Via **MarXact importeren** kan een ASCII- of binaire DXF met `MULTILEADER` labels, gesloten `POLYLINE` grenzen en `INSERT` meetpunten rechtstreeks worden geladen.
- Controleert het `Name` op iedere grensvertex, koppelt punten op maximaal 8 cm aan labels en wijst overlappende punten toe aan de kleinste gemeten grens.
- Bepaalt middenlijn en maaiveld-Z van de gemeten 3D-grens (met voorkeur voor meetpunt-richting), neemt laag-naar-discipline mapping over en bewaart de oorspronkelijke 3D-omtrek.
- Toont de gemeten contour op de kaart, neemt objecten mee in de basis-DXF/Excel en bewaart alles in een Rust-project.
- **Nog niet volledig:** overige vendorvarianten en volledige blok-/CAD-pariteit ontbreken nog. Geneste `MULTILEADER`-contexten zijn nu vergeleken met de oorspronkelijke Python-import op een synthetisch ezdxf-bestand; representatieve exportbestanden blijven vereist. Lokale dwarsprofielen, maatvoering, gekleurde en geclipte profielweergave en overige MarXact-workflows zijn nog niet volledig overgezet.

## MarXact-labels uit geneste CAD-contexten

- De DXF-lezer onderscheidt `CONTEXT_DATA`, `LEADER` en `LEADER_LINE`. Het eerste werkelijke pijlpunt wordt gebruikt; tekst-/landingspunten en scope-markeringen worden niet als meetpunten of labeltekst gelezen.
- Een synthetische R2018-DXF met elf echte geneste labels, meerdere pijllijnen, gemeten 3D-grens en INSERT-objecten wordt vergeleken met de oorspronkelijke Python-parser en ezdxf 1.4.4. De test controleert objecttoewijzing, originele coördinaten, tekst en hoogten.
- Open POLYLINEs worden overgeslagen als sleufgrens, ook bij labels op alle hoekpunten. Het oorspronkelijke gesloten-vlaggetje blijft in nieuwe Rust-projecten behouden; oudere opgeslagen overlays blijven leesbaar.
- Labeltekst volgt de oorspronkelijke eerste-matchregel en numerieke hoogteprefix, inclusief decimale komma, `\P`, nieuwe regels en een achtervoegsel zoals `m NAP`. Beschadigde geneste scopes geven een leesfout; een ontbrekende pijl gebruikt geen tekstpositie als vervanging.
- Fixture opnieuw maken: `python tools/generate_marxact_cad_fixture.py` met ezdxf 1.4.4. Volledige CAD-sjablonen en acceptatie met echte MarXact-bestanden blijven open.

## Ruimtelijke MarXact-labelkoppeling

- Meetpunten en grensvertices gebruiken een gedeelde ruimtelijke index in plaats van per punt alle labels te doorzoeken. De index verwijst naar de oorspronkelijke labels; gelijke XY-posities bewaren alleen de laatste kandidaat, inclusief +0/-0.
- Bij gelijke afstanden blijft de Python-volgorde per cel en bronpositie bepalend. Dit herstelt een afwijkende label-/hoogtekeuze van de eerdere native lineaire scan.
- Een extra celrand corrigeert een Python-fout: ties-to-even-afronding kan labels binnen 8 cm twee cellen uiteen plaatsen (bijvoorbeeld 0,04 en 0,12). Rust zoekt ook die cellen en controleert steeds de werkelijke afstand. Dit is een bewuste correctie, geen byte-identieke reproductie van die fout.
- 10.018 vragen in 207 fixtures vergelijken de oorspronkelijke Python-functie met een onafhankelijke volledige afstandszoekactie in dezelfde tievolgorde. De referentie legt 36 afrondingscorrecties vast. Ook 100.000 gelijke labels, negatieve nul en zeer grote coördinaten hebben regressies. Generator: `python tools/generate_leader_lookup_fixture.py`.
- Dit versnelt deze zoekbewerking bij verspreide labels; veel verschillende punten in één cel kunnen nog steeds een dure zoekactie geven. Volledige importtijd, grens-/objecttoewijzing en geheugen op echte projecten moeten nog worden gemeten.

## ASCII- en binaire DXF-invoer

- Native binaire tagdecoder voor oude éénbyte-groepcodes met XDATA-escape en moderne tweebyte-groepcodes. Beide formaten gebruiken dezelfde geometrie- en MarXact-parser; er is geen Python-runtime nodig.
- Vier door ezdxf 1.4.4 geschreven ASCII/binary-paren controleren R12, R2000 en R2018, Windows-1252, Windows-1251, Shift-JIS en UTF-8. Dezelfde punten, lagen, handles en kleuren worden gelezen. Een binaire geneste MarXact-fixture geeft dezelfde objecten en hoogten als ASCII.
- Codepagina's komen uit HEADER; moderne UTF-8 heeft voorrang ongeacht de volgorde van de headerwaarden. Ongeldige tekst en nog niet ondersteunde codepagina's geven een foutmelding. Letterlijke DXF Unicode-escapes blijven in de bronvelden behouden, zoals bij de oorspronkelijke Python-laagnamen.
- ASCII-waarden verwijzen tijdens het lezen naar de bronbuffer. Alleen de huidige entiteit houdt haar groepvelden vast; er wordt geen extra vector met alle tags en kopieën daarvan opgebouwd. De bronbytes en uiteindelijke geometrie staan nog wel in geheugen.
- Invoer is begrensd op 256 MiB, 1 MiB per waarde, 1 miljoen velden per entiteit, 2 miljoen weergegeven entiteiten en 10 miljoen weergegeven vertices. Deze grenzen zijn geen totale procesgeheugenlimiet.
- Afgebroken scalars/strings, niet-eindige getallen, ontbrekende ENDSEC/EOF en onderbroken POLYLINE-sequenties worden geweigerd, zodat een beschadigd prefix geen ogenschijnlijk complete tekening oplevert.
- INSERT-attributen, XDATA/binaire chunks, kleur, lijntype en lijndikte zijn beschikbaar als bronmetadata en blijven in native projecten bewaard. Volledige attribuut-/tekstgeometrie, Meshes, HATCH en CAD-sjablonen blijven open.
- Fixturegenerator: `python tools/generate_dxf_encoding_fixtures.py`. De scalarindeling volgt de [Autodesk-groepcodereferentie](https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-DXF/files/GUID-2553CF98-44F6-4828-82DD-FE3BC7448113.htm) en [binaire DXF-beschrijving](https://help.autodesk.com/cloudhelp/2019/ENU/AutoCAD-DXF/files/GUID-FC1C3C69-DBC2-49E4-893A-000D6538C0FE.htm).

## CAD-blokken en WCS-geometrie

- Geneste BLOCK/INSERT en MINSERT respecteren blokbasispunten, OCS, spiegeling, niet-uniforme schaal, rotatie en rijen/kolommen. Rasterafstanden worden geroteerd maar niet geschaald. Laag 0 en BYBLOCK erven de plaatsing; verborgen bovenliggende lagen verbergen ook hun inhoud.
- LWPOLYLINE en 2D POLYLINE ondersteunen bulges en hoogteligging; 3D POLYLINE blijft WCS. CIRCLE/ARC en ELLIPSE bewaren hun curve en worden na de volledige geneste transformatie afgevlakt met maximaal 0,25 tekeneenheid koordafwijking (bij RD-meters: 25 cm). POINT wordt als punt getoond.
- De cache deelt bronmetadata en blokdefinities tussen instanties en projectsnapshots. Geometrie en zichtbare kaartgrenzen worden opnieuw berekend bij bron-/laagwijziging, niet tijdens gewone frames. MarXact gebruikt de oorspronkelijke modelspace-entiteiten voor zijn meetpunten.
- **Opnieuw laden** leest een DXF opnieuw; **Bron kiezen...** herstelt een verplaatst bronbestand. Zichtbaarheidskeuzes blijven behouden waar laagnamen overeenkomen. Bij een leesfout blijven de huidige overlay en onopgeslagen wijzigingen intact. Dit helpt oude projecten waarvan eerdere previews blokdefinities niet opsloegen; ontbrekende oorspronkelijke OCS-data kan niet uit oude punten worden terugberekend.
- Een ezdxf 1.4.4-orakel vergelijkt 72 plaatsingen en 84 WCS-primitieven in ASCII en binary, inclusief geneste shear, schuine vlakken, negatieve schalen en dichte curvesamples. Projectherstel, gedeelde metadata voor 10.000 instanties, laagcache en beschadigde/recursieve/te grote blokken hebben regressietests. Generator: `python tools/generate_cad_scene_fixtures.py`.
- Recursie, ontbrekende blokken, budgetoverschrijdingen en niet ondersteunde entiteitsoorten krijgen meldingen. TEXT/MTEXT/ATTRIB-weergave, volledige meshoppervlakken, HATCH, dynamische blokken, volledige lijnstijlen en sjabloonexport ontbreken nog. DXF-/projectinvoer loopt via de bestandsworker; dit is geen volledige CAD- of performanceacceptatie.
- Plaatsing volgt de Autodesk-definities voor [INSERT](https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-DXF/files/GUID-28FA4CFB-9D5E-4880-9F11-36C97578252F.htm) en het [arbitrary-axis-algoritme](https://help.autodesk.com/cloudhelp/2020/ENU/AutoCAD-DXF/files/GUID-E19E5B42-0CC7-4EBA-B29F-5E1D595149EE.htm).

## CAD-vlakken en mesh-randen

- 3DFACE, SOLID en TRACE tonen hun zichtbare randen, met WCS voor 3DFACE en OCS plus de afwijkende puntvolgorde voor SOLID/TRACE. Alle vier 3DFACE-randbits blijven aan hun bronrand gekoppeld, ook bij driehoeken met een dubbel vierde punt.
- POLYLINE-polyfaces lezen signed één-gebaseerde vlakindices, ook als vlakrecords vóór coördinaatrecords staan. Declaratieve polyface-aantallen worden als bronmetadata behouden; de werkelijke records bepalen de geometrie. Polygonmeshes respecteren M/N-rasterafmetingen en beide sluitrichtingen. Moderne MESH leest n-gons, expliciete randen en vouwgewichten als aparte lijsten.
- Geïndexeerde bronvertices/vlakken, normaal/dikte, smoothing-/subdivisieparameters en niet-standaard vertex-/vlakmetadata/XDATA blijven behouden. Meshbronnen worden bij snapshots gedeeld. Een gecachte randset dedupliceert gedeelde topologische randen; losse zichtbare randen vormen aparte paden. Projectherstel bouwt de randen opnieuw uit de bron op en verandert het bronbestand niet. Oudere previews zonder dit mesh-schema weigeren de nieuwe velden.
- Zesenvijftig ezdxf bron-/wereldgevallen in ASCII/binary vergelijken 278 zichtbare randen, inclusief geneste shear/spiegeling, alle 3DFACE-maskers en OCS. Ook opslag, verouderde randcache, ongeldige indices/aantallen, budgetten, een 100 × 100 raster en werkelijk geschilderde verborgen randen zijn getest. De fixtures regenereren byte voor byte; zie de [referentiebeschrijving en primaire bronnen](tests/fixtures/meshes/README.md).
- Dit is rand-/controlekooiweergave. Vlakvulling, subdivisie, gladde polygonoppervlakken, dikte-extrusies en het toepassen van afzonderlijke vertex-/vlakstijlen zijn nog open en krijgen meldingen. MESH-subobjectoverrides worden met een expliciete fout geweigerd. Een mesh wordt nooit als MarXact-sleufgrens gebruikt. Bestaande bron-, punt-, pad- en projectbudgetten blijven gelden; geen harde totale RAM-garantie.

## SPLINE/NURBS en CAD-fitcurves

- Controlepunten, knopen, positieve gewichten, fitpunten, eindtangenten, vlaggen, normaal en toleranties blijven als bronparameters bewaard. Open, ongeklemde en periodieke curves, herhaalde knopen en standaardknopen bij ontbrekende waarden worden ondersteund. SPLINE-punten blijven WCS; het normaalveld maakt ze niet OCS.
- Rationele Bézier-spans worden uit de B-spline afgeleid zonder herhaalde kopieën van de hele curve. Afvlakking gebruikt de omhulling van de controlepunten na de volledige bloktransformatie, met 0,25 tekeneenheid tolerantie. De buffer wordt tussen spans hergebruikt. Volledig herhaalde binnenknopen splitsen de weergave in aparte paden; er wordt geen verbindingslijn over een gat getekend.
- Fitpunten volgen een kubische C2-interpolatie in koordlengteparameters, met natuurlijke uiteinden of opgegeven eindtangenten volgens ezdxf/CAD. Het tridiagonale stelsel wordt in lineaire tijd opgelost. Zoals ezdxf wordt een afwijkende graad bij alleen fitpunten niet gebruikt voor de curvevorm.
- Twintig bronvarianten vergelijken 60 wereldcurves en 63 paden met dichte samples van de ezdxf 1.4.4 Python-kernel. Dat omvat graad 1/3/5/11, rationele kwartcirkel, uiteenlopende gewichten, natuurlijke/clamped fitcurves, lussen, spiegeling, schuine vlakken en geneste shear. Ook 20.000 fitpunten, projectherstel, ongeldige gewichten/knopen/aantallen en uitvoerbudgetten hebben regressies. Generator: `python tools/generate_spline_fixtures.py`.
- De referentie gebruikt de geldige knooprange van ezdxf `params()`, zodat ongeklemde/periodieke curves geen extrapolaties buiten hun domein krijgen. De native parser bewaart geschreven knopen zonder tolerantieronding.
- Ondersteuning is begrensd op graad 32, 250.000 controle- of fitpunten per curve, een berekenings-/dieptebudget en het bestaande totale vertexbudget. Negatieve/nulgewichten en numeriek onbruikbare bereiken geven een fout. Dit is geen harde totale geheugenlimiet of volledige CAD-sjabloonacceptatie. De bronindeling volgt [Autodesk SPLINE](https://help.autodesk.com/cloudhelp/2023/ENU/AutoCAD-DXF/files/GUID-E1F884F8-AA90-4864-A215-3182D47A9C74.htm); fit-/controlegegevens zijn vergeleken met de [ezdxf-spline-API](https://ezdxf.readthedocs.io/en/stable/tutorials/spline.html).

## Getallen behouden bij projectopslag

- De JSON-parser gebruikt exacte float-rondgangen: opgeslagen eindige `f64`-metingen behouden hun bits, inclusief negatieve nul en subnormale waarden. Dit herstelt een gevonden afwijking van één floatstap bij het heropenen van een curvepunt. Regressies vergelijken ruim 4.000 waarden en complete CAD-projectrondgangen.
- Het oorspronkelijke Python-JSON bewaart ook grote gehele getallen en lange decimalen zonder omzetting naar `f64`. Onbekende metadata blijven in het bronarchief staan; geometrievelden blijven eindige, gevalideerde meetwaarden. JSON-opmaak en exponentnotatie kunnen bij opnieuw serialiseren verschillen.

## CAD-kleuren, lagen en objectgegevens

- De kaart gebruikt het volledige ACI-palet van de bestaande Python/ezdxf-versie, inclusief de kleurregels voor BYLAYER/BYBLOCK op modelspace-niveau. Expliciete true-color heeft voorrang. Net als Python krijgt wit uit ACI een contrasterende kaartkleur; expliciete RGB-kleuren blijven ongewijzigd.
- Rust corrigeert twee Python-leesfouten: expliciet RGB-zwart wordt behouden en true-color op een laag wordt toegepast. Uitgeschakelde/bevroren lagen en onzichtbare objecten worden verborgen. Zichtbare objecten bepalen de DXF-kaartgrenzen.
- Laagverwijzingen volgen hoofdletterongevoelig Unicode `lower()`, zoals de ezdxf-symbooltabel. De oorspronkelijke schrijfwijze blijft in de bronmetadata behouden; kaartkleur, laagkeuze en geneste blokzichtbaarheid gebruiken één laagdefinitie. Afgeleide aliassen worden eenmaal opgebouwd, gedeeld bij snapshots en bij projectherstel hersteld. ASCII/binary-referenties dekken ook Griekse, Cyrillische en geaccentueerde namen. `Straße` en `STRASSE` blijven afzonderlijke lagen, overeenkomstig ezdxf.
- Oude projecten met conflicterende opgeslagen laagvarianten behouden beide keuzes en tonen een herlaadmelding. Nieuwe DXF-invoer met dubbele laagdefinities wordt geweigerd. Herladen bewaart zichtbaarheid ook als de schrijfwijze van een laagnaam is gewijzigd. Fixturegenerator: `python tools/generate_cad_layer_fixture.py`.
- Onder **DXF-lagen → CAD-lagen** kun je afzonderlijke lagen tonen/verbergen. De keuze blijft in het project bewaard. Onder **Objecten bekijken** staan handles en lagen; de lijst verwerkt alleen zichtbare rijen.
- **CAD-objectgegevens** toont bestand, type, handle, laag, aantal punten, lijntype/lijndikte, INSERT-attributen en XDATA. Dubbele attribuutnamen volgen de oorspronkelijke laatste-waarde-regel. Lange waarden worden herkenbaar verkort getoond en kunnen volledig worden gekopieerd. De details worden alleen bij selectie opgebouwd.
- Een door ezdxf geschreven ASCII/binary-fixture vergelijkt 266 gevallen met de oorspronkelijke Python-functies: alle 255 ACI-kleuren, laagregels, true-color, verborgen objecten, attributen en XDATA. Bewuste correcties hebben een afzonderlijke verwachte uitkomst. Een egui-renderproef controleert werkelijk getekende kleuren en het verbergen van een laag.
- Native opslag schrijft nu formaat **3**, inclusief bronblokken, plaatsingen, curves, CAD-metadata en laagkeuzes. Native formaten 1 en 2 worden bij lezen in geheugen omgezet; het bronbestand blijft bij alleen openen ongewijzigd. Oudere Rust-builds weigeren formaat 3, zodat ze deze nieuwe gegevens niet stilzwijgend verwijderen. Dit betreft het Rust-formaat; Python `.klicproject` v1-import blijft beschikbaar.
- Onbekende velden in native objectstructuren worden geweigerd. Het volledige oorspronkelijke Python-JSON blijft als open bronarchief behouden, inclusief onbekende Python-metadata.
- Fixturegenerator: `python tools/generate_cad_properties_fixture.py` met ezdxf 1.4.4. Attribuutgeometrie, volledige lijnstijlen/-gewichten en CAD-sjabloonexport blijven open. True-color en laagstatus volgen de [Autodesk-groepcodes](https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-DXF/files/GUID-3F0380A5-1C15-464D-BC66-2C5F094BCFB9.htm) en [LAYER-definitie](https://help.autodesk.com/cloudhelp/2018/ENU/AutoCAD-DXF/files/GUID-D94802B0-8BE8-4AC9-8054-17197688AFDB.htm).

## MarXact-richting en uitlijning

- Import en **Automatisch uitlijnen** gebruiken de Python-regels: unieke objectposities, robuuste PCA met mediaan/MAD, stabilisatie op gemeten grensranden binnen 12,5° en snijpunten rond de objecten bij holle grenzen.
- De automatische begin-/eindpunten komen op de middens van de geraakte randen, met geïnterpoleerde Z uit de oorspronkelijke 3D-grens.
- Onder iedere MarXact-proefsleuf staat een **Rotatie °**-regelaar (-90° tot +90°). De correctie is absoluut ten opzichte van de automatische as; herhaald bijdraaien stapelt geen afrondingen op. Handmatige eindpunten liggen op de exacte snijpunten met de grens.
- **Automatisch uitlijnen** berekent de middenlijn opnieuw en wist de correctie. Een handmatige correctie van 0° behoudt de automatische richting, maar gebruikt zoals Python de objectgemiddelde positie en exacte snijpunten; gebruik de knop om ook de grensmiddens terug te zetten.
- De kaart toont de actuele as met een pijl en de gemeten objectpunten. De basis-DXF schrijft de 3D-as op `VIRTUAL_TRENCH_AXIS`; de gemeten contour en objectcoördinaten blijven behouden.
- Projecten en back-ups bewaren de automatische basis en de correctie. Oudere Rust-projecten zonder deze velden blijven leesbaar; de bestaande as wordt bij de eerste handmatige correctie de basis, of kies **Automatisch uitlijnen** om de nieuwe regels toe te passen.
- Regressietests vergelijken tien synthetische gevallen en zeventig handmatige correcties rechtstreeks met bevroren uitvoer van de Python-geometriefuncties. Fixturegenerator: `python tools/generate_marxact_alignment_fixtures.py` vanuit de repository. Dit bewijst geen volledige MarXact- of DXF-sjabloonpariteit op productiedata.

## Lokale kabel-/leidingmarkeringen

- De kaart en basis-DXF gebruiken dezelfde snijpuntberekening om per object de lokale sleufbreedte te bepalen. Bij holle grenzen kiest de gemeten zijwaartse objectpositie het juiste deel van de sleuf.
- Markeringen verbinden geen losse delen van een holle grens. Als er geen geldige doorsnede bestaat, wordt geen onbegrensde markering getekend. In DXF blijven de gemeten object-Z's behouden op `VIRTUAL_TRENCH_MARKER`.
- De kaart herberekent deze geometrie na import of uitlijningswijzigingen; zoomen en verschuiven hergebruiken de uitkomst. Ook de geselecteerde job wordt tijdens kaartverversing als referentie gebruikt in plaats van alle objecten te kopiëren.
- 240 gevallen worden vergeleken met de oorspronkelijke Python-functie. Volledige profieltekeningen, pixelmaskers, kleureninstellingen en sjabloonpariteit blijven open.

## Kernbenchmarks

`cargo run --release --manifest-path rust/Cargo.toml --example benchmark_core` meet
de oude en nieuwe camerainvoer/validatie op dezelfde synthetische job met 100.000
objecten, gedurende twintig herhalingen. CSV bevat tijd, allocatieaanroepen en
buildmodus. De Windows-CI publiceert het CSV als apart artifact en controleert
dat de nieuwe gemeten paden geen tijdelijke objectkopieën/puntvectors alloceren.
Dit is een gerichte microbenchmark, geen bewijs van volledige renderperformance
of een vergelijking met de complete Python-app op echte projecten.

## MarXact- en virtuele-proefsleufgrenzen in DXF

- De basis-DXF-export schrijft nu ook gesloten **3D POLYLINE**-grenzen (R2018) met de oorspronkelijke gemeten MarXact-Z per vertex, of de rechthoekige grens bij handmatige proefsleuven.
- De polylines krijgen de laag `VIRTUAL_TRENCH_3D` en een eenvoudig `TEXT`-label; origineel blijft in het Rust-project beschikbaar.
- Dit is nog **niet** de volledige AutoCAD-sjabloon/DXF-pariteit, maar voorkomt dat de gemeten MarXact-grens volledig uit de basisexport verdwijnt.

## PDOK Kadaster WFS (eerste native versie)

- WFS 2.0.0 `GetFeature` in RD EPSG:28992 met vijf bestaande SleufBase-categorieën: perceel, kadastrale grens, bebouwing, straatnaam en huisnummer.
- Gecombineerde kleine tegelverzoeken bij grotere gebieden; beperkte paginering, feature-ID-deduplicatie, HTTP-timeouts en grenzen voor geheugen-/downloadgrootte.
- Perceel- en bebouwingslijnen en de tekstlabels worden als afzonderlijk zichtbare laag op de kaart getoond en in het Rust-project bewaard.
- De labelhoek wordt bewaard maar **nog niet geroteerd** getoond in de Rust-kaart. Ook export van kadastrale lijnen en tekst naar de volledige DXF-sjablonen ontbreekt nog.

## Kadastrale en BGT-kaartlagen in basis-DXF

- De **DXF objecten**-export kan nu tevens de zichtbare Kadaster-lijnen, tekstlabels (inclusief rotatiehoek) en BGT-lijnen opnemen op afzonderlijke CAD-lagen.
- Export gebruikt de native R2018-objectencoder. Volledige SleufBase-sjablonen en dynamische blokken blijven een afzonderlijke workflow.
- Alleen de zichtbare WFS/BGT-lagen worden meegenomen; grote datasetlimieten en daadwerkelijke CAD-validatie blijven nodig.

## KickTheMap-downloads: bronidentiteit behouden

- Bij een gesigneerde GeoTIFF- of objecten-JSON-download schrijft Rust nu een eigen `*.tiff.job.json` / `*.json.job.json` sidecar met job-ID en titel.
- De downloader maakt beide bestanden eerst tijdelijk aan en rolt een onvolledig nieuw bestand terug als de sidecar niet kan worden opgeslagen.
- Lokale objecten-JSON haalt de job-ID eerst uit de sidecar of bestandsnaam (`Naam_123_jobFeatures.json`) en gebruikt de oude mapnaam alleen als fallback.
- Daardoor werkt de 'alleen geladen jobs'-filter ook voor lokaal in één gedeelde Rust-map gedownloade jobs.

## Native MarXact batch-CLI

- `sleufbase-rust.exe --marxact nieuw-project.json meting1.dxf meting2.dxf` maakt een Rust-project zonder GUI.
- `sleufbase-rust.exe --marxact-dxf uitvoer.dxf meting1.dxf meting2.dxf` exporteert de gemeten 3D-grenzen en objecten in de native R2018-DXF.
- De conversie leest de bestaande niet-gevoelige MarXact-mappings uit de Rust-instellingen en weigert bestaande uitvoerbestanden te overschrijven.
- Omdat de DXF-invoer nog geen volledige MarXact-vendordekking heeft, controleer conversies met echte MarXact-bestanden voordat je er ontwerpbeslissingen op baseert.

## Wat nog ontbreekt

1. Interactieve KickTheMap-browserlogin, accountbeheer, volledige downloadfallbacks en sessieherstel; de eerste API-koppeling is aanwezig.
2. Volledige GeoTIFF-rasterweergave met overviews en resolutie op zoomniveau, PDOK WMTS/offline-cache, volledige BGT/WFS/AHN-workflows, StreetSmart en kaartreprojectie.
3. Volledige kadastrale DXF-sjabloonexport inclusief profielen, raster, foto's en blokken.
4. Volledig uitgewerkte virtuele proefsleufprofielen (de geometriebasis is aanwezig), MarXact, volledige compatibiliteit met Python-projectopslag (v1-import is aanwezig), migratie van bestaande instellingen en volledig back-upherstel.
5. Overige oorspronkelijke instellingen, installatiepakket en volledige migratietests.

## Aanpak voor vervolgmigratie

1. Rust data-/exportkern en Windows CI: deze pull request.
2. GeoTIFF, RD-projectie, kaartweergave en sidecar-identificatie.
3. Read-only KickTheMap jobs-lijst en betrouwbare actieve-job filter.
4. Volledige template-/kadastrale DXF-pariteit met regressietests.
5. Projectformaat, instellingen, credential handling, installer en gebruikerstests.

De bestaande Python-app, build en releases worden in deze PR niet gewijzigd.
