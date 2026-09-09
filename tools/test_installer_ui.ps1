param(
  [Parameter(Mandatory = $true)][string]$SetupPath,
  [Parameter(Mandatory = $true)][string]$InstallDir
)

$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class NativeMouse {
    [DllImport("user32.dll")]
    public static extern bool SetCursorPos(int X, int Y);

    [DllImport("user32.dll")]
    public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);

    public const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
    public const uint MOUSEEVENTF_LEFTUP = 0x0004;
}
'@

function Normalize-ControlName {
  param([AllowNull()][string]$Name)
  if ($null -eq $Name) { return '' }
  # Classic Win32/Inno controls may expose accelerator ampersands through UI Automation.
  return (($Name -replace '&', '') -replace '\s+', ' ').Trim()
}

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
    $name = Normalize-ControlName $element.Current.Name
    if ($name -match $NameRegex -and $element.Current.IsEnabled -and -not $element.Current.IsOffscreen) {
      return $element
    }
  }
  return $null
}

function Dump-ControlTree {
  param([Parameter(Mandatory = $true)][System.Windows.Automation.AutomationElement]$Root)

  Write-Host '--- Installer UI Automation controls ---'
  $elements = $Root.FindAll(
    [System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.Condition]::TrueCondition
  )
  foreach ($element in $elements) {
    try {
      $name = Normalize-ControlName $element.Current.Name
      $type = $element.Current.ControlType.ProgrammaticName
      $id = $element.Current.AutomationId
      $enabled = $element.Current.IsEnabled
      $offscreen = $element.Current.IsOffscreen
      $r = $element.Current.BoundingRectangle
      Write-Host ("type={0}; name='{1}'; id='{2}'; enabled={3}; offscreen={4}; rect={5},{6},{7},{8}" -f $type,$name,$id,$enabled,$offscreen,[int]$r.X,[int]$r.Y,[int]$r.Width,[int]$r.Height)
    } catch { }
  }
  Write-Host '--- einde Installer UI Automation controls ---'
}

