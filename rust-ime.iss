#define MyAppName "Rust IME"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "Rust-IME Team"
#define MyAppExeName "rust-ime.exe"
#define MyAppDllName "rust_ime_tsf_v3.dll"

[Setup]
; 唯一 AppID，防止重复安装
AppId={{A7F23B4C-D5E6-4F7G-8H9I-J0K1L2M3N4O5}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
AllowNoIcons=yes
; 输出文件名
OutputBaseFilename=RustIME_Setup
Compression=lzma
SolidCompression=yes
WizardStyle=modern
; 需要管理员权限进行 TSF 注册
PrivilegesRequired=admin
SetupIconFile=picture\rust-ime_v2.ico

[Languages]
Name: "chinesesimplified"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "startup"; Description: "开机自动启动后台程序"; GroupDescription: "其他任务:";

[Files]
; 主程序和 DLL
Source: "target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "target\release\{#MyAppDllName}"; DestDir: "{app}"; Flags: ignoreversion
; 词库数据 (包含子目录)
Source: "data\*"; DestDir: "{app}\data"; Flags: ignoreversion recursesubdirs createallsubdirs
; 静态网页资源
Source: "static\*"; DestDir: "{app}\static"; Flags: ignoreversion recursesubdirs createallsubdirs
; 默认配置
Source: "configs\*"; DestDir: "{app}\configs"; Flags: ignoreversion recursesubdirs createallsubdirs
; 图标
Source: "picture\*"; DestDir: "{app}\picture"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Registry]
; 开机自启动
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "RustIME"; ValueData: """{app}\{#MyAppExeName}"""; Flags: uninsdeletevalue; Tasks: startup

[Run]
; 注册 TSF DLL (核心步骤)
Filename: "regsvr32.exe"; Parameters: "/s ""{app}\{#MyAppDllName}"""; StatusMsg: "正在向系统注册输入法组件..."; Flags: runhidden
; 安装完成后启动后台
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; 卸载前先停掉后台进程 (防止文件占用)
Filename: "taskkill.exe"; Parameters: "/f /im {#MyAppExeName}"; RunOnceId: "StopApp"; Flags: runhidden
; 反注册 TSF DLL
Filename: "regsvr32.exe"; Parameters: "/u /s ""{app}\{#MyAppDllName}"""; StatusMsg: "正在注销输入法组件..."; Flags: runhidden
