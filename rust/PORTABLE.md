# SleufBase Rust — portable Windows preview

Deze map bevat de **native Rust-migratiepreview**, niet de huidige stabiele SleufBase-release.

Laat de bijgeleverde `Pillow-LICENSE.txt` en `ezdxf-LICENSE.txt` bij de executable staan. De licenties zijn ook in de executable opvraagbaar met `--licenses`.

## Gebruik

1. Pak de **hele** ZIP uit in een schrijfbare map op Windows 11 (bijvoorbeeld je documenten of een USB-schijf).
2. Start `sleufbase-rust.exe`. Een MSI-installatie of Python-installatie is niet nodig om de EXE te starten. Windows- en driverafhankelijkheden (zoals grafische drivers en de MSVC-runtime) blijven van toepassing.
3. Open of maak een afzonderlijk **SleufBase Rust**-project. Test bij voorkeur op een kopie van je bestaande bestanden.
4. Sluit het programma vóór je een nieuw ZIP-pakket op dezelfde plek uitpakt.

De applicatie is **portable als executable** (geen installatieprogramma); lokale instellingen, aanmeldings-/cachegegevens en automatische back-ups staan waar nodig nog in de Windows-gebruikersappdatamap. Dit is **geen volledig stand-alone configuratie** waarbij al je instellingen naast de EXE staan.

## Controle

De CI levert naast de ZIP een SHA-256-controlebestand `SleufBase-Rust-portable-preview.sha256`. Controleer in PowerShell:

```powershell
(Get-FileHash .\SleufBase-Rust-portable-preview.zip -Algorithm SHA256).Hash
```

Vergelijk de uitkomst met de SHA-256 in het bestand. Een controlehash bewijst de downloadintegriteit ten opzichte van dat bestand, maar vervangt geen codeondertekening.

## Migratiewaarschuwing

De Rust-migratie is niet 100% functieparitair met SleufBase Python v0.3.62. Bevestig GeoTIFF-matches, AutoCAD-tekeningen, Excel-overzichten en template-uitvoer met echte proefprojecten voordat je de preview in productie gebruikt. Oude Python-projecten worden niet stilzwijgend overschreven. Problemen kun je melden via de GitHub-migratie-PR.
