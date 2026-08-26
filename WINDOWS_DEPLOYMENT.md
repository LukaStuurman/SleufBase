# SleufBase Windows deployment

## Aanbevolen distributie

Gebruik `SleufBase-Setup.exe` voor dagelijkse/professionele inzet. De installer plaatst de PyInstaller onedir-build in `%LOCALAPPDATA%\Programs\Techbase\SleufBase`. Daardoor hoeft de applicatie bij iedere start niet eerst een volledige one-file runtime uit te pakken.

`SleufBase-Portable.exe` blijft beschikbaar voor incidenteel/portable gebruik. Deze start normaal gesproken langzamer omdat de ingebedde PyInstaller-runtime eerst naar een tijdelijke map wordt uitgepakt.

## Stille installatie

Voor softwaredistributie via bijvoorbeeld Intune, scripts of een softwareportal:

```powershell
SleufBase-Setup.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
```

Een alternatief doelpad kan worden opgegeven met `/DIR="C:\Pad\Naar\SleufBase"`.

## Logs en diagnoses

SleufBase schrijft operationele logs naar:

```text
%LOCALAPPDATA%\Techbase\SleufBase\logs\sleufbase.log
```

Logs roteren automatisch (maximaal 2 MB per bestand, vijf backups) zodat de map niet onbeperkt groeit.

Onverwachte fouten krijgen daarnaast een apart crashrapport in:

```text
%LOCALAPPDATA%\Techbase\SleufBase\diagnostics\crash-YYYYMMDD-HHMMSS.txt
```

Voor een systeemdiagnose zonder de volledige GUI te starten:

```powershell
SleufBase.exe --diagnostics
```

Dit schrijft `system-info.json` in de diagnostics-map.

## Windows-integratie

De professionele runtime stelt `Techbase.SleufBase` in als Windows AppUserModelID, activeert per-monitor DPI-awareness waar Windows dat ondersteunt en bevat bedrijfs-/product-/versiemetadata in de executable.

## CI-validatie

De Windows-build valideert zowel de snelle onedir-versie als de portable one-file-versie met `--smoke-test`. De test vereist onder andere dat `ktk_accel.dll` daadwerkelijk geladen wordt. De installer wordt in CI ook stil geïnstalleerd in een tijdelijke map en de geïnstalleerde executable krijgt opnieuw dezelfde smoke-test.

## Code signing

Voor volledige enterprise trust hoort de installer en executable met een organisatiecode-signingcertificaat te worden ondertekend. SleufBase bevat nog geen certificaat of geheime signing key in de repository. Voeg dergelijke sleutels uitsluitend als beschermde CI-secrets toe, nooit als repositorybestand.
