#define AppName "DPER_Measure"
#ifndef AppVersion
  #define AppVersion "0.2.0"
#endif

[Setup]
AppId={{2F6552BE-6B4B-4C40-9C38-438795148E57}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=DPER
DefaultDirName={autopf}\DPER_Measure
DefaultGroupName={#AppName}
UninstallDisplayName={#AppName}
ArchitecturesInstallIn64BitMode=x64
PrivilegesRequired=admin
OutputDir=..\dist\installer
OutputBaseFilename=DPER_Measure_Setup_{#AppVersion}_x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
DisableProgramGroupPage=yes
CloseApplications=ask
RestartApplications=no

[Languages]
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "Créer un raccourci sur le bureau"; GroupDescription: "Raccourcis :"; Flags: unchecked

[Files]
Source: "..\target\release\dper_measure.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\*"; DestDir: "{app}\assets"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\DPER_Measure"; Filename: "{app}\dper_measure.exe"
Name: "{autodesktop}\DPER_Measure"; Filename: "{app}\dper_measure.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\dper_measure.exe"; Description: "Lancer DPER_Measure"; Flags: postinstall nowait skipifsilent
