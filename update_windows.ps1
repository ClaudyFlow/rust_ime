# Windows IME Force Update Script
$ErrorActionPreference = "Continue"

Write-Host "--- 1. Killing rust-ime process..." -ForegroundColor Cyan
Get-Process rust-ime -ErrorAction SilentlyContinue | Stop-Process -Force

Write-Host "--- 2. Unregistering TSF service..." -ForegroundColor Cyan
$ExePath = if (Test-Path "target\release\rust-ime.exe") { "target\release\rust-ime.exe" } else { "target\debug\rust-ime.exe" }

if (Test-Path $ExePath) {
    & $ExePath --unregister
}

Write-Host "--- 3. Waiting for DLL release..." -ForegroundColor Cyan
Start-Sleep -Seconds 2

Write-Host "--- 4. Compiling (cargo build)..." -ForegroundColor Cyan
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "!!! Error: cargo build failed." -ForegroundColor Red
    exit $LASTEXITCODE
}

Write-Host "--- 5. Registering new TSF DLL..." -ForegroundColor Cyan
$NewExePath = "target\release\rust-ime.exe"
if (Test-Path $NewExePath) {
    & $NewExePath --register
}

Write-Host "--- 6. Starting background process..." -ForegroundColor Cyan
if (Test-Path $NewExePath) {
    Start-Process $NewExePath "--daemon"
}

Write-Host "========================================" -ForegroundColor Green
Write-Host "  Update Finished Successfully!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green
