; PublishKit — Windows Installer
; Build: iscc /DMyAppVersion=X.Y.Z installer\installer.iss
; Output: dist\publishkit-setup.exe

#ifndef MyAppVersion
  #define MyAppVersion "0.1.0"
#endif

#define MyAppName      "PublishKit"
#define MyAppPublisher "Mayorana"
#define MyAppURL       "https://github.com/Bennekrouf/publishkit"
#define MyAppExeName   "publishkit.exe"

[Setup]
AppId={{C3A72F1B-8E4D-4B9C-A1F6-D7E5C2B08A34}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
AppUpdatesURL={#MyAppURL}/releases/latest
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
AllowNoIcons=yes
OutputDir=..\dist
OutputBaseFilename=publishkit-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
; Always a per-user install (no UAC prompt): {autopf} resolves to
; %LOCALAPPDATA%\Programs, the folder the site's MCP command and Claude Code
; entries point at. Blog Manager 0.1.4 and earlier installed machine-wide under
; the same AppId, so UsePreviousPrivileges is off: otherwise an upgrade follows
; that old install into Program Files. /ALLUSERS still installs machine-wide.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=commandline
UsePreviousPrivileges=no
; Start menu entries under the app's name, not the folder an earlier install
; made under the old name (removed in [InstallDelete]).
UsePreviousGroup=no
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Windows 10 1809+ required for WebView2
MinVersion=10.0.17763
UninstallDisplayName={#MyAppName} {#MyAppVersion}
CloseApplications=yes
; Shows the licence (PolyForm Noncommercial) as a page the user accepts before installing.
LicenseFile=..\LICENSE
; Branding — uses assets\icon.ico if present (the release workflow makes it
; from assets\icon.png). Skipped silently otherwise.
#if FileExists(AddBackslash(SourcePath) + "..\assets\icon.ico")
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\icon.ico
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "french";  MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; \
  Description: "Create a &desktop shortcut"; \
  GroupDescription: "Additional shortcuts:"
Name: "installdeps"; \
  Description: "Install runtime dependencies  (Node.js >=20, WebView2)"; \
  GroupDescription: "Runtime dependencies:"; \
  Flags: checkedonce

[Files]
Source: "..\target\release\publishkit.exe";   DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\WebView2Loader.dll"; DestDir: "{app}"; Flags: ignoreversion; Check: FileExists('..\target\release\WebView2Loader.dll')
Source: "..\scripts\setup-windows.ps1";         DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md";                         DestDir: "{app}"; Flags: ignoreversion isreadme
#if FileExists(AddBackslash(SourcePath) + "..\assets\icon.ico")
; Shortcuts point at the .ico explicitly: some Windows builds fail to extract
; the icon embedded in the .exe for shortcut display.
Source: "..\assets\icon.ico";                   DestDir: "{app}"; Flags: ignoreversion
#endif

[InstallDelete]
; Shortcuts left by installs from when the app was called Blog Manager.
Type: filesandordirs; Name: "{autoprograms}\Blog Manager"
Type: files;          Name: "{autodesktop}\Blog Manager.lnk"

[Icons]
#if FileExists(AddBackslash(SourcePath) + "..\assets\icon.ico")
Name: "{group}\{#MyAppName}";           Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\icon.ico"
Name: "{group}\Uninstall {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}";   Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\icon.ico"; Tasks: desktopicon
#else
Name: "{group}\{#MyAppName}";           Filename: "{app}\{#MyAppExeName}"
Name: "{group}\Uninstall {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}";   Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon
#endif

[Run]
Filename: "powershell.exe"; \
  Parameters: "-ExecutionPolicy Bypass -NoProfile -File ""{app}\setup-windows.ps1"" -NoPrompt"; \
  Tasks: installdeps; \
  StatusMsg: "Installing runtime dependencies..."; \
  Flags: waituntilterminated runascurrentuser

Filename: "{app}\{#MyAppExeName}"; \
  Description: "Launch {#MyAppName}"; \
  Flags: nowait postinstall skipifsilent
