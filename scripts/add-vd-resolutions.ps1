#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Adds common device resolutions to the Amyuni IDD virtual display driver.

.DESCRIPTION
    The Amyuni IDD driver reads supported resolutions from the Windows registry.
    This script adds resolutions for popular tablets, phones, and monitors
    without duplicating existing entries.

    After running this script:
    1. Disconnect the virtual display (if connected)
    2. Reconnect the virtual display
    3. Open Windows Settings > Display > select the virtual display
    4. The new resolutions will be available in the resolution dropdown

.NOTES
    Must be run as Administrator.
    Registry key: HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\WUDF\Services\usbmmIdd\Parameters\Monitors
#>

$regPath = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\WUDF\Services\usbmmIdd\Parameters\Monitors"

# ── Device resolutions to add ────────────────────────────────────────────────
# Format: "Width,Height"  # Device / standard name
$resolutions = @(
    # ── Tablets ──
    "2732,2048"   # iPad Pro 12.9" (Gen 3/4/5/6)
    "2388,1668"   # iPad Pro 11" (Gen 1/2/3/4) / iPad Air (Gen 4/5)
    "2360,1640"   # iPad (Gen 10) / iPad Air 11" M2
    "2266,1488"   # iPad mini (Gen 6)
    "2160,1620"   # iPad (Gen 7/8/9)
    "2048,1536"   # iPad (Gen 5/6) / iPad mini (Gen 4/5)
    "2560,1600"   # Samsung Galaxy Tab S9+ / Huawei MatePad Pro
    "2800,1752"   # Samsung Galaxy Tab S9 Ultra
    "2000,1200"   # Samsung Galaxy Tab S9 / Xiaomi Pad 6
    "1920,1200"   # Samsung Galaxy Tab A8 / Fire HD 10
    "2560,1600"   # Lenovo Tab P12 Pro

    # ── Smartphones (landscape) ──
    "2796,1290"   # iPhone 15 Pro Max / iPhone 14 Pro Max
    "2556,1179"   # iPhone 15 Pro / iPhone 14 Pro
    "2532,1170"   # iPhone 15 / iPhone 14 / iPhone 13
    "2340,1080"   # Samsung Galaxy S24 / Xiaomi 14 / Pixel 8
    "3088,1440"   # Samsung Galaxy S24 Ultra
    "3120,1440"   # Samsung Galaxy S23 Ultra / OnePlus 12
    "2400,1080"   # Samsung Galaxy A54 / Redmi Note 13 Pro

    # ── Common monitors ──
    "1920,1080"   # Full HD (1080p)
    "2560,1440"   # QHD (1440p)
    "3840,2160"   # 4K UHD
    "1280,720"    # HD (720p)
    "1366,768"    # HD+ (common laptop)
    "1600,900"    # HD+ (common laptop)
    "1680,1050"   # WSXGA+
    "1920,1200"   # WUXGA
    "2560,1080"   # UltraWide FHD
    "3440,1440"   # UltraWide QHD
    "5120,2880"   # 5K

    # ── Portable monitors / Steam Deck ──
    "1280,800"    # Steam Deck (LCD)
    "1280,720"    # Nintendo Switch (docked)
    "2560,1600"   # Steam Deck OLED
)

# ── Check registry key exists ────────────────────────────────────────────────
if (-not (Test-Path $regPath)) {
    Write-Host "ERROR: Registry key not found:" -ForegroundColor Red
    Write-Host "  $regPath" -ForegroundColor Red
    Write-Host ""
    Write-Host "The Amyuni IDD driver may not be installed." -ForegroundColor Yellow
    Write-Host "Install it via Fulldesk first, then re-run this script." -ForegroundColor Yellow
    exit 1
}

# ── Read existing entries ────────────────────────────────────────────────────
$existing = @{}
$maxIndex = -1

for ($i = 0; $i -lt 100; $i++) {
    try {
        $val = (Get-ItemProperty -Path $regPath -Name "$i" -ErrorAction Stop)."$i"
        $existing[$val] = $i
        if ($i -gt $maxIndex) { $maxIndex = $i }
    } catch {
        break
    }
}

Write-Host "Amyuni IDD Virtual Display - Resolution Manager" -ForegroundColor Cyan
Write-Host "================================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Registry: $regPath" -ForegroundColor DarkGray
Write-Host "Existing entries: $($maxIndex + 1)" -ForegroundColor DarkGray
Write-Host ""

# ── Add missing resolutions ──────────────────────────────────────────────────
$added = 0
$skipped = 0
$nextIndex = $maxIndex + 1

foreach ($res in ($resolutions | Sort-Object -Unique)) {
    if ($existing.ContainsKey($res)) {
        $skipped++
        continue
    }

    $dims = $res -split ","
    $w = $dims[0]; $h = $dims[1]

    Set-ItemProperty -Path $regPath -Name "$nextIndex" -Value $res -Type String
    Write-Host "  [+] $($nextIndex): ${w}x${h}" -ForegroundColor Green
    $nextIndex++
    $added++
}

Write-Host ""
if ($added -gt 0) {
    Write-Host "Added $added new resolution(s). Skipped $skipped already present." -ForegroundColor Green
    Write-Host ""
    Write-Host "NEXT STEPS:" -ForegroundColor Yellow
    Write-Host "  1. Disconnect the virtual display in Fulldesk" -ForegroundColor White
    Write-Host "  2. Reconnect the virtual display" -ForegroundColor White
    Write-Host "  3. Right-click Desktop > Display settings" -ForegroundColor White
    Write-Host "  4. Select the virtual display, then choose your resolution" -ForegroundColor White
} else {
    Write-Host "All $skipped resolution(s) already present. Nothing to add." -ForegroundColor Cyan
}

Write-Host ""
Write-Host "Current resolution list:" -ForegroundColor Cyan
for ($i = 0; $i -lt $nextIndex; $i++) {
    try {
        $val = (Get-ItemProperty -Path $regPath -Name "$i" -ErrorAction Stop)."$i"
        $dims = $val -split ","
        Write-Host "  [$i] $($dims[0])x$($dims[1])" -ForegroundColor White
    } catch {
        break
    }
}
