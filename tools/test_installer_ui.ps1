param(
  [Parameter(Mandatory = $true)][string]$SetupPath,
  [Parameter(Mandatory = $true)][string]$InstallDir
)

$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

function Get-RootElement {
  param([Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process)

  $deadline = (Get-Date).AddSeconds(30)
  while ((Get-Date) -lt $deadline) {
    if ($Process.HasExited) {
      throw "Installer stopte voordat het wizardvenster zichtbaar werd (exitcode $($Process.ExitCode))."
    }
    $Process.Refresh()
    if ($Process.MainWindowHandle -ne 0) {
      return [System.Windows.Automation.AutomationElement]::FromHandle($Process.MainWindowHandle)
    }
    Start-Sleep -Milliseconds 250
  }
  throw 'Installerwizard werd niet binnen 30 seconden zichtbaar.'
}

function Find-Control {
  param(
    [Parameter(Mandatory = $true)][System.Windows.Automation.AutomationElement]$Root,
    [Parameter(Mandatory = $true)][System.Windows.Automation.ControlType]$ControlType,
    [Parameter(Mandatory = $true)][string]$NameRegex
  )

  $condition = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    $ControlType
  )
  $elements = $Root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition)
  foreach ($element in $elements) {
    $name = $element.Current.Name
    if ($name -match $NameRegex -and $element.Current.IsEnabled) {
      return $element
    }
  }
  return $null
}

function Invoke-Button {
  param([Parameter(Mandatory = $true)][System.Windows.Automation.AutomationElement]$Button)
  $pattern = $Button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
  Write-Host "Installer UI: klik '$($Button.Current.Name)'"
  $pattern.Invoke()
}

function Set-Checkbox {
  param(
    [Parameter(Mandatory = $true)][System.Windows.Automation.AutomationElement]$CheckBox,
    [Parameter(Mandatory = $true)][bool]$Checked
  )
  $pattern = $CheckBox.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern)
  $isOn = $pattern.Current.ToggleState -eq [System.Windows.Automation.ToggleState]::On
  if ($isOn -ne $Checked) {
    Write-Host "Installer UI: toggle '$($CheckBox.Current.Name)' -> $Checked"
    $pattern.Toggle()
  }
}

$resolvedSetup = (Resolve-Path $SetupPath).Path
if (Test-Path $InstallDir) {
  Remove-Item $InstallDir -Recurse -Force
}

$desktopDir = [Environment]::GetFolderPath('Desktop')
$desktopShortcut = Join-Path $desktopDir 'SleufBase.lnk'
if (Test-Path $desktopShortcut) {
  Remove-Item $desktopShortcut -Force
}

$process = Start-Process -FilePath $resolvedSetup -ArgumentList @("/DIR=$InstallDir") -PassThru
$root = Get-RootElement -Process $process
$desktopTaskSeen = $false
$clickedInstall = $false
$clickedFinish = $false

for ($step = 0; $step -lt 160; $step++) {
  if ($process.HasExited) { break }

  $process.Refresh()
  if ($process.MainWindowHandle -eq 0) {
    Start-Sleep -Milliseconds 250
    continue
  }
  $root = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)

  $desktopTask = Find-Control -Root $root -ControlType ([System.Windows.Automation.ControlType]::CheckBox) -NameRegex '^Bureaubladsnelkoppeling maken$'
  if ($null -ne $desktopTask) {
    Set-Checkbox -CheckBox $desktopTask -Checked $true
    $desktopTaskSeen = $true
  }

  $launchTask = Find-Control -Root $root -ControlType ([System.Windows.Automation.ControlType]::CheckBox) -NameRegex '^SleufBase starten$'
  if ($null -ne $launchTask) {
    Set-Checkbox -CheckBox $launchTask -Checked $false
  }

  $finish = Find-Control -Root $root -ControlType ([System.Windows.Automation.ControlType]::Button) -NameRegex '^(Voltooien|Finish)$'
  if ($null -ne $finish) {
    Invoke-Button -Button $finish
    $clickedFinish = $true
    Start-Sleep -Milliseconds 500
    continue
  }

  $install = Find-Control -Root $root -ControlType ([System.Windows.Automation.ControlType]::Button) -NameRegex '^(Installeren|Install)$'
  if ($null -ne $install) {
    Invoke-Button -Button $install
    $clickedInstall = $true
    Start-Sleep -Milliseconds 500
    continue
  }

  $next = Find-Control -Root $root -ControlType ([System.Windows.Automation.ControlType]::Button) -NameRegex '^(Volgende\s*>?|Next\s*>?)$'
  if ($null -ne $next) {
    Invoke-Button -Button $next
    Start-Sleep -Milliseconds 500
    continue
  }

  Start-Sleep -Milliseconds 500
}

if (-not $process.HasExited) {
  try { $process.Kill() } catch { }
  throw 'Interactieve installertest liep vast: installerproces bleef actief.'
}
$process.WaitForExit()
if ($process.ExitCode -ne 0) {
  throw "Interactieve installer eindigde met exitcode $($process.ExitCode)."
}
if (-not $desktopTaskSeen) {
  throw 'De standaard Inno Setup-checkbox voor de bureaubladsnelkoppeling is niet aangetroffen.'
}
if (-not $clickedInstall) {
  throw 'De knop Installeren is niet interactief aangeklikt.'
}
if (-not $clickedFinish) {
  throw 'De knop Voltooien is niet interactief aangeklikt.'
}

$installedExe = Join-Path $InstallDir 'SleufBase.exe'
if (-not (Test-Path $installedExe)) {
  throw "SleufBase.exe ontbreekt na interactieve installatie: $installedExe"
}
if (-not (Test-Path $desktopShortcut)) {
  throw "Bureaubladsnelkoppeling is niet aangemaakt nadat de checkbox interactief werd aangezet: $desktopShortcut"
}

Write-Host 'Interactieve installer UI-test geslaagd: Volgende/Installeren/Voltooien en desktop-checkbox werken.'
