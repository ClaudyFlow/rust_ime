# sync_dev.ps1 - 用于本地开发环境快速测试
$ErrorActionPreference = "Stop"

Write-Host "--- 快速同步开发版本 ---" -ForegroundColor Cyan

# 1. 编译
Write-Host "正在编译 (Release 模式)..." -ForegroundColor Yellow
cargo build --release

# 2. 检查权限
$principal = [Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Host "警告: 注册输入法需要管理员权限。正在尝试提权..." -ForegroundColor Red
    Start-Process powershell -Verb RunAs -ArgumentList "-File", "$PSCommandPath"
    exit
}

# 3. 停止当前运行的程序
Write-Host "正在停止旧进程..." -ForegroundColor Yellow
taskkill /F /IM rust-ime.exe /T 2>$null

# 4. 注册 DLL (原地注册)
Write-Host "正在原地注册 TSF 组件..." -ForegroundColor Yellow
$DllPath = Join-Path (Get-Location) "target\release\rust_ime_tsf_v3.dll"
regsvr32 /s "$DllPath"

# 5. 编译词库 (如果需要)
if (!(Test-Path "data") -or (Get-ChildItem "data").Count -eq 0) {
    Write-Host "正在初始化词库数据..." -ForegroundColor Yellow
    & "target\release\rust-ime.exe" --compile-only
}

# 6. 启动程序
Write-Host "正在启动新版本..." -ForegroundColor Green
Start-Process "target\release\rust-ime.exe"

Write-Host "同步完成！现在可以直接测试了。" -ForegroundColor Cyan
