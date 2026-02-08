#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Build and install Fulldesk (RustDesk fork) with Virtual Display Extension.
.DESCRIPTION
    Builds the Rust library and Flutter UI, then installs to Program Files,
    (re)creates the Windows service, and launches the application.
#>

$ErrorActionPreference = "Stop"

$ProjectRoot = $PSScriptRoot
$InstallDir = "C:\Program Files\RustDesk"
$ServiceName = "RustDesk"
$AppExe = "rustdesk.exe"

# Environment
$env:VCPKG_ROOT = "C:\vcpkg"
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"

Write-Host "=== Fulldesk Build & Install ===" -ForegroundColor Cyan

# Step 1: Build Rust library
Write-Host "`n[1/5] Building Rust library (release)..." -ForegroundColor Yellow
Push-Location $ProjectRoot
try {
    cargo build --release --features flutter
    if ($LASTEXITCODE -ne 0) { throw "Rust build failed" }
} finally {
    Pop-Location
}
Write-Host "  Rust build OK" -ForegroundColor Green

# Step 2: Build Flutter UI
Write-Host "`n[2/5] Building Flutter UI..." -ForegroundColor Yellow
Push-Location "$ProjectRoot\flutter"
try {
    flutter build windows
    if ($LASTEXITCODE -ne 0) { throw "Flutter build failed" }
} finally {
    Pop-Location
}
Write-Host "  Flutter build OK" -ForegroundColor Green

# Verify artifacts exist
$FlutterBuildDir = "$ProjectRoot\flutter\build\windows\x64\runner\Release"
if (-not (Test-Path "$FlutterBuildDir\$AppExe")) {
    # Try alternate path
    $FlutterBuildDir = "$ProjectRoot\flutter\build\windows\runner\Release"
    if (-not (Test-Path "$FlutterBuildDir\$AppExe")) {
        throw "Cannot find built $AppExe in flutter build output"
    }
}
$RustDll = "$ProjectRoot\target\release\librustdesk.dll"
if (-not (Test-Path $RustDll)) {
    throw "Cannot find librustdesk.dll in target\release"
}

Write-Host "`n[3/5] Stopping service and existing processes..." -ForegroundColor Yellow
# Stop service if it exists
$svc = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($svc) {
    if ($svc.Status -eq "Running") {
        Stop-Service -Name $ServiceName -Force
        Start-Sleep -Seconds 2
    }
    sc.exe delete $ServiceName | Out-Null
    Start-Sleep -Seconds 1
}
# Kill any running instances
Get-Process -Name "rustdesk" -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1

Write-Host "  Stopped" -ForegroundColor Green

# Step 4: Install files
Write-Host "`n[4/5] Installing to $InstallDir..." -ForegroundColor Yellow
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

# Copy Flutter build output (exe + dlls + data folder)
Copy-Item -Path "$FlutterBuildDir\*" -Destination $InstallDir -Recurse -Force

# Copy Rust library (overwrite the one from Flutter build if present)
Copy-Item -Path $RustDll -Destination $InstallDir -Force

Write-Host "  Files installed" -ForegroundColor Green

# Step 5: Create and start service
Write-Host "`n[5/5] Creating and starting service..." -ForegroundColor Yellow
$exePath = "$InstallDir\$AppExe"
$binPath = "`"$exePath`" --service"
New-Service -Name $ServiceName -BinaryPathName $binPath -DisplayName 'RustDesk Service' -StartupType Automatic -ErrorAction SilentlyContinue
Start-Sleep -Seconds 1
Start-Service -Name $ServiceName -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2
$svc = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($svc -and $svc.Status -eq "Running") {
    Write-Host "  Service running" -ForegroundColor Green
} else {
    Write-Host "  WARNING: Service created but not running (Status: $($svc.Status))" -ForegroundColor Red
}

# Launch the application
Write-Host "`nLaunching RustDesk..." -ForegroundColor Cyan
Start-Process -FilePath $exePath
Write-Host "Done!" -ForegroundColor Green
