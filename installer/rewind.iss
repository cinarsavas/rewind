; Inno Setup script for Rewind. Built by the release workflow:
;   iscc /DMyAppVersion=1.0.0 installer\rewind.iss
; Expects the staged app (Rewind.exe + the OBS runtime) in dist\stage.

#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif
#define MyAppName "Rewind"
#define MyAppExe "Rewind.exe"
#ifndef MyRepoUrl
  #define MyRepoUrl "https://github.com/OWNER/rewind"
#endif

[Setup]
AppId={{D3747A36-CB18-45D4-B79E-1953CE8C6990}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher=Rewind contributors
AppPublisherURL={#MyRepoUrl}
AppSupportURL={#MyRepoUrl}/issues
AppUpdatesURL={#MyRepoUrl}/releases
VersionInfoVersion={#MyAppVersion}
DefaultDirName={autopf}\Rewind
DefaultGroupName=Rewind
DisableProgramGroupPage=yes
OutputDir=..\dist
OutputBaseFilename=Rewind-Setup-{#MyAppVersion}
SetupIconFile=..\crates\app\assets\rewind.ico
UninstallDisplayIcon={app}\{#MyAppExe}
LicenseFile=..\LICENSE
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
; Per-user install by default: no administrator prompt, nothing touches system folders.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
; Wait for / close a running Rewind (matches the app's single-instance mutex).
AppMutex=RewindInstantReplaySingleInstance
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "startup"; Description: "Start Rewind hidden in the tray when I sign in"; GroupDescription: "Startup:"; Flags: unchecked

[Files]
Source: "..\dist\stage\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion

[Icons]
Name: "{autoprograms}\Rewind"; Filename: "{app}\{#MyAppExe}"
Name: "{autodesktop}\Rewind"; Filename: "{app}\{#MyAppExe}"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Rewind"; ValueData: """{app}\{#MyAppExe}"" --tray"; Tasks: startup; Flags: uninsdeletevalue

[Run]
Filename: "{app}\{#MyAppExe}"; Description: "Launch Rewind"; Flags: nowait postinstall skipifsilent

[Code]
// The app can also add its own "start with Windows" entry from Settings; remove it on uninstall,
// and offer to remove settings and downloaded add-ons.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Rewind');
  if CurUninstallStep = usPostUninstall then
  begin
    if DirExists(ExpandConstant('{userappdata}\rewind\Rewind')) then
      if MsgBox('Also delete your Rewind settings and downloaded add-ons?' + #13#10 +
                '(Your saved clips are never touched.)', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        DelTree(ExpandConstant('{userappdata}\rewind\Rewind'), True, True, True);
  end;
end;
