# make_portable.ps1 - 生成绿色便携版文件夹
$ErrorActionPreference = "Stop"

Write-Host "--- 正在生成绿色便携版 ---" -ForegroundColor Cyan

# 1. 编译
Write-Host "正在编译 Release 版本..." -ForegroundColor Yellow
cargo build --release

# 2. 预编译词库
Write-Host "正在预编译词库数据..." -ForegroundColor Yellow
if (Test-Path "target/release/rust-ime.exe") {
    & "target/release/rust-ime.exe" --compile-only
}

# 3. 创建输出目录
$DistDir = "portable_release"
if (Test-Path $DistDir) { Remove-Item -Recurse -Force $DistDir }
New-Item -ItemType Directory $DistDir | Out-Null

# 4. 拷贝二进制文件
Write-Host "正在收集核心文件..." -ForegroundColor Green
Copy-Item "target/release/rust-ime.exe" $DistDir
Copy-Item "target/release/rust_ime_tsf_v3.dll" $DistDir

# 5. 拷贝资源文件夹
$Resources = @("dicts", "static", "fonts", "sounds", "data", "picture", "configs")
foreach ($Res in $Resources) {
    if (Test-Path $Res) {
        Write-Host "正在拷贝资源: $Res" -ForegroundColor Gray
        Copy-Item -Recurse $Res (Join-Path $DistDir $Res)
    }
}

# 6. 生成简单的注册/卸载批处理
$InstallBat = @"
@echo off
net session >nul 2>&1
if %errorLevel% neq 0 (
    echo 请以管理员权限运行此脚本来注册输入法！
    pause
    exit /b
)
cd /d %~dp0
echo 正在注册输入法...
regsvr32 /s rust_ime_tsf_v3.dll
echo 正在启动程序...
start "" rust-ime.exe
echo 完成！
pause
"@

$UninstallBat = @"
@echo off
net session >nul 2>&1
if %errorLevel% neq 0 (
    echo 请以管理员权限运行此脚本！
    pause
    exit /b
)
cd /d %~dp0
echo 正在停止进程...
taskkill /f /im rust-ime.exe /t 2>nul
echo 正在反注册...
regsvr32 /u /s rust_ime_tsf_v3.dll
echo 完成！
pause
"@

$InstallBat | Out-File -FilePath "$DistDir\一键注册.bat" -Encoding ascii
$UninstallBat | Out-File -FilePath "$DistDir\一键卸载.bat" -Encoding ascii

Write-Host "`n生成完成！便携版目录位于: $DistDir" -ForegroundColor Cyan
Write-Host "提示：你可以直接把整个 $DistDir 文件夹发给别人测试。" -ForegroundColor Gray
