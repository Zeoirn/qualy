; ==============================================================================
; Qualy Inno Setup Script
; Requires Inno Setup 6.0 or later: https://jrsoftware.org/isinfo.php
;
; Build invocation:
;   ISCC.exe /DAppVersion=0.5.2 installer\windows\mechvibes-dx-setup.iss
; ==============================================================================

#ifndef AppVersion
  #error AppVersion must be defined, e.g. ISCC /DAppVersion=0.5.2 mechvibes-dx-setup.iss
#endif

#define MyAppName "qualy"
#define MyAppDisplayName "MechvibesDX (Qualy)"
#define MyAppVersion AppVersion
#define MyAppPublisher "Hai Nguyen"
#define MyAppURL "https://github.com/Zeoirn/qualy"
#define MyAppExeName "mechvibes-dx.exe"
#define MyAppId "{{8B5F5E5E-5E5E-5E5E-5E5E-5E5E5E5E5E5E}"

[Setup]
AppId={#MyAppId}
AppName={#MyAppDisplayName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
AppUpdatesURL={#MyAppURL}/releases

; Installation locations
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppDisplayName}
DisableProgramGroupPage=yes
DisableWelcomePage=no
AllowNoIcons=yes

; Binary & UI branding
LicenseFile=..\..\LICENSE
OutputDir=..\..\dist
; OutputBaseFilename MUST contain "x64" and end in ".exe" for the in-app auto-updater
OutputBaseFilename=MechvibesDX-{#MyAppVersion}-Setup-x64
SetupIconFile=..\..\assets\icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
WizardStyle=modern

; Compression
Compression=lzma2/max
SolidCompression=yes

; Privileges & Architecture
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesInstallIn64BitMode=x64compatible
ArchitecturesAllowed=x64compatible

; Restart Manager & app termination (scoped strictly to target exe)
CloseApplications=yes
CloseApplicationsFilter={#MyAppExeName}
RestartApplications=yes

; File details metadata for setup.exe properties
VersionInfoVersion={#MyAppVersion}
VersionInfoCompany={#MyAppPublisher}
VersionInfoDescription={#MyAppDisplayName} Setup
VersionInfoProductName={#MyAppDisplayName}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "startup"; Description: "Run at Windows startup (minimized to tray)"; GroupDescription: "Additional options:"; Flags: unchecked

[Files]
; Main executable
Source: "..\..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion

; Assets folder (required alongside executable for Dioxus asset!() macro)
Source: "..\..\assets\*"; DestDir: "{app}\assets"; Flags: ignoreversion recursesubdirs createallsubdirs

; Soundpacks folder (built-in soundpacks)
Source: "..\..\soundpacks\*"; DestDir: "{app}\soundpacks"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
; Start Menu shortcut
Name: "{autoprograms}\{#MyAppDisplayName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\{#MyAppExeName}"; IconIndex: 0

; Desktop shortcut
Name: "{autodesktop}\{#MyAppDisplayName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\{#MyAppExeName}"; IconIndex: 0; Tasks: desktopicon

[Registry]
; Windows startup registration (minimized to system tray)
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#MyAppName}"; ValueData: """{app}\{#MyAppExeName}"" --minimized"; Flags: uninsdeletevalue; Tasks: startup

[Run]
; Interactive installs: launch checkbox on final wizard page
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#MyAppDisplayName}}"; WorkingDir: "{app}"; Flags: nowait postinstall skipifsilent

; Silent installs (Auto-updater path): relaunch app under the desktop user session
Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; Flags: nowait runasoriginaluser skipifnotsilent

[UninstallDelete]
; Clean up app directory runtime caches and logs
Type: filesandordirs; Name: "{app}\data"
Type: filesandordirs; Name: "{localappdata}\{#MyAppName}"
; NOTE: User soundpacks in {userappdata}\Mechvibes are handled conditionally in [Code]

[Code]
// Native Windows Shell API to notify of icon and association changes
procedure SHChangeNotify(wEventId: LongInt; uFlags: Cardinal; dwItem1, dwItem2: Cardinal);
  external 'SHChangeNotify@shell32.dll stdcall';

const
  SHCNE_ASSOCCHANGED = $08000000;
  SHCNF_IDLIST       = $00000000;

procedure RefreshIconCache();
begin
  try
    SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, 0, 0);
  except
    // Non-fatal if notification fails
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
  begin
    // Create custom soundpacks directories in AppData for user drop-ins
    CreateDir(ExpandConstant('{userappdata}\Mechvibes\soundpacks\keyboard'));
    CreateDir(ExpandConstant('{userappdata}\Mechvibes\soundpacks\mouse'));

    // Refresh shell icon cache
    RefreshIconCache();
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  DataPath: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    DataPath := ExpandConstant('{userappdata}\Mechvibes');
    
    // Only prompt if not in silent mode and the folder actually exists
    if not UninstallSilent and DirExists(DataPath) then
    begin
      if MsgBox('Do you want to remove all custom soundpacks and user configurations in ' + DataPath + '?', mbConfirmation, MB_YESNO) = IDYES then
      begin
        DelTree(DataPath, True, True, True);
      end;
    end;
    
    RefreshIconCache();
  end;
end;
