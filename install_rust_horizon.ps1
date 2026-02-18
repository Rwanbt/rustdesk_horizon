#Requires -RunAsAdministrator
$ErrorActionPreference = "Stop"
$ReleaseDir = "D:\App\Fulldesk\flutter\build\windows\x64\runner\Release"
$InstallDir = "C:\Program Files\Rust Horizon"
$ServiceName = "Rust Horizon"

Write-Host "=== Installing Rust Horizon ===" -ForegroundColor Cyan

# Stop and kill everything
Get-Process -Name "rust_horizon","rustdesk" -ErrorAction SilentlyContinue | Stop-Process -Force
$svc = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($svc) {
    Stop-Service -Name $ServiceName -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
    & sc.exe delete $ServiceName
    Start-Sleep -Seconds 1
}

# Also clean old RustDesk service/install
$oldSvc = Get-Service -Name "RustDesk" -ErrorAction SilentlyContinue
if ($oldSvc) {
    Stop-Service -Name "RustDesk" -Force -ErrorAction SilentlyContinue
    & sc.exe delete "RustDesk"
}

# Copy files
Write-Host "Copying files..." -ForegroundColor Yellow
if (!(Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}
& robocopy $ReleaseDir $InstallDir /E /MIR /NJH /NJS /NP /NFL /NDL
Write-Host "Files copied" -ForegroundColor Green

# Create and start service
Write-Host "Creating service..." -ForegroundColor Yellow
& sc.exe create $ServiceName binpath= "`"$InstallDir\rust_horizon.exe`" --service" start= auto DisplayName= "Rust Horizon Service"
Start-Sleep -Seconds 1
& sc.exe start $ServiceName
Start-Sleep -Seconds 2

$svc = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($svc -and $svc.Status -eq 'Running') {
    Write-Host "Service running!" -ForegroundColor Green
} else {
    Write-Host "WARNING: Service status: $($svc.Status)" -ForegroundColor Yellow
}

# Launch
Write-Host "Launching..." -ForegroundColor Yellow
Start-Process "$InstallDir\rust_horizon.exe"
Write-Host "=== Done ===" -ForegroundColor Cyan
