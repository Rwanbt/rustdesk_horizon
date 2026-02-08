#Requires -RunAsAdministrator
$ErrorActionPreference = "Stop"

$ProjectRoot = "D:\App\Fulldesk"
$InstallDir = "C:\Program Files\RustDesk"
$ServiceName = "RustDesk"
$AppExe = "rustdesk.exe"

Write-Host "=== Install Only ===" -ForegroundColor Cyan

# Find build output
$FlutterBuildDir = "$ProjectRoot\flutter\build\windows\x64\runner\Release"
if (-not (Test-Path "$FlutterBuildDir\$AppExe")) {
    $FlutterBuildDir = "$ProjectRoot\flutter\build\windows\runner\Release"
}
$RustDll = "$ProjectRoot\target\release\librustdesk.dll"

Write-Host "Flutter: $FlutterBuildDir\$AppExe"
Write-Host "Rust DLL: $RustDll"

# Stop service and processes
Write-Host "`nStopping service and processes..." -ForegroundColor Yellow
$svc = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($svc) {
    if ($svc.Status -eq "Running") {
        Stop-Service -Name $ServiceName -Force
        Start-Sleep -Seconds 2
    }
    sc.exe delete $ServiceName | Out-Null
    Start-Sleep -Seconds 1
}
Get-Process -Name "rustdesk" -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1

# Install files
Write-Host "Installing to $InstallDir..." -ForegroundColor Yellow
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}
Copy-Item -Path "$FlutterBuildDir\*" -Destination $InstallDir -Recurse -Force
Copy-Item -Path $RustDll -Destination $InstallDir -Force
Write-Host "Files installed" -ForegroundColor Green

# Create and start service
Write-Host "Creating service..." -ForegroundColor Yellow
$exePath = "$InstallDir\$AppExe"
$binPath = "`"$exePath`" --service"
New-Service -Name $ServiceName -BinaryPathName $binPath -DisplayName 'RustDesk Service' -StartupType Automatic -ErrorAction SilentlyContinue
Start-Sleep -Seconds 1
Start-Service -Name $ServiceName
Start-Sleep -Seconds 2

$svc = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
Write-Host "Service status: $($svc.Status)" -ForegroundColor $(if ($svc.Status -eq "Running") {"Green"} else {"Red"})

# Launch
Write-Host "Launching RustDesk..." -ForegroundColor Cyan
Start-Process -FilePath $exePath
Write-Host "Done!" -ForegroundColor Green

Read-Host "Press Enter to close"
