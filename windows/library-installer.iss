; Action Footage Library's installer for Windows (Inno Setup 6), made by tools/make_windows_app.sh:
;   ISCC /DAppVersion=<version> /DAppArch=<x86_64 or arm64> /DSourceDir=<the Library's folder, build\windows-library>
;        /O<out> windows\library-installer.iss
;
; A download of its own, apart from ActionFootagePlayer's (windows/installer.iss), which it needs: the Library runs
; the player without a window and its tools, and looks for ActionFootagePlayer.exe beside itself
; (library/src/batch.rs). So it goes into the player's folder, %LOCALAPPDATA%\Programs\ActionFootagePlayer, and
; refuses to install where the player isn't. For the user alone, without admin rights. A trip's Project.afproject
; opens in it. Uninstalling it leaves the player as it is.

#ifndef AppVersion
  #define AppVersion "dev"
#endif
#ifndef AppArch
  #define AppArch "x86_64"
#endif
#if AppArch == "arm64"
  #define Architectures "arm64"
#else
  #define Architectures "x64compatible"
#endif
#ifndef SourceDir
  #define SourceDir "..\build\windows-library"
#endif
#define AppName "ActionFootageLibrary"
#define AppExe "ActionFootageLibrary.exe"
; The player's folder and name: the Library goes beside it. The project's file type keeps the name the player's
; installer gave it, when it carried the Library
#define PlayerName "ActionFootagePlayer"

[Setup]
AppId={{2C5E8F4A-7B1D-4C3E-9F60-AF0C0D1E2F32}
AppName=Action Footage Library
AppVersion={#AppVersion}
AppPublisherURL=https://github.com/anton-belkin/ActionFootagePlayer
AppSupportURL=https://github.com/anton-belkin/ActionFootagePlayer/issues
DefaultDirName={autopf}\{#PlayerName}
DisableProgramGroupPage=yes
DisableDirPage=yes
UsePreviousAppDir=no
PrivilegesRequired=lowest
ArchitecturesAllowed={#Architectures}
ArchitecturesInstallIn64BitMode={#Architectures}
MinVersion=10.0
OutputBaseFilename={#AppName}-{#AppVersion}-windows-{#AppArch}-setup
SetupIconFile=..\library\icons\icon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName=Action Footage Library
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ChangesAssociations=yes
CloseApplications=yes

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\Action Footage Library"; Filename: "{app}\{#AppExe}"

[Registry]
; A trip's Project.afproject opens in Action Footage Library
Root: HKCU; Subkey: "Software\Classes\.afproject"; ValueType: string; ValueData: "{#PlayerName}.Project"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\{#PlayerName}.Project"; ValueType: string; ValueData: "Action Footage project"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\{#PlayerName}.Project\DefaultIcon"; ValueType: string; ValueData: "{app}\{#AppExe},0"
Root: HKCU; Subkey: "Software\Classes\{#PlayerName}.Project\shell\open\command"; ValueType: string; ValueData: """{app}\{#AppExe}"" ""%1"""

[Code]
// Only where the player is: the Library runs it
function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if not FileExists(ExpandConstant('{app}\{#PlayerName}.exe')) then
    Result := 'Install ActionFootagePlayer first: Action Footage Library goes beside it, in ' + ExpandConstant('{app}') + '.';
end;
