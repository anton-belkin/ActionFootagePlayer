; ActionFootagePlayer's installer for Windows (Inno Setup 6), made by tools/make_windows_app.sh:
;   ISCC /DAppVersion=<version> /DSourceDir=<the app's folder, build\windows-app> /O<out> windows\installer.iss
;
; For the user alone, without admin rights: into %LOCALAPPDATA%\Programs\ActionFootagePlayer, the settings in HKCU.
; ActionFootagePlayer is in Open With for MP4 and MOV videos and in Windows' Default apps; Explorer's menu for them
; gets Play in ActionFootagePlayer and Stabilize + LUT. Windows doesn't let an app make itself the default: the
; installer opens Settings for it. DJI's LUT is downloaded from DJI. Uninstalling leaves what's yours in
; %LOCALAPPDATA%\ActionFootagePlayer (DJI's LUT, your default preset).

#ifndef AppVersion
  #define AppVersion "dev"
#endif
#ifndef SourceDir
  #define SourceDir "..\build\windows-app"
#endif
#define AppName "ActionFootagePlayer"
#define AppExe "ActionFootagePlayer.exe"
#define ProgId "ActionFootagePlayer.Video"

[Setup]
AppId={{6B0E9C1E-3D7A-4E5B-9A51-AF0C0D1E2F31}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisherURL=https://github.com/anton-belkin/ActionFootagePlayer
AppSupportURL=https://github.com/anton-belkin/ActionFootagePlayer/issues
DefaultDirName={autopf}\{#AppName}
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputBaseFilename={#AppName}-{#AppVersion}-windows-x86_64-setup
SetupIconFile=AppIcon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ChangesAssociations=yes
CloseApplications=yes

[Tasks]
Name: lut; Description: "Download DJI's D-Log M to Rec.709 LUT from DJI (Ctrl+s, and Rec.709 exports)"
Name: default; Description: "Make it the default app for MP4 videos (Windows asks you to choose it in Settings)"

[InstallDelete]
; What an earlier version put there that this one may not have
Type: filesandordirs; Name: "{app}\mpv"
Type: filesandordirs; Name: "{app}\python"
Type: filesandordirs; Name: "{app}\plugins"
Type: filesandordirs; Name: "{app}\tools"
Type: filesandordirs; Name: "{app}\licenses"

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"

[Registry]
; The kind of file it opens: Open With, and what Default apps lists for it
Root: HKCU; Subkey: "Software\Classes\{#ProgId}"; ValueType: string; ValueData: "Video ({#AppName})"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\{#ProgId}\DefaultIcon"; ValueType: string; ValueData: "{app}\{#AppExe},0"
Root: HKCU; Subkey: "Software\Classes\{#ProgId}\shell\open\command"; ValueType: string; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\.mp4\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.mov\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\Applications\{#AppExe}"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#AppName}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Applications\{#AppExe}\shell\open\command"; ValueType: string; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".mp4"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\{#AppExe}\SupportedTypes"; ValueType: string; ValueName: ".mov"; ValueData: ""
; Default apps
Root: HKCU; Subkey: "Software\{#AppName}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\{#AppName}\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#AppName}"
Root: HKCU; Subkey: "Software\{#AppName}\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Action camera footage in full color, stabilized, with what the camera measured"
Root: HKCU; Subkey: "Software\{#AppName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".mp4"; ValueData: "{#ProgId}"
Root: HKCU; Subkey: "Software\{#AppName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".mov"; ValueData: "{#ProgId}"
Root: HKCU; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "{#AppName}"; ValueData: "Software\{#AppName}\Capabilities"; Flags: uninsdeletevalue
; Explorer's menu for videos (on Windows 11 under Show more options): the Finder Quick Actions' counterparts
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.mp4\shell\{#AppName}.play"; ValueType: string; ValueData: "Play in {#AppName}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.mp4\shell\{#AppName}.play"; ValueType: string; ValueName: "Icon"; ValueData: "{app}\{#AppExe},0"
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.mp4\shell\{#AppName}.play\command"; ValueType: string; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.mp4\shell\{#AppName}.export"; ValueType: string; ValueData: "Stabilize + LUT"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.mp4\shell\{#AppName}.export"; ValueType: string; ValueName: "Icon"; ValueData: "{app}\{#AppExe},0"
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.mp4\shell\{#AppName}.export\command"; ValueType: string; ValueData: """{app}\{#AppExe}"" --export ""%1"""
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.mov\shell\{#AppName}.play"; ValueType: string; ValueData: "Play in {#AppName}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.mov\shell\{#AppName}.play"; ValueType: string; ValueName: "Icon"; ValueData: "{app}\{#AppExe},0"
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.mov\shell\{#AppName}.play\command"; ValueType: string; ValueData: """{app}\{#AppExe}"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.mov\shell\{#AppName}.export"; ValueType: string; ValueData: "Stabilize + LUT"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.mov\shell\{#AppName}.export"; ValueType: string; ValueName: "Icon"; ValueData: "{app}\{#AppExe},0"
Root: HKCU; Subkey: "Software\Classes\SystemFileAssociations\.mov\shell\{#AppName}.export\command"; ValueType: string; ValueData: """{app}\{#AppExe}"" --export ""%1"""

[Run]
Filename: "{app}\python\python.exe"; Parameters: "-I ""{app}\tools\setup.py"" lut"; StatusMsg: "Downloading DJI's LUT..."; Flags: runhidden; Tasks: lut
Filename: "ms-settings:defaultapps?registeredAppUser={#AppName}"; Flags: shellexec nowait; Tasks: default

[UninstallDelete]
Type: filesandordirs; Name: "{localappdata}\{#AppName}\export-queue"
