#define MyAppName "Z-Mortal"
#define MyAppVersion "0.1"
#define MyAppPublisher "Z-Mortal"
#define MyAppURL "https://github.com/daniellelruizl36-cyber/Z-Mortal"

[Setup]
AppId={{7C3A9B1E-4D2F-4E5A-9C6B-8E1F2A3B4C5D}}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={localappdata}\Z-Mortal
DefaultGroupName=Z-Mortal
PrivilegesRequired=lowest
OutputDir=.
OutputBaseFilename=Z-Mortal-Setup
Compression=lzma2/ultra
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop icon"; GroupDescription: "Additional icons:"; Flags: unchecked

[Files]
Source: "Z-Mortal\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs; Excludes: "*.log,erl_crash.dump"

[Icons]
Name: "{group}\Z-Mortal"; Filename: "wscript.exe"; Parameters: """{app}\Launch.vbs"""; WorkingDir: "{app}"
Name: "{userdesktop}\Z-Mortal"; Filename: "wscript.exe"; Parameters: """{app}\Launch.vbs"""; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "wscript.exe"; Parameters: """{app}\Launch.vbs"""; Description: "Launch Z-Mortal now"; Flags: postinstall skipifsilent nowait

[UninstallRun]
Filename: "cmd.exe"; Parameters: "/c ""{app}\stop.bat"""; Flags: runhidden; RunOnceId: "StopZMort"
