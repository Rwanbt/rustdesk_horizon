## Test Amyuni v2 - more thorough display detection
$outFile = "D:\App\Fulldesk\test_amyuni_v2_result.txt"
"=== Amyuni Resolution Test v2 ===" | Out-File $outFile
"Date: $(Get-Date)" | Out-File $outFile -Append

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

public class DH2 {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct DEVMODEW {
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string dmDeviceName;
        public ushort dmSpecVersion;
        public ushort dmDriverVersion;
        public ushort dmSize;
        public ushort dmDriverExtra;
        public uint dmFields;
        public int dmPositionX;
        public int dmPositionY;
        public uint dmDisplayOrientation;
        public uint dmDisplayFixedOutput;
        public short dmColor;
        public short dmDuplex;
        public short dmYResolution;
        public short dmTTOption;
        public short dmCollate;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string dmFormName;
        public ushort dmLogPixels;
        public uint dmBitsPerPel;
        public uint dmPelsWidth;
        public uint dmPelsHeight;
        public uint dmDisplayFlags;
        public uint dmDisplayFrequency;
        public uint dmICMMethod;
        public uint dmICMIntent;
        public uint dmMediaType;
        public uint dmDitherType;
        public uint dmReserved1;
        public uint dmReserved2;
        public uint dmPanningWidth;
        public uint dmPanningHeight;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct DISPLAY_DEVICEW {
        public uint cb;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string DeviceName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string DeviceString;
        public uint StateFlags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string DeviceID;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string DeviceKey;
    }

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern bool EnumDisplayDevicesW(string lpDevice, uint iDevNum, ref DISPLAY_DEVICEW lpDisplayDevice, uint dwFlags);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern bool EnumDisplaySettingsW(string lpszDeviceName, int iModeNum, ref DEVMODEW lpDevMode);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int ChangeDisplaySettingsExW(string lpszDeviceName, ref DEVMODEW lpDevMode, IntPtr hwnd, uint dwflags, IntPtr lParam);

    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern IntPtr SetupDiGetClassDevs(ref Guid ClassGuid, IntPtr Enumerator, IntPtr hwndParent, uint Flags);

    [DllImport("setupapi.dll", SetLastError = true)]
    public static extern bool SetupDiEnumDeviceInterfaces(IntPtr DeviceInfoSet, IntPtr DeviceInfoData, ref Guid InterfaceClassGuid, uint MemberIndex, ref SP_DEVICE_INTERFACE_DATA DeviceInterfaceData);

    [DllImport("setupapi.dll", CharSet = CharSet.Auto, SetLastError = true)]
    public static extern bool SetupDiGetDeviceInterfaceDetail(IntPtr DeviceInfoSet, ref SP_DEVICE_INTERFACE_DATA DeviceInterfaceData, IntPtr DeviceInterfaceDetailData, uint DeviceInterfaceDetailDataSize, ref uint RequiredSize, IntPtr DeviceInfoData);

    [DllImport("setupapi.dll", SetLastError = true)]
    public static extern bool SetupDiDestroyDeviceInfoList(IntPtr DeviceInfoSet);

    [DllImport("kernel32.dll", CharSet = CharSet.Auto, SetLastError = true)]
    public static extern SafeFileHandle CreateFile(string lpFileName, uint dwDesiredAccess, uint dwShareMode, IntPtr lpSecurityAttributes, uint dwCreationDisposition, uint dwFlagsAndAttributes, IntPtr hTemplateFile);

    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool DeviceIoControl(SafeFileHandle hDevice, uint dwIoControlCode, byte[] lpInBuffer, uint nInBufferSize, IntPtr lpOutBuffer, uint nOutBufferSize, ref uint lpBytesReturned, IntPtr lpOverlapped);

    [StructLayout(LayoutKind.Sequential)]
    public struct SP_DEVICE_INTERFACE_DATA {
        public uint cbSize;
        public Guid InterfaceClassGuid;
        public uint Flags;
        public IntPtr Reserved;
    }

