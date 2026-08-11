$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $Root

if (-not (Get-Command py -ErrorAction SilentlyContinue)) {
    throw 'Python launcher (py.exe) niet gevonden. Installeer Python 3.11 voor Windows.'
}

$PythonVersion = '3.11'
$Venv = Join-Path $Root '.venv-build'

if (-not (Test-Path $Venv)) {
    py -$PythonVersion -m venv $Venv
}

$Python = Join-Path $Venv 'Scripts\python.exe'
& $Python -m pip install --upgrade pip wheel
& $Python -m pip install -r (Join-Path $Root 'requirements-windows.txt')

Remove-Item -Recurse -Force (Join-Path $Root 'build') -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force (Join-Path $Root 'dist') -ErrorAction SilentlyContinue

& $Python -m PyInstaller --clean --noconfirm (Join-Path $Root 'SleufBase.spec')

$Exe = Join-Path $Root 'dist\SleufBase.exe'
if (-not (Test-Path $Exe)) {
    throw 'Build voltooid zonder dist\SleufBase.exe.'
}

Write-Host ''
Write-Host 'Build gereed:' -ForegroundColor Green
Write-Host $Exe
