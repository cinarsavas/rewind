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
; Only Rewind's own programs are ever closed. Games and browsers that Rewind captured can
; keep one of its DLLs (the capture hook) loaded; without this filter Setup offered to close
; them (and an in-app update closed them silently). Such DLLs are moved aside instead, see
; MoveAside below.
CloseApplicationsFilter=*.exe
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "startup"; Description: "Start Rewind hidden in the tray when I sign in"; GroupDescription: "Startup:"; Flags: unchecked

[Files]
Source: "..\dist\stage\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion; BeforeInstall: MoveAside

[Icons]
Name: "{autoprograms}\Rewind"; Filename: "{app}\{#MyAppExe}"
Name: "{autodesktop}\Rewind"; Filename: "{app}\{#MyAppExe}"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Rewind"; ValueData: """{app}\{#MyAppExe}"" --tray"; Tasks: startup; Flags: uninsdeletevalue

[Run]
Filename: "{app}\{#MyAppExe}"; Description: "Launch Rewind"; Flags: nowait postinstall skipifsilent
; In-app updates run this installer silently with /RELAUNCH: start the new version afterwards.
Filename: "{app}\{#MyAppExe}"; Parameters: "--updated"; Flags: nowait; Check: ShouldRelaunch

[Code]
// True when Rewind's own updater started this installer (it passes /RELAUNCH).
function ShouldRelaunch: Boolean;
var
  I: Integer;
begin
  Result := False;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), '/RELAUNCH') = 0 then
      Result := True;
end;

// A file another program still has loaded (a game holding the capture hook DLL) can't be
// overwritten, but Windows does allow renaming it. Rename it out of the way so the new copy
// can be written; the program keeps using the old one until it closes, and the leftover is
// removed by the next install. Nothing is closed and no restart is needed.
procedure MoveAside;
var
  Target, Old: String;
  N: Integer;
begin
  Target := ExpandConstant(CurrentFileName);
  if not FileExists(Target) then
    Exit;
  if DeleteFile(Target) then
    Exit;
  N := 0;
  repeat
    N := N + 1;
    Old := Target + '.old' + IntToStr(N);
  until (not FileExists(Old)) or (N > 50);
  if FileExists(Old) then
    DeleteFile(Old);
  RenameFile(Target, Old);
end;

// Remove files earlier installs moved aside (only those no program still holds).
procedure CleanMovedAside(const Dir: String);
var
  F: TFindRec;
begin
  if FindFirst(Dir + '\*', F) then
  try
    repeat
      if (F.Name <> '.') and (F.Name <> '..') then
      begin
        if (F.Attributes and FILE_ATTRIBUTE_DIRECTORY) <> 0 then
          CleanMovedAside(Dir + '\' + F.Name)
        else if (Pos('.dll.old', Lowercase(F.Name)) > 0) or (Pos('.exe.old', Lowercase(F.Name)) > 0) then
          DeleteFile(Dir + '\' + F.Name);
      end;
    until not FindNext(F);
  finally
    FindClose(F);
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    CleanMovedAside(ExpandConstant('{app}'));
end;

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
