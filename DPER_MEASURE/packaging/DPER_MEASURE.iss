#define AppName "DPER_Measure"
#ifndef AppVersion
  #error AppVersion doit etre fourni par build-windows-installer.ps1
#endif
#ifndef PackageRoot
  #error PackageRoot doit pointer vers le contenu prepare par build-windows-installer.ps1
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
ArchitecturesAllowed=x64
PrivilegesRequired=admin
OutputDir=..\dist\installer
OutputBaseFilename=DPER_Measure_Setup_{#AppVersion}_x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
DisableProgramGroupPage=yes
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "Créer un raccourci sur le bureau"; GroupDescription: "Raccourcis :"; Flags: unchecked

[Files]
Source: "{#PackageRoot}\dper_measure.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PackageRoot}\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PackageRoot}\NOTICE_UTILISATEUR.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PackageRoot}\assets\demo_calibration_virtuelle.png"; DestDir: "{app}\assets"; Flags: ignoreversion

[Icons]
Name: "{group}\DPER_Measure"; Filename: "{app}\dper_measure.exe"
Name: "{autodesktop}\DPER_Measure"; Filename: "{app}\dper_measure.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\dper_measure.exe"; Description: "Lancer DPER_Measure"; Flags: postinstall nowait skipifsilent

[Code]
function InitializeSetup(): Boolean;
var
  Installed: Cardinal;
begin
  Installed := 0;
  Result := RegQueryDWordValue(HKLM64,
    'SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64', 'Installed', Installed)
    and (Installed = 1);
  if not Result then
    MsgBox('Installez le runtime Microsoft Visual C++ v14 x64 avant DPER_Measure, puis relancez cet installateur.', mbError, MB_OK);
end;
