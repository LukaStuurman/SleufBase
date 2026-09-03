#define MyAppName "SleufBase"
#define MyAppVersion "0.3.33"
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
VersionInfoVersion=0.3.33.0
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
  ExtraTasksPage: TInputOptionWizardPage;

procedure InitializeWizard;
begin
  { Gebruik bewust een eigen pagina in plaats van de standaard [Tasks]-pagina.
    In v0.3.32 kon de standaard Inno Setup-pagina leeg worden weergegeven. }
  ExtraTasksPage := CreateInputOptionPage(
    wpSelectDir,
    'Selecteer extra taken',
    'Welke extra taken moeten uitgevoerd worden?',
    'Selecteer de extra taken die u door Setup wilt laten uitvoeren en klik vervolgens op Volgende.',
    False,
    False
  );
  ExtraTasksPage.Add('Bureaubladsnelkoppeling maken');
  ExtraTasksPage.Values[0] := False;
end;

function ShouldCreateDesktopShortcut(): Boolean;
begin
  if ExtraTasksPage = nil then
  begin
    Result := False;
    Exit;
  end;
  Result := ExtraTasksPage.Values[0];
end;
