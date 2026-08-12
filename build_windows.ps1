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

& $Python (Join-Path $Root 'prepare_windows_assets.py')

Remove-Item -Recurse -Force (Join-Path $Root 'build') -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force (Join-Path $Root 'dist') -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force (Join-Path $Root 'dist-installer') -ErrorAction SilentlyContinue

Write-Host 'Bouwen: snelle professionele onedir-versie...'
& $Python -m PyInstaller --clean --noconfirm (Join-Path $Root 'SleufBase.spec')

$FastExe = Join-Path $Root 'dist\SleufBase\SleufBase.exe'
if (-not (Test-Path $FastExe)) {
    throw 'Build voltooid zonder dist\SleufBase\SleufBase.exe.'
}

Write-Host 'Bouwen: portable one-file versie...'
& $Python -m PyInstaller --clean --noconfirm (Join-Path $Root 'SleufBasePortable.spec')

$PortableExe = Join-Path $Root 'dist\SleufBase-Portable.exe'
if (-not (Test-Path $PortableExe)) {
    throw 'Build voltooid zonder dist\SleufBase-Portable.exe.'
}

$Zip = Join-Path $Root 'dist\SleufBase-Windows.zip'
Compress-Archive -Path (Join-Path $Root 'dist\SleufBase\*') -DestinationPath $Zip -Force

Write-Host 'Runtime smoke-test: snelle build...'
$FastProcess = Start-Process -FilePath $FastExe -ArgumentList '--smoke-test' -Wait -PassThru
if ($FastProcess.ExitCode -ne 0) {
    throw "Snelle SleufBase smoke-test faalde met exitcode $($FastProcess.ExitCode)."
}

Write-Host 'Runtime smoke-test: portable build...'
$PortableProcess = Start-Process -FilePath $PortableExe -ArgumentList '--smoke-test' -Wait -PassThru
if ($PortableProcess.ExitCode -ne 0) {
    throw "Portable SleufBase smoke-test faalde met exitcode $($PortableProcess.ExitCode)."
}

$IsccCandidates = @(
    (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'),
    (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe')
) | Where-Object { $_ -and (Test-Path $_) }

if ($IsccCandidates.Count -gt 0) {
    Write-Host 'Bouwen: SleufBase installer...'
    & $IsccCandidates[0] (Join-Path $Root 'installer\SleufBase.iss')
} else {
    Write-Warning 'Inno Setup 6 niet gevonden; installer overgeslagen. De onedir- en portable-builds zijn wel gereed.'
}

Write-Host ''
Write-Host 'Build gereed:' -ForegroundColor Green
Write-Host "Fast:     $FastExe"
Write-Host "Portable: $PortableExe"
Write-Host "ZIP:      $Zip"