function Click-Control {
  param([Parameter(Mandatory = $true)][System.Windows.Automation.AutomationElement]$Element)

  $rect = $Element.Current.BoundingRectangle
  if ($rect.Width -le 2 -or $rect.Height -le 2) {
    throw "Control '$($Element.Current.Name)' heeft geen bruikbare klikrechthoek."
  }

  $x = [int][Math]::Round($rect.X + ($rect.Width / 2.0))
  $y = [int][Math]::Round($rect.Y + ($rect.Height / 2.0))
  Write-Host "Installer UI: echte muisklik '$((Normalize-ControlName $Element.Current.Name))' op ($x,$y)"
  if (-not [NativeMouse]::SetCursorPos($x, $y)) {
    throw "Muiscursor kon niet naar control '$($Element.Current.Name)' worden verplaatst."
  }
  Start-Sleep -Milliseconds 100
  [NativeMouse]::mouse_event([NativeMouse]::MOUSEEVENTF_LEFTDOWN, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 80
  [NativeMouse]::mouse_event([NativeMouse]::MOUSEEVENTF_LEFTUP, 0, 0, 0, [UIntPtr]::Zero)
}

function Set-Checkbox {
  param(
    [Parameter(Mandatory = $true)][System.Windows.Automation.AutomationElement]$CheckBox,
    [Parameter(Mandatory = $true)][bool]$Checked
  )

  $pattern = $null
  try {
    $pattern = $CheckBox.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern)
  } catch {
    throw "Checkbox '$($CheckBox.Current.Name)' ondersteunt geen TogglePattern."
  }

  $isOn = $pattern.Current.ToggleState -eq [System.Windows.Automation.ToggleState]::On
  if ($isOn -eq $Checked) { return }

  Click-Control -Element $CheckBox
  Start-Sleep -Milliseconds 250

  $isOnAfterClick = $pattern.Current.ToggleState -eq [System.Windows.Automation.ToggleState]::On
  if ($isOnAfterClick -ne $Checked) {
    throw "Echte muisklik op checkbox '$($CheckBox.Current.Name)' wijzigde de status niet."
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
$lastAction = ''
$stalledIterations = 0

try {
  for ($step = 0; $step -lt 160; $step++) {
    if ($process.HasExited) { break }

    $process.Refresh()
    if ($process.MainWindowHandle -eq 0) {
      Start-Sleep -Milliseconds 250
      continue
    }
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)

    $desktopTask = Find-Control -Root $root -ControlType ([System.Windows.Automation.ControlType]::CheckBox) -NameRegex '^Bureaubladsnelkoppeling maken(?:\s.*)?$'
    if ($null -ne $desktopTask) {
      Set-Checkbox -CheckBox $desktopTask -Checked $true
      $desktopTaskSeen = $true
    }

    $launchTask = Find-Control -Root $root -ControlType ([System.Windows.Automation.ControlType]::CheckBox) -NameRegex '^SleufBase starten(?:\s.*)?$'
    if ($null -ne $launchTask) {
      Set-Checkbox -CheckBox $launchTask -Checked $false
    }

    $finish = Find-Control -Root $root -ControlType ([System.Windows.Automation.ControlType]::Button) -NameRegex '^(Voltooien|Finish)$'
    if ($null -ne $finish) {
      Click-Control -Element $finish
      $clickedFinish = $true
      $lastAction = 'Voltooien'
      $stalledIterations = 0
      Start-Sleep -Milliseconds 600
      continue
    }

    $install = Find-Control -Root $root -ControlType ([System.Windows.Automation.ControlType]::Button) -NameRegex '^(Installeren|Install)$'
    if ($null -ne $install) {
      Click-Control -Element $install
      $clickedInstall = $true
      $lastAction = 'Installeren'
      $stalledIterations = 0
      Start-Sleep -Milliseconds 600
      continue
    }

    $next = Find-Control -Root $root -ControlType ([System.Windows.Automation.ControlType]::Button) -NameRegex '^(Volgende(?:\s*>?)?|Next(?:\s*>?)?)$'
    if ($null -ne $next) {
      Click-Control -Element $next
      $lastAction = 'Volgende'
      $stalledIterations = 0
      Start-Sleep -Milliseconds 600
      continue
    }

    $stalledIterations++
    if ($stalledIterations -eq 12) {
      Write-Warning "Geen bruikbare installercontrol gevonden na laatste actie '$lastAction'."
      Dump-ControlTree -Root $root
    }
    Start-Sleep -Milliseconds 500
  }

  if (-not $process.HasExited) {
    Dump-ControlTree -Root $root
    throw "Interactieve installertest liep vast na actie '$lastAction': installerproces bleef actief."
  }
  $process.WaitForExit()
  if ($process.ExitCode -ne 0) {
    throw "Interactieve installer eindigde met exitcode $($process.ExitCode)."
  }
  if (-not $desktopTaskSeen) {
    throw 'De standaard Inno Setup-checkbox voor de bureaubladsnelkoppeling is niet aangetroffen.'
  }
  if (-not $clickedInstall) {
    throw 'De knop Installeren is niet met een echte muisklik bediend.'
  }
  if (-not $clickedFinish) {
    throw 'De knop Voltooien is niet met een echte muisklik bediend.'
  }

  $installedExe = Join-Path $InstallDir 'SleufBase.exe'
  if (-not (Test-Path $installedExe)) {
    throw "SleufBase.exe ontbreekt na interactieve installatie: $installedExe"
  }
  if (-not (Test-Path $desktopShortcut)) {
    throw "Bureaubladsnelkoppeling is niet aangemaakt nadat de checkbox met een echte muisklik werd aangezet: $desktopShortcut"
  }

  Write-Host 'Interactieve installer UI-test geslaagd: echte muisklikken op Volgende/Installeren/Voltooien en desktop-checkbox werken.'
} finally {
  if (-not $process.HasExited) {
    try { $process.Kill() } catch { }
  }
}
