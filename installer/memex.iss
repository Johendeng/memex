; Memex — Guided Installer (Inno Setup)
; Build: ISCC.exe memex.iss
; Output: C:\workspace\my_project\memex\installer\Output\Memex-Setup-1.0.6.exe
;
; Target directory: C:\Memex\
; Structure:
;   C:\Memex\memex.exe          (main binary)
;   C:\Memex\models\              (ONNX model)
;   C:\Memex\data\                (index / cache — created at runtime)
;   C:\Memex\unins000.exe         (uninstaller — created by Inno Setup)
;
; Registry: only the HKLM uninstall entry (standard, so Windows app list works).
; Memex itself writes NOTHING to the registry at runtime.

[Setup]
AppName=Memex
AppId={{MEMEX-2026-JohenDeng}
AppVersion=1.0.6
AppPublisher=JohenDeng
AppPublisherURL=https://github.com/JohenDeng/memex
DefaultDirName=C:\Memex
DefaultGroupName=Memex
DisableProgramGroupPage=yes
OutputDir=Output
OutputBaseFilename=Memex-Setup-1.0.6
Compression=lzma2/ultra64
SolidCompression=yes
PrivilegesRequired=admin
WizardStyle=modern
SetupIconFile=..\assets\memex_icon.ico
UninstallDisplayIcon={app}\memex.exe
; Close running Memex before upgrade
CloseApplications=yes
; Do not create a Start Menu folder (we only want Desktop + optional Quick Launch)
DisableDirPage=no
; Allow user to change install directory
AlwaysShowDirOnReadyPage=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create desktop shortcut"; GroupDescription: "Additional options:"
Name: "startupicon"; Description: "Auto-start on login (minimized to tray)"; GroupDescription: "Additional options:"; Flags: unchecked

[Files]
; Main binary
Source: "..\target\release\memex.exe"; DestDir: "{app}"; Flags: ignoreversion
; Runtime DLLs (required for vector embedding + WebView2)
Source: "..\target\release\onnxruntime.dll"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\target\release\onnxruntime_providers_shared.dll"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\target\release\WebView2Loader.dll"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
; pdfium.dll (optional — for enhanced PDF parsing, ~7MB)
Source: "..\target\release\pdfium.dll"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
; ONNX model (~91MB)
Source: "..\models\*"; DestDir: "{app}\models"; Flags: ignoreversion recursesubdirs createallsubdirs
; OCR model files (optional — PP-OCRv6 Tiny, ~6MB)
Source: "..\models\ocr\*"; DestDir: "{app}\models\ocr"; Flags: ignoreversion skipifsourcedoesntexist
; README
Source: "README.txt"; DestDir: "{app}"; Flags: ignoreversion
; Launcher (desktop shortcut target — shows mode selection menu)
Source: "launcher.bat"; DestDir: "{app}"; Flags: ignoreversion

[Dirs]
; Create empty data directory (runtime will populate)
Name: "{app}\data"

[Icons]
Name: "{autodesktop}\Memex"; Filename: "{app}\launcher.bat"; WorkingDir: "{app}"; IconFilename: "{app}\memex.exe"; Tasks: desktopicon; Comment: "Memex 本地知识库"
Name: "{autostartup}\Memex"; Filename: "{app}\memex.exe"; WorkingDir: "{app}"; Parameters: "serve --no-window"; Tasks: startupicon; Comment: "Memex 本地知识库（后台模式）"

[Run]
; Launch after install (optional, user can uncheck)
Filename: "{app}\launcher.bat"; WorkingDir: "{app}"; Description: "立即启动 Memex"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Kill the process before uninstalling
Filename: "{cmd}"; Parameters: "/C taskkill /F /IM memex.exe"; Flags: runhidden

[UninstallDelete]
; Clean up data directory on uninstall (user data — ask first via confirmation)
Type: filesandordirs; Name: "{app}\data"
Type: dirifempty; Name: "{app}"

[Code]
procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
  begin
    // Ensure data directory exists and is writable
    CreateDir(ExpandConstant('{app}\data'));
  end;
end;
