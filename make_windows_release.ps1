# 1. 确保在发布模式下编译
Write-Host "正在编译发布版本..." -ForegroundColor Cyan
cargo build --release

# 2. 创建发布目录
$ReleaseDir = "rust-ime-windows-v0.1.0"
if (Test-Path $ReleaseDir) { Remove-Item -Recurse -Force $ReleaseDir }
New-Item -ItemType Directory $ReleaseDir

# 3. 复制二进制文件
Write-Host "正在收集文件..." -ForegroundColor Green
Copy-Item "target/release/rust-ime.exe" $ReleaseDir
Copy-Item "target/release/rust_ime_tsf_v3.dll" $ReleaseDir

# 4. 复制资源文件
if (Test-Path "dicts") { Copy-Item -Recurse "dicts" $ReleaseDir }
if (Test-Path "static") { Copy-Item -Recurse "static" $ReleaseDir }

# 5. 直接写入 bat 文件
Add-Content -Path "$ReleaseDir\install.bat" -Value "@echo off"
Add-Content -Path "$ReleaseDir\install.bat" -Value "cd /d %~dp0"
Add-Content -Path "$ReleaseDir\install.bat" -Value "rust-ime.exe --register"
$RegValue = 'reg add HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v RustIme /t REG_SZ /d """%~dp0rust-ime.exe"" --daemon" /f'
Add-Content -Path "$ReleaseDir\install.bat" -Value $RegValue
Add-Content -Path "$ReleaseDir\install.bat" -Value 'start "" "rust-ime.exe" --daemon'
Add-Content -Path "$ReleaseDir\install.bat" -Value "echo Done!"
Add-Content -Path "$ReleaseDir\install.bat" -Value "pause"

Add-Content -Path "$ReleaseDir\uninstall.bat" -Value "@echo off"
Add-Content -Path "$ReleaseDir\uninstall.bat" -Value "taskkill /F /IM rust-ime.exe /T"
Add-Content -Path "$ReleaseDir\uninstall.bat" -Value "rust-ime.exe --unregister"
Add-Content -Path "$ReleaseDir\uninstall.bat" -Value "reg delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v RustIme /f"
Add-Content -Path "$ReleaseDir\uninstall.bat" -Value "echo Uninstalled!"
Add-Content -Path "$ReleaseDir\uninstall.bat" -Value "pause"

# 6. 压缩成 ZIP
$ZipFile = "$ReleaseDir.zip"
if (Test-Path $ZipFile) { Remove-Item $ZipFile }
Write-Host "正在压缩..." -ForegroundColor Cyan
Compress-Archive -Path "$ReleaseDir\*" -DestinationPath $ZipFile

Write-Host "打包完成！请查看目录: $ReleaseDir 和压缩包: $ZipFile" -ForegroundColor Yellow
