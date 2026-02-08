#Requires -RunAsAdministrator

$exePath = 'C:\Program Files\RustDesk\rustdesk.exe'
$serviceName = 'RustDesk'

# Remove existing service if any
$existing = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
if ($existing) {
    Stop-Service -Name $serviceName -Force -ErrorAction SilentlyContinue
    sc.exe delete $serviceName
    Start-Sleep -Seconds 2
}

# Create service using New-Service
$binPath = "`"$exePath`" --service"
Write-Host "Creating service with binPath: $binPath"
New-Service -Name $serviceName -BinaryPathName $binPath -DisplayName 'RustDesk Service' -StartupType Automatic -Description 'RustDesk remote desktop service'
Start-Sleep -Seconds 1

# Start the service
Start-Service -Name $serviceName
Start-Sleep -Seconds 2

# Report status
$svc = Get-Service -Name $serviceName
Write-Host "Service status: $($svc.Status)"

Read-Host 'Press Enter to close'
