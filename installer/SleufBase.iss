#define MyAppName "SleufBase"
#define MyAppVersion "0.3.6"
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
VersionInfoVersion=0.3.6.0
VersionInfoCompany={#MyAppPublisher}
VersionInfoDescription=SleufBase Windows installer
VersionInfoProductName={#MyAppName}
VersionInfoProductVersion={#MyAppVersion}

[Languages]
Name: "dutch"; MessagesFile: "compiler:Languages\Dutch.isl"

[Tasks]
Name: "desktopicon"; Description: "Bureaubladsnelkoppeling maken"; GroupDescription: "Extra snelkoppelingen:"; Flags: unchecked

[Files]
Source: "..\dist\SleufBase\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\SleufBase"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"
Name: "{autodesktop}\SleufBase"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "SleufBase starten"; Flags: nowait postinstall skipifsilent
