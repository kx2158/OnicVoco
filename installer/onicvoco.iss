; Inno Setup script for the OnicVoco Windows installer.
; Built by .github/workflows/release.yml. Installs per-user (no admin needed).

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{6C1E4F2B-8A3D-4C55-9B7E-0E5D2C9A41F7}
AppName=OnicVoco
AppVersion={#AppVersion}
AppPublisher=OnicVoco
DefaultDirName={localappdata}\Programs\OnicVoco
DefaultGroupName=OnicVoco
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=OnicVoco-{#AppVersion}-setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\OnicVoco.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"

[Files]
Source: "..\target\release\onicvoco.exe"; DestDir: "{app}"; DestName: "OnicVoco.exe"; Flags: ignoreversion

[Icons]
Name: "{group}\OnicVoco"; Filename: "{app}\OnicVoco.exe"
Name: "{userdesktop}\OnicVoco"; Filename: "{app}\OnicVoco.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\OnicVoco.exe"; Description: "Launch OnicVoco"; Flags: nowait postinstall skipifsilent
