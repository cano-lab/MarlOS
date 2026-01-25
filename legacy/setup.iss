[Setup]
AppName=MarlOS
AppVersion=1.0.0
DefaultDirName={pf}\MarlOS
DefaultGroupName=MarlOS
OutputDir=installer
OutputBaseFilename=MarlOS-Setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ShowLanguageDialog=no
UninstallDisplayIcon={app}\MarlOS.exe
CreateAppDir=yes
DisableDirPage=no

[Files]
Source: "dist\MarlOS\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs

[Icons]
Name: "{group}\MarlOS"; Filename: "{app}\MarlOS.exe"
Name: "{commondesktop}\MarlOS"; Filename: "{app}\MarlOS.exe"

[Run]
Filename: "{app}\MarlOS.exe"; Description: "Launch MarlOS"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: filesandordirs; Name: "{app}"
