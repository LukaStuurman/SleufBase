# Securitybeleid

## Ondersteunde versie

Alleen de meest recente publieke SleufBase Windows-release wordt actief onderhouden voor beveiligings- en betrouwbaarheidsfixes.

## Een kwetsbaarheid melden

Publiceer beveiligingsgevoelige details niet in een openbaar GitHub-issue. Meld een vermoedelijke kwetsbaarheid rechtstreeks en voeg waar mogelijk toe:

- getroffen SleufBase-versie;
- concrete stappen om het probleem te reproduceren;
- verwachte en werkelijke uitkomst;
- relevante log- of diagnosebestanden zonder wachtwoorden, tokens of andere geheimen.

Geheimen, authenticatietokens en persoonsgegevens horen nooit in openbare issues, crashrapporten of screenshots.

## Release-integriteit

Windows-releases bevatten `SHA256SUMS.txt` en een CycloneDX-SBOM. Controleer bij distributie van een gedownloade installer of portable build de SHA-256-hash tegen het bestand uit dezelfde GitHub Release.
