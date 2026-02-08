# Windows IME Force Update Script (English Only)
$ErrorActionPreference = "Continue"

Write-Host "--- 1. Killing rust-ime process..." -ForegroundColor Cyan
Get-Process rust-ime -ErrorAction SilentlyContinue | Stop-Process -Force

Write-Host "--- 2. Unregistering TSF service..." -ForegroundColor Cyan
if (Test-Path "target\debug\rust-ime.exe") {
    & "target\debug\rust-ime.exe" --unregister
}

Write-Host "--- 3. Waiting for DLL release..." -ForegroundColor Cyan
Start-Sleep -Seconds 2

Write-Host "--- 4. Compiling (cargo build)..." -ForegroundColor Cyan
cargo build
if ($LASTEXITCODE -ne 0) {
    Write-Host "!!! Error: cargo build failed." -ForegroundColor Red
    exit $LASTEXITCODE
}

Write-Host "--- 5. Registering new TSF DLL..." -ForegroundColor Cyan
if (Test-Path "target\debug\rust-ime.exe") {
    & "target\debug\rust-ime.exe" --register
}

Write-Host "--- 6. Starting background process..." -ForegroundColor Cyan
if (Test-Path "target\debug\rust-ime.exe") {
    Start-Process "target\debug\rust-ime.exe" "--daemon"
}

Write-Host "========================================" -ForegroundColor Green
Write-Host "  Update Finished Successfully!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green
