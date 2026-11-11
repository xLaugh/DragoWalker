; DragoWalker : installeur Windows (Inno Setup 6).
; Généré par tools\package.ps1, qui passe /DAppVersion=... et /DSrcDir=... (dossier contenant DragoWalker.exe).

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SrcDir
  #error SrcDir manquant : lancez tools\package.ps1
#endif

[Setup]
; Identifiant fixe : ne pas le changer, il permet les mises à jour et la désinstallation.
AppId={{0EBEB443-4422-4E05-9529-34C810B6F3D6}
AppName=DragoWalker
AppVersion={#AppVersion}
AppVerName=DragoWalker {#AppVersion}
AppPublisher=DragoWalker
DefaultDirName={autopf}\DragoWalker
DefaultGroupName=DragoWalker
DisableProgramGroupPage=yes
; Installation pour l'utilisateur seul par défaut (pas besoin d'être administrateur),
; ou pour tout le PC si on le choisit.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#SrcDir}\..
OutputBaseFilename=DragoWalker-{#AppVersion}-setup
UninstallDisplayIcon={app}\DragoWalker.exe
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SrcDir}\DragoWalker.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SrcDir}\LISEZMOI.txt"; DestDir: "{app}"; Flags: ignoreversion isreadme

[Icons]
Name: "{group}\DragoWalker"; Filename: "{app}\DragoWalker.exe"
Name: "{group}\{cm:UninstallProgram,DragoWalker}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\DragoWalker"; Filename: "{app}\DragoWalker.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\DragoWalker.exe"; Description: "{cm:LaunchProgram,DragoWalker}"; Flags: nowait postinstall skipifsilent
; mise à jour depuis l'app (setup lancé en /SILENT) : on relance DragoWalker
Filename: "{app}\DragoWalker.exe"; Flags: nowait; Check: WizardSilent

; Les réglages (%APPDATA%\DragoWalker) et le cache des clés (%LOCALAPPDATA%\DragoWalker)
; sont gardés à la désinstallation, comme les projets de l'utilisateur.
