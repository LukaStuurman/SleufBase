# SleufBase: volledige functiepariteit als release-eis

**Doel:** de nieuwe Rust-versie is pas een vervanger van de huidige SleufBase wanneer alle relevante gebruikersfuncties en bestaande projectformaten correct werken.

**Huidige status:** Rust v0.1.x is een migratie-preview, géén volledige port. Een groene Windows-build bewijst alleen dat de huidige Rust-subset compileert en haar eigen tests passeert.

| Onderdeel | Bestaande Python-bron | Rust status | Acceptatie-eis voor volwaardige vervanging |
|---|---|---|---|
| Desktop UI, menu's, dialogs, shortcuts, selectie, zoom/pan, drag/drop | app.py, rounded_widgets.py, renderer.py | Basis kaartinteractie en opmerkingen, rest deels | Alle bedieningen, kaartinteracties en workflows hebben dezelfde gevolgen |
| Project openen, opslaan, metadata, herstel | app.py, models.py, autosave_* | Eigen Rust-projectopslag; Python-compatibiliteit ontbreekt | Oud project laden/bewerken/opslaan zonder verlies |
| Backups/autosave + crash recovery | autosave_backup_patch.py, autosave_restore_ui.py | Automatische roterende Rust-backup; restore-dialoog ontbreekt | Onderbreking en restore-tests |
| GeoTIFF decoderen en weergeven | geotiff.py, renderer.py | Metadata en begrensde 8-bit kaartpreview deels | Alle kleurmodi, enorme TIFFs, pyramides, alpha en renderperformance |
| GeoTIFF RD-referentie, rotatie en transformaties | models.py, geotiff.py | Affine kern gereed, niet UI-getest | RD coördinaten en overlay-match op fixtures |
| PDOK luchtfoto en kwaliteitsopties | pdok.py, pdok_default_background_patch.py | Asynchrone WMS-kaartlaag, overige bron-/kwaliteitsopties ontbreken | Source-switch/cache/retry en standaardgedrag |
| BGT kaart en alle API-bronnen | bgt_roadpart.py, bgt_terrain_boundary.py, bgt_vector_tiles.py, cadastral_wfs.py | Ontbreekt | Geometrie, caching, stijlen, WFS/foutgevallen |
| Kaarttegels, OpenStreetMap, lokale cache | web_tiles.py, osm.py | Ontbreekt | Identieke rendering, limieten en offline-fallback |
| AHN, maaiveld en AI-assistent | ahn.py, ai_maaiveld.py, vllm_runtime.py | Ontbreekt | Geldige meetwaarden, foutafhandeling en AI-fallback |
| DXF import, bestaande blokken, metadata | dxf.py | ASCII basis deels | Kleur, laag, attributen, geometrie, XDATA |
| DXF objectexport (basis) | kickthemap_dxf_export.py | Deels | Volledige layer rules/XDATA/labels/polyline/tekstpariteit |
| Volledige kadastrale/template DXF export | cadastral_export.py, dxf_template_pipeline_* | Ontbreekt | Identieke output op regressie-fixtures, inclusief fonts en block/visibility |
| Dwarsprofielen, omgekeerde profielen en labels | template_reverse_patch.py, template_dynamic_profile_leader_patch.py | Ontbreekt | Gelijkwaardige geometrie, labelplaatsing en snapshots |
| Foto's/raster asset-bundels, veilige exports | template_raster_io.py, template_export_transaction.py | Ontbreekt | Geen verloren vorige DXF of assets bij exportfouten |
| KickTheMap online login/account en veilig sessiebeheer | kickthemap.py, kickthemap_browser.py | HTTPS form-login/CSRF/cookies in geheugen; reCAPTCHA/WebView2 ontbreekt | Login/logout, token/cookies, verval/herstel zonder lekken |
| KickTheMap live jobs-lijst, retry, caching | kickthemap.py, kickthemap_jobs_browser.py | Live API-kern, in-memory sessie en offline cache; browserfallback en betrouwbare cache ontbreekt | API- en browserfallback, sort/search/selected |
| KickTheMap GeoTIFF/jobs downloads + sidecars | kickthemap.py, geotiff.py | Geautoriseerde signed-URL GeoTIFF/objecten download met veilige opslag; fallback/coords/sidecars niet klaar | Checksums, retries, TIFF-validatie, metadata en jobs |
| KickTheMap objecten/parsing | kickthemap_dxf_export.py | Deels | Punt/polyline, hoogte, attributen, groepsfilters op echte exports |
| Filter alleen actief geladen jobs | app.py, kickthemap_jobs_browser.py | Python-heartbeat read-only | Zelfstandige Rust-app schrijft/evalueert state betrouwbaar |
| Discipline-overzicht + Excel-formaat | discipline_excel_export.py | Deels | Identieke regels, ordering, categorisering, totalen, sjablonen en styling |
| Virtuele proefsleuven/profielen | virtual_trench.py, virtual_trench_template_patch.py | Basisgeometrie en aanmaak, profielen nog niet | Plaatsen, maatvoering, kettingmaat, export |
| MarXact import, snapping, metingen | marxact_*.py | Ontbreekt | Identieke CSV/point mapping, modeltransformaties, grenzen |
| Cyclomedia/StreetSmart | cyclomedia.py, streetsmart*.py | Ontbreekt | Browser, selectie, overdracht/extern auth |
| Instellingen en oude voorkeuren | settings.py, settings_ui.py, settings_* | Niet-gevoelige instellingenimport en autosave-config, volledige opties ontbreken | Migratie van bestaande settings zonder wachtwoordverlies |
| Performance voor grote jobs / lage hardware | resource_policy.py, kickthemap_scalability_patch.py, renderer.py | Niet gemeten | Zelfde of betere stabiliteit op benchmark datasets |
| Portable EXE, Windows installer, updates, signing | installer/, .github/workflows/build-windows-exe.yml | Alleen CI preview EXE | Installer, portable, upgrade/rollback en release integriteit |

## Verplichte go/no-go voorwaarden

1. Elk onderdeel hierboven is daadwerkelijk geïmplementeerd; 'deels' telt niet als gereed.
2. Functionele acceptatietests worden uitgevoerd met representatieve echte SleufBase-projectbestanden en toestemming voor externe diensten.
3. Rust regressietests dekken de contracts uit de 72 huidige Python testmodules en belangrijke bytecode-only workflows.
4. Output van DXF/Excel/GeoTIFF is inhoudelijk en geometrisch vergeleken met Python, niet slechts 'bestand bestaat'.
5. Versie- en projectmigratie is terugwaarts compatibel of er is een expliciete veilige converter.
6. Geen productie-release of merge naar main enkel omdat Rust-tests groen zijn.

De bestaande Windows-versie v0.3.62 blijft leidend totdat al deze voorwaarden aantoonbaar gehaald zijn.

**Let op:** De Rust-projectbestanden zijn niet uitwisselbaar met de bestaande Python-projectbestanden. Geen conversie of overschrijving zonder expliciete verificatie.
