; PAM_Edit Video's Windows installer.
;
; Built by build-app.yml with Inno Setup from the staged folder the .msi
; is also made of, so the two ship the same files; this is the one a
; person double-clicks, and it puts PAM_Edit Video in the Start menu and can take
; it out again. Everything it needs is handed in on the command line:
;
;   iscc /DVersion=0.2.7 /DArch=x64compatible /DSuffix=x86_64 ^
;        /DStage=C:\...\stage\PAM_Edit_Video-0.2.7-windows-x86_64 /DOut=C:\...\stage ^
;        assets\windows\pamedit.iss
;
; Arch is Inno's own word for the machine: x64compatible for the x86_64
; build (which also installs on ARM PCs, under emulation), arm64 for the
; native one. Suffix is the bundle's word for it, which names the file.

#ifndef Version
  #error Version is required
#endif
#ifndef Arch
  #error Arch is required: x64compatible or arm64
#endif
#ifndef Suffix
  #error Suffix is required: x86_64 or aarch64
#endif
#ifndef Stage
  #error Stage is required: the staged folder to install
#endif
#ifndef Out
  #define Out "."
#endif

[Setup]
; One id for the life of the product, so an install over an older one is
; an upgrade and not a second copy.
AppId={{29C9CD5C-D098-4744-A91B-E8C13615A344}
AppName=PAM_Edit Video
AppVersion={#Version}
AppVerName=PAM_Edit Video {#Version}
AppPublisher=PAM_Edit Video team
DefaultDirName={autopf}\PAM_Edit Video
DefaultGroupName=PAM_Edit Video
DisableProgramGroupPage=yes
LicenseFile=..\..\LICENSE
OutputDir={#Out}
OutputBaseFilename=PAM_Edit_Video-{#Version}-windows-{#Suffix}-setup
SetupIconFile=..\icons\pamedit.ico
UninstallDisplayIcon={app}\pamedit.ico
UninstallDisplayName=PAM_Edit Video
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed={#Arch}
ArchitecturesInstallIn64BitMode={#Arch}
; For the user alone unless they ask for the machine: no prompt for an
; administrator to install a video editor into one's own account.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
; Follow Windows' display language when the setup speaks it, and ask only
; when it does not.
ShowLanguageDialog=auto

[Languages]
; PAM_Edit Video: the installer speaks English and Vietnamese, as the app does.
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "vietnamese"; MessagesFile: "languages\Vietnamese.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#Stage}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\icons\pamedit.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\PAM_Edit Video"; Filename: "{app}\pamedit.exe"; IconFilename: "{app}\pamedit.ico"
Name: "{autodesktop}\PAM_Edit Video"; Filename: "{app}\pamedit.exe"; IconFilename: "{app}\pamedit.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\pamedit.exe"; Description: "{cm:LaunchProgram,PAM_Edit Video}"; Flags: nowait postinstall skipifsilent
