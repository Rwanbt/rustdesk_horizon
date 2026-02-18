#Requires -RunAsAdministrator

# 1. Stop the Windows service (runs as LocalService, handles connections)
Write-Host "Stopping RustDesk service..."
$svc = Get-Service -Name "RustDesk" -ErrorAction SilentlyContinue
if ($svc -and $svc.Status -eq 'Running') {
    Stop-Service -Name "RustDesk" -Force
    Write-Host "  Service stopped."
} else {
    Write-Host "  Service not running (status: $($svc.Status))."
}

# 2. Kill ALL remaining rustdesk processes (UI, --cm, tray, stale --server)
Write-Host "Killing all rustdesk processes..."
Get-Process -Name rustdesk -ErrorAction SilentlyContinue | ForEach-Object {
    Write-Host "  Killing PID $($_.Id) ($($_.Path))"
    Stop-Process -Id $_.Id -Force
}
Start-Sleep -Seconds 2

# Verify no process left
$remaining = Get-Process -Name rustdesk -ErrorAction SilentlyContinue
if ($remaining) {
    Write-Host "WARNING: $($remaining.Count) process(es) still alive!" -ForegroundColor Red
    $remaining | ForEach-Object { Write-Host "  PID $($_.Id)" }
} else {
    Write-Host "  All processes stopped."
}

# 3. Copy new DLL
Write-Host "Copying new librustdesk.dll..."
Copy-Item "d:\App\Fulldesk\target\release\librustdesk.dll" "C:\Program Files\RustDesk\librustdesk.dll" -Force
$newSize = (Get-Item 'C:\Program Files\RustDesk\librustdesk.dll').Length
$srcSize = (Get-Item 'd:\App\Fulldesk\target\release\librustdesk.dll').Length
if ($newSize -eq $srcSize) {
    Write-Host "  OK: $newSize bytes (matches source)." -ForegroundColor Green
} else {
    Write-Host "  ERROR: dest=$newSize vs src=$srcSize" -ForegroundColor Red
}

# 4. Start the Windows service (this is what makes is_self_service_running() = true)
Write-Host "Starting RustDesk service..."
if ($svc) {
    Start-Service -Name "RustDesk"
    Start-Sleep -Seconds 2
    $svc2 = Get-Service -Name "RustDesk"
    Write-Host "  Service status: $($svc2.Status)" -ForegroundColor $(if ($svc2.Status -eq 'Running') { 'Green' } else { 'Red' })
} else {
    Write-Host "  No 'RustDesk' service found. Starting as regular process..." -ForegroundColor Yellow
    Start-Process "C:\Program Files\RustDesk\rustdesk.exe"
}

# 5. Launch the UI (tray icon)
Write-Host "Launching RustDesk UI..."
Start-Process "C:\Program Files\RustDesk\rustdesk.exe" -ArgumentList "--tray"
Write-Host "Done."
