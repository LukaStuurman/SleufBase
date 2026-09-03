#define MyAppName "SleufBase"
#define MyAppVersion "0.3.34"
#define MyAppPublisher "Techbase"
#define MyAppExeName "SleufBase.exe"

[Setup]
AppId={{8A96F2F3-9F2B-4C5C-8E62-7D9071C4D2AF}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={localappdata}\Programs\Techbase\SleufBase
DefaultGroupName=Techbase\SleufBase
DisableProgramGroupPage=yes
DisableReadyPage=no
AlwaysShowDirOnReadyPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\dist-installer
OutputBaseFilename=SleufBase-Setup
SetupIconFile=..\assets\sleufbase_icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=yes
UsePreviousAppDir=yes
VersionInfoVersion=0.3.34.0
VersionInfoCompany={#MyAppPublisher}
VersionInfoDescription=SleufBase Windows installer
VersionInfoProductName={#MyAppName}
VersionInfoProductVersion={#MyAppVersion}

[Languages]
Name: "dutch"; MessagesFile: "compiler:Languages\Dutch.isl"

[Files]
Source: "..\dist\SleufBase\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\SleufBase"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"
Name: "{autodesktop}\SleufBase"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; Check: ShouldCreateDesktopShortcut

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "SleufBase starten"; Flags: nowait postinstall skipifsilent

[Code]
var
  DesktopShortcutCheckBox: TNewCheckBox;

procedure InitializeWizard;
begin
  { Geen aparte Tasks/custom wizardpagina meer. In v0.3.32 en v0.3.33 kon
    juist die extra stap interactief vastlopen. De keuze staat daarom op de
    bestaande Ready to Install-pagina, zodat de normale Inno Setup-navigatie
    volledig intact blijft. }
  WizardForm.ReadyMemo.Height := WizardForm.ReadyMemo.Height - ScaleY(24);

  DesktopShortcutCheckBox := TNewCheckBox.Create(WizardForm);
  DesktopShortcutCheckBox.Parent := WizardForm.ReadyMemo.Parent;
  DesktopShortcutCheckBox.Left := WizardForm.ReadyMemo.Left;
  DesktopShortcutCheckBox.Top :=
    WizardForm.ReadyMemo.Top + WizardForm.ReadyMemo.Height + ScaleY(8);
  DesktopShortcutCheckBox.Width := WizardForm.ReadyMemo.Width;
  DesktopShortcutCheckBox.Height := ScaleY(17);
  DesktopShortcutCheckBox.Caption := 'Bureaubladsnelkoppeling maken';
  DesktopShortcutCheckBox.Checked := False;
end;

function ShouldCreateDesktopShortcut(): Boolean;
begin
  if DesktopShortcutCheckBox = nil then
  begin
    Result := False;
    Exit;
  end;
  Result := DesktopShortcutCheckBox.Checked;
end;
