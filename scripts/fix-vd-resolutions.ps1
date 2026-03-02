#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Replaces the Amyuni IDD virtual display resolution list with useful device resolutions.

.DESCRIPTION
    The Amyuni IDD driver reads a maximum of 10 resolution entries (indices 0-9)
    from the registry. This script replaces those 10 entries with the most useful
    resolutions, then restarts the driver so it picks up the changes.

.NOTES
    Must be run as Administrator.
#>

$regPath = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\WUDF\Services\usbmmIdd\Parameters\Monitors"

# Best 10 resolutions: common monitors + iPad Pro + phones
$resolutions = @(
    "1920,1080"   # 0: Full HD (most common)
    "2560,1440"   # 1: QHD
    "3840,2160"   # 2: 4K UHD
    "2732,2048"   # 3: iPad Pro 12.9" (Gen 3/4/5/6)
    "2388,1668"   # 4: iPad Pro 11" / iPad Air
    "2340,1080"   # 5: Samsung Galaxy S24 / Pixel 8
    "1920,1200"   # 6: WUXGA / Fire HD 10
    "2560,1600"   # 7: Steam Deck OLED / Galaxy Tab S9+
    "1280,720"    # 8: HD 720p
    "3440,1440"   # 9: UltraWide QHD
)

if (-not (Test-Path $regPath)) {
    Write-Host "ERROR: Amyuni IDD registry key not found." -ForegroundColor Red
    exit 1
}

Write-Host "Amyuni IDD - Resolution Replacement" -ForegroundColor Cyan
Write-Host "====================================" -ForegroundColor Cyan
Write-Host ""

# Show old entries
Write-Host "OLD resolutions:" -ForegroundColor DarkGray
for ($i = 0; $i -lt 20; $i++) {
    try {
        $val = (Get-ItemProperty -Path $regPath -Name "$i" -ErrorAction Stop)."$i"
        $dims = $val -split ","
        Write-Host "  [$i] $($dims[0])x$($dims[1])" -ForegroundColor DarkGray
    } catch { break }
}

# Remove all existing numbered entries (0-99)
Write-Host ""
Write-Host "Clearing old entries..." -ForegroundColor Yellow
for ($i = 0; $i -lt 100; $i++) {
    try {
        Remove-ItemProperty -Path $regPath -Name "$i" -ErrorAction Stop
    } catch { break }
}

# Write new entries
Write-Host "Writing new resolutions:" -ForegroundColor Green
for ($i = 0; $i -lt $resolutions.Count; $i++) {
    Set-ItemProperty -Path $regPath -Name "$i" -Value $resolutions[$i] -Type String
    $dims = $resolutions[$i] -split ","
    Write-Host "  [$i] $($dims[0])x$($dims[1])" -ForegroundColor Green
}

# Also set default value
Set-ItemProperty -Path $regPath -Name "(default)" -Value "1920,1080" -Type String

Write-Host ""
Write-Host "Registry updated. Restarting Amyuni driver..." -ForegroundColor Yellow

# Restart the Amyuni device
$devices = Get-PnpDevice -FriendlyName "*USB Mobile Monitor*" -ErrorAction SilentlyContinue
if ($devices) {
    foreach ($dev in $devices) {
        Write-Host "  Disabling: $($dev.FriendlyName) ($($dev.InstanceId))..." -ForegroundColor DarkYellow
        Disable-PnpDevice -InstanceId $dev.InstanceId -Confirm:$false -ErrorAction SilentlyContinue
    }
    Start-Sleep -Seconds 2
    foreach ($dev in $devices) {
        Write-Host "  Re-enabling: $($dev.FriendlyName)..." -ForegroundColor DarkYellow
        Enable-PnpDevice -InstanceId $dev.InstanceId -Confirm:$false -ErrorAction SilentlyContinue
    }
    Start-Sleep -Seconds 2
    Write-Host ""
    Write-Host "Driver restarted." -ForegroundColor Green
} else {
    Write-Host "  Could not find Amyuni device. Try manually:" -ForegroundColor Yellow
    Write-Host "    1. Device Manager > Display Adapters" -ForegroundColor White
    Write-Host "    2. Right-click 'USB Mobile Monitor Virtual Display'" -ForegroundColor White
    Write-Host "    3. Disable, wait 2s, then Enable" -ForegroundColor White
}

Write-Host ""
Write-Host "DONE! Next steps:" -ForegroundColor Cyan
Write-Host "  1. Disconnect the virtual display in Fulldesk" -ForegroundColor White
Write-Host "  2. Reconnect it" -ForegroundColor White
Write-Host "  3. Right-click Desktop > Display settings" -ForegroundColor White
Write-Host "  4. Select the virtual display and choose your resolution" -ForegroundColor White
Write-Host ""
Write-Host "iPad Pro 12.9 (2732x2048) should now be available!" -ForegroundColor Green
