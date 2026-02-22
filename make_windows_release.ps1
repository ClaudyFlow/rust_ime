# 1. Ensure building in release mode
Write-Host "Building release version..." -ForegroundColor Cyan
cargo build --release

# 2. Create release directory with Timestamp
$Timestamp = Get-Date -Format "yyyyMMdd_HHmm"
if (!(Test-Path "release")) { New-Item -ItemType Directory "release" }
$ReleaseDir = "release/rust-ime-windows-$Timestamp"

if (Test-Path $ReleaseDir) { Remove-Item -Recurse -Force $ReleaseDir }
New-Item -ItemType Directory $ReleaseDir

# 3. Copy binaries
Write-Host "Collecting files into $ReleaseDir..." -ForegroundColor Green
if (Test-Path "target/release/rust-ime.exe") {
    Copy-Item "target/release/rust-ime.exe" $ReleaseDir
}
if (Test-Path "target/release/rust_ime_tsf_v3.dll") {
    Copy-Item "target/release/rust_ime_tsf_v3.dll" $ReleaseDir
}

# 4. Copy resource files
$Resources = @("dicts", "static", "fonts", "sounds", "data", "picture")
foreach ($Res in $Resources) {
    if (Test-Path $Res) { 
        Copy-Item -Recurse $Res $ReleaseDir 
    }
}

if (Test-Path "config.json") { Copy-Item "config.json" $ReleaseDir }
Copy-Item "INSTALL_GUIDE.md" $ReleaseDir
Copy-Item "INSTALL_GUIDE_ZH.md" $ReleaseDir

# 5. Write bat files in English to avoid encoding issues
$InstallBat = @"
@echo off
:: Check for administrator privileges
net session >nul 2>&1
if %errorLevel% neq 0 (
    echo Requesting administrator privileges...
    powershell -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
    exit /b
)
cd /d %~dp0
if not exist "rust-ime.exe" (
    echo Error: rust-ime.exe not found in this folder!
    pause
    exit /b
)
echo Registering IME...
.\rust-ime.exe --register
reg add HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v RustIme /t REG_SZ /d "\"%~dp0rust-ime.exe\" --daemon" /f
echo Starting service...
start "" ".\rust-ime.exe" --daemon
echo Installation complete! Please add Rust IME in system language settings.
pause
"@

$UninstallBat = @"
@echo off
:: Check for administrator privileges
net session >nul 2>&1
if %errorLevel% neq 0 (
    echo Requesting administrator privileges...
    powershell -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
    exit /b
)
cd /d %~dp0
echo Stopping Rust IME...
taskkill /F /IM rust-ime.exe /T 2>nul
echo Unregistering IME...
.\rust-ime.exe --unregister
echo Cleaning up registry...
reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v RustIme /f 2>nul
echo.
echo Uninstallation complete!
echo Note: If rust_ime_tsf_v3.dll cannot be deleted, please restart your computer or close all applications.
pause
"@

# Use ASCII/UTF8 without BOM for bat files to be safe in CMD
$InstallBat | Out-File -FilePath "$ReleaseDir\install.bat" -Encoding ascii
$UninstallBat | Out-File -FilePath "$ReleaseDir\uninstall.bat" -Encoding ascii

Write-Host "Packaging complete! Files are available in: $ReleaseDir" -ForegroundColor Yellow