    public const uint DISPLAY_DEVICE_ACTIVE = 0x1;
    public const uint DIGCF_PRESENT = 0x2;
    public const uint DIGCF_DEVICEINTERFACE = 0x10;
    public const uint GENERIC_WRITE = 0x40000000;
    public const uint OPEN_EXISTING = 3;
    public const uint DM_PELSWIDTH = 0x80000;
    public const uint DM_PELSHEIGHT = 0x100000;
    public const uint CDS_UPDATEREGISTRY = 0x01;
    public const uint CDS_GLOBAL = 0x08;
    public const uint CDS_RESET = 0x40000000;
    public const int ENUM_CURRENT_SETTINGS = -1;
}
"@

function Log($msg) {
    $msg | Out-File $outFile -Append
    Write-Host $msg
}

function Get-AllDisplays {
    $displays = @()
    $dd = New-Object DH2+DISPLAY_DEVICEW
    $dd.cb = [System.Runtime.InteropServices.Marshal]::SizeOf($dd)
    $devNum = 0
    while ([DH2]::EnumDisplayDevicesW($null, $devNum, [ref]$dd, 0)) {
        $isActive = ($dd.StateFlags -band [DH2]::DISPLAY_DEVICE_ACTIVE) -ne 0
        $isAmyuni = $dd.DeviceString -like "USB Mobile Monitor*"
        $displays += @{
            Name=$dd.DeviceName
            String=$dd.DeviceString
            Active=$isActive
            Amyuni=$isAmyuni
            Flags=$dd.StateFlags
        }
        $devNum++
        $dd.cb = [System.Runtime.InteropServices.Marshal]::SizeOf($dd)
    }
    return $displays
}

function Get-DisplayModes($deviceName) {
    $modes = @()
    $dm = New-Object DH2+DEVMODEW
    $dm.dmSize = [System.Runtime.InteropServices.Marshal]::SizeOf($dm) -as [uint16]
    $modeNum = 0
    while ([DH2]::EnumDisplaySettingsW($deviceName, $modeNum, [ref]$dm)) {
        $res = "$($dm.dmPelsWidth)x$($dm.dmPelsHeight)"
        if ($modes -notcontains $res) { $modes += $res }
        $modeNum++
        $dm.dmSize = [System.Runtime.InteropServices.Marshal]::SizeOf($dm) -as [uint16]
    }
    return $modes
}

$interfaceGuid = [Guid]"b5ffd75f-da40-4353-8ff8-b6daf6f1d8ca"
$ioCtlCode = 2307084

function Invoke-AmyuniIoctl($cmd) {
    $DIGCF = [DH2]::DIGCF_PRESENT -bor [DH2]::DIGCF_DEVICEINTERFACE
    $devInfoSet = [DH2]::SetupDiGetClassDevs([ref]$interfaceGuid, [IntPtr]::Zero, [IntPtr]::Zero, $DIGCF)
    if ($devInfoSet -eq [IntPtr]::new(-1)) { Log "  FAIL: SetupDiGetClassDevs"; return $false }

    $ifd = New-Object DH2+SP_DEVICE_INTERFACE_DATA
    $ifd.cbSize = [System.Runtime.InteropServices.Marshal]::SizeOf($ifd) -as [uint32]
    if (-not [DH2]::SetupDiEnumDeviceInterfaces($devInfoSet, [IntPtr]::Zero, [ref]$interfaceGuid, 0, [ref]$ifd)) {
        Log "  FAIL: SetupDiEnumDeviceInterfaces err=$([System.Runtime.InteropServices.Marshal]::GetLastWin32Error())"
        [DH2]::SetupDiDestroyDeviceInfoList($devInfoSet) | Out-Null; return $false
    }

    [uint32]$reqSize = 0
    [DH2]::SetupDiGetDeviceInterfaceDetail($devInfoSet, [ref]$ifd, [IntPtr]::Zero, 0, [ref]$reqSize, [IntPtr]::Zero) | Out-Null
    $buf = [System.Runtime.InteropServices.Marshal]::AllocHGlobal($reqSize)
    [System.Runtime.InteropServices.Marshal]::WriteInt32($buf, $(if ([IntPtr]::Size -eq 8) {8} else {6}))
    if (-not [DH2]::SetupDiGetDeviceInterfaceDetail($devInfoSet, [ref]$ifd, $buf, $reqSize, [ref]$reqSize, [IntPtr]::Zero)) {
        [System.Runtime.InteropServices.Marshal]::FreeHGlobal($buf)
        [DH2]::SetupDiDestroyDeviceInfoList($devInfoSet) | Out-Null; return $false
    }
    $devPath = [System.Runtime.InteropServices.Marshal]::PtrToStringUni([IntPtr]::Add($buf, 4))
    [System.Runtime.InteropServices.Marshal]::FreeHGlobal($buf)
    [DH2]::SetupDiDestroyDeviceInfoList($devInfoSet) | Out-Null

    $handle = [DH2]::CreateFile($devPath, [DH2]::GENERIC_WRITE, 0, [IntPtr]::Zero, [DH2]::OPEN_EXISTING, 0, [IntPtr]::Zero)
    if ($handle.IsInvalid) { Log "  FAIL: CreateFile err=$([System.Runtime.InteropServices.Marshal]::GetLastWin32Error())"; return $false }

    [byte[]]$cmdBytes = @($cmd, 0, 0, 0)
    [uint32]$br = 0
    $ok = [DH2]::DeviceIoControl($handle, $ioCtlCode, $cmdBytes, 4, [IntPtr]::Zero, 0, [ref]$br, [IntPtr]::Zero)
    $handle.Close()
    return $ok
}

# ============================================================
# STEP 1: Snapshot all displays BEFORE
# ============================================================
Log "=== STEP 1: All displays BEFORE plug-in ==="
$before = Get-AllDisplays
foreach ($d in $before) {
    $status = if ($d.Active) {"ACTIVE"} else {"inactive"}
    $tag = if ($d.Amyuni) {" [AMYUNI]"} else {""}
    Log "  $($d.Name) | $($d.String) | $status$tag | flags=0x$($d.Flags.ToString('X'))"
}
$amyuniBefore = ($before | Where-Object { $_.Amyuni -and $_.Active }).Count
Log "  Amyuni active count: $amyuniBefore"

# ============================================================
# STEP 2: Verify registry has 2340,1080
# ============================================================
Log ""
Log "=== STEP 2: Verify registry ==="
$regPath = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\WUDF\Services\usbmmIdd\Parameters\Monitors"
$has2340reg = $false
$regProps = Get-ItemProperty $regPath
for ($i = 0; $i -lt 20; $i++) {
    $val = $regProps."$i"
    if ($val) {
        Log "  [$i] = $val"
        if ($val -eq "2340,1080") { $has2340reg = $true }
    } else { break }
}
if ($has2340reg) { Log "  >> 2340,1080 present in registry" }
else {
    Log "  >> Adding 2340,1080..."
    for ($i = 0; $i -lt 20; $i++) {
        if (-not $regProps."$i") {
            Set-ItemProperty -Path $regPath -Name "$i" -Value "2340,1080" -Type String
            Log "  >> Added at index $i"
            break
        }
    }
}

# ============================================================
# STEP 3: Plug in a virtual display
# ============================================================
Log ""
Log "=== STEP 3: Plug in VD ==="
$ok = Invoke-AmyuniIoctl 0x10
if ($ok) { Log "  DeviceIoControl plug-in OK" }
else { Log "  DeviceIoControl plug-in FAILED"; exit }

# ============================================================
# STEP 4: Poll for new display (up to 10 seconds)
# ============================================================
Log ""
Log "=== STEP 4: Waiting for display to appear... ==="
$found = $false
$amyuniDisplay = $null
for ($t = 1; $t -le 20; $t++) {
    Start-Sleep -Milliseconds 500
    $current = Get-AllDisplays
    $amyuniNow = $current | Where-Object { $_.Amyuni }
    $amyuniActiveNow = $amyuniNow | Where-Object { $_.Active }

    if ($t % 4 -eq 0) {
        $elapsed = $t * 0.5
        Log "  ${elapsed}s: total=$($current.Count) amyuni_all=$($amyuniNow.Count) amyuni_active=$($amyuniActiveNow.Count)"
        foreach ($d in $amyuniNow) {
            $s = if ($d.Active) {"ACTIVE"} else {"inactive"}
            Log "    $($d.Name) | $s | flags=0x$($d.Flags.ToString('X'))"
        }
    }

    if ($amyuniActiveNow.Count -gt $amyuniBefore) {
        $amyuniDisplay = $amyuniActiveNow | Select-Object -Last 1
        Log "  >> NEW DISPLAY DETECTED at ${t}x0.5s = $($t*0.5)s: $($amyuniDisplay.Name)"
        $found = $true
        break
    }
}

if (-not $found) {
    Log "  >> No new active Amyuni display after 10s"
    Log ""
    Log "=== Checking ALL displays (including inactive) ==="
    $allNow = Get-AllDisplays
    foreach ($d in $allNow) {
        $s = if ($d.Active) {"ACTIVE"} else {"inactive"}
        $tag = if ($d.Amyuni) {" [AMYUNI]"} else {""}
        Log "  $($d.Name) | $($d.String) | $s$tag | flags=0x$($d.Flags.ToString('X'))"
    }

    # Try checking if there are any Amyuni at all (even inactive)
    $anyAmyuni = $allNow | Where-Object { $_.Amyuni }
    if ($anyAmyuni.Count -gt 0) {
        Log ""
        Log "  Found $($anyAmyuni.Count) Amyuni display(s) (possibly inactive). Trying to enumerate modes on first one..."
        $amyuniDisplay = $anyAmyuni[0]
        $found = $true  # try to enumerate modes anyway
    }
}

# ============================================================
# STEP 5: Enumerate modes
# ============================================================
if ($found -and $amyuniDisplay) {
    Log ""
    Log "=== STEP 5: Enumerate modes for $($amyuniDisplay.Name) ==="
    $modes = Get-DisplayModes $amyuniDisplay.Name
    if ($modes.Count -eq 0) {
        Log "  No modes returned (display may be inactive)"
    } else {
        foreach ($m in ($modes | Sort-Object)) { Log "  $m" }
        Log "  Total unique modes: $($modes.Count)"

        $has2340 = $modes | Where-Object { $_ -eq "2340x1080" }
        if ($has2340) {
            Log "  >> SUCCESS: 2340x1080 IS in mode list!"

            # STEP 6: Try ChangeDisplaySettingsEx
            Log ""
            Log "=== STEP 6: ChangeDisplaySettingsEx 2340x1080 ==="
            $dm = New-Object DH2+DEVMODEW
            $dm.dmSize = [System.Runtime.InteropServices.Marshal]::SizeOf($dm) -as [uint16]
            $dm.dmPelsWidth = 2340
            $dm.dmPelsHeight = 1080
            $dm.dmFields = [DH2]::DM_PELSWIDTH -bor [DH2]::DM_PELSHEIGHT
            $result = [DH2]::ChangeDisplaySettingsExW($amyuniDisplay.Name, [ref]$dm, [IntPtr]::Zero,
                ([DH2]::CDS_UPDATEREGISTRY -bor [DH2]::CDS_GLOBAL -bor [DH2]::CDS_RESET), [IntPtr]::Zero)
            switch ($result) {
                0  { Log "  >> RESOLUTION APPLIED SUCCESSFULLY!" }
                -2 { Log "  >> BADMODE: driver does not support this resolution" }
                default { Log "  >> Result code: $result" }
            }

            # Verify
            $dmV = New-Object DH2+DEVMODEW
            $dmV.dmSize = [System.Runtime.InteropServices.Marshal]::SizeOf($dmV) -as [uint16]
            if ([DH2]::EnumDisplaySettingsW($amyuniDisplay.Name, [DH2]::ENUM_CURRENT_SETTINGS, [ref]$dmV)) {
                Log "  Current: $($dmV.dmPelsWidth)x$($dmV.dmPelsHeight)"
            }
        } else {
            Log "  >> FAIL: 2340x1080 NOT in mode list"
            Log "  >> Driver did NOT pick up the new registry entry"
        }
    }
} else {
    Log ""
    Log "=== STEP 5-6: SKIPPED (no display found) ==="
}

# ============================================================
# CLEANUP
# ============================================================
Log ""
Log "=== CLEANUP ==="
Invoke-AmyuniIoctl 0x00 | Out-Null
Log "  Plugged out."

# Check RustDesk service status
Log ""
Log "=== EXTRA: RustDesk service status ==="
$svc = Get-Service -Name "RustDesk" -ErrorAction SilentlyContinue
if ($svc) { Log "  RustDesk service: $($svc.Status)" }
else { Log "  RustDesk service: NOT FOUND" }

# Check if RustDesk process is running
$proc = Get-Process -Name "rustdesk" -ErrorAction SilentlyContinue
if ($proc) { Log "  RustDesk process: RUNNING (PID $($proc.Id))" }
else { Log "  RustDesk process: NOT RUNNING" }

Log ""
Log "=== DONE ==="
