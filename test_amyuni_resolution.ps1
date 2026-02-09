## Test Amyuni custom resolution injection
## Run as Administrator!
$outFile = "D:\App\Fulldesk\test_amyuni_result.txt"
"=== Amyuni Resolution Test ===" | Out-File $outFile
"Date: $(Get-Date)" | Out-File $outFile -Append

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

public class DisplayHelper {
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

    // For DeviceIoControl (plug in/out Amyuni virtual display)
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
    public const uint DISPLAY_DEVICE_MIRRORING_DRIVER = 0x8;
    public const int ENUM_CURRENT_SETTINGS = -1;
    public const uint DM_PELSWIDTH = 0x80000;
    public const uint DM_PELSHEIGHT = 0x100000;
    public const uint CDS_UPDATEREGISTRY = 0x01;
    public const uint CDS_GLOBAL = 0x08;
    public const uint CDS_RESET = 0x40000000;
    public const uint DIGCF_PRESENT = 0x2;
    public const uint DIGCF_DEVICEINTERFACE = 0x10;
    public const uint GENERIC_WRITE = 0x40000000;
    public const uint OPEN_EXISTING = 3;
}
"@

function Log($msg) {
    $msg | Out-File $outFile -Append
    Write-Host $msg
}

# ============================================================
# STEP 1: Find Amyuni virtual displays and enumerate modes
# ============================================================
Log ""
Log "=== STEP 1: Find Amyuni displays and enumerate modes ==="

$amyuniDisplays = @()
$dd = New-Object DisplayHelper+DISPLAY_DEVICEW
$dd.cb = [System.Runtime.InteropServices.Marshal]::SizeOf($dd)
$devNum = 0

while ([DisplayHelper]::EnumDisplayDevicesW($null, $devNum, [ref]$dd, 0)) {
    $devNum++
    if ($dd.DeviceString -like "USB Mobile Monitor*") {
        if ($dd.StateFlags -band [DisplayHelper]::DISPLAY_DEVICE_ACTIVE) {
            $amyuniDisplays += @{Name=$dd.DeviceName; String=$dd.DeviceString; Flags=$dd.StateFlags}
            Log "  Found ACTIVE Amyuni display: $($dd.DeviceName) [$($dd.DeviceString)]"
        } else {
            Log "  Found INACTIVE Amyuni display: $($dd.DeviceName) [$($dd.DeviceString)]"
        }
    }
    $dd.cb = [System.Runtime.InteropServices.Marshal]::SizeOf($dd)
}

Log "Total Amyuni active displays: $($amyuniDisplays.Count)"

# Enumerate modes for each Amyuni display
foreach ($disp in $amyuniDisplays) {
    Log ""
    Log "  Modes for $($disp.Name):"
    $dm = New-Object DisplayHelper+DEVMODEW
    $dm.dmSize = [System.Runtime.InteropServices.Marshal]::SizeOf($dm) -as [uint16]
    $modeNum = 0
    $modes = @()
    while ([DisplayHelper]::EnumDisplaySettingsW($disp.Name, $modeNum, [ref]$dm)) {
        $res = "$($dm.dmPelsWidth)x$($dm.dmPelsHeight)@$($dm.dmDisplayFrequency)Hz"
        if ($modes -notcontains $res) {
            $modes += $res
        }
        $modeNum++
        $dm.dmSize = [System.Runtime.InteropServices.Marshal]::SizeOf($dm) -as [uint16]
    }
    foreach ($m in ($modes | Sort-Object)) { Log "    $m" }
    Log "  Total unique modes: $($modes.Count)"

    # Check specifically for 2340x1080
    $has2340 = $modes | Where-Object { $_ -like "2340x1080*" }
    if ($has2340) {
        Log "  >> 2340x1080 IS already supported"
    } else {
        Log "  >> 2340x1080 is NOT supported"
    }
}

# ============================================================
# STEP 2: Read current Amyuni registry
# ============================================================
Log ""
Log "=== STEP 2: Current Amyuni registry entries ==="

$regPath = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\WUDF\Services\usbmmIdd\Parameters\Monitors"
try {
    $regProps = Get-ItemProperty $regPath
    for ($i = 0; $i -lt 20; $i++) {
        $val = $regProps."$i"
        if ($val) {
            Log "  [$i] = $val"
        } else {
            Log "  Next free index: $i"
            break
        }
    }
} catch {
    Log "  ERROR reading registry: $_"
}

# ============================================================
# STEP 3: Add test resolution 2340,1080
# ============================================================
Log ""
Log "=== STEP 3: Add test resolution 2340,1080 to registry ==="

$testW = 2340
$testH = 1080
$testRes = "$testW,$testH"
$alreadyExists = $false

try {
    $regProps = Get-ItemProperty $regPath
    for ($i = 0; $i -lt 20; $i++) {
        $val = $regProps."$i"
        if ($val -eq $testRes) {
            Log "  Resolution $testRes already exists at index $i"
            $alreadyExists = $true
            break
        }
        if (-not $val) {
            # Add at this index
            Set-ItemProperty -Path $regPath -Name "$i" -Value $testRes -Type String
            Log "  Added $testRes at index $i"
            break
        }
    }
} catch {
    Log "  ERROR writing registry: $_"
    Log "  >> Make sure you run this script as Administrator!"
}

# Verify
Log "  Verifying registry after write:"
try {
    $regProps = Get-ItemProperty $regPath
    for ($i = 0; $i -lt 20; $i++) {
        $val = $regProps."$i"
        if ($val) { Log "    [$i] = $val" } else { break }
    }
} catch {
    Log "  ERROR: $_"
}

# ============================================================
# STEP 4: Plug out all, then plug in one virtual display
# ============================================================
Log ""
Log "=== STEP 4: Plug out all Amyuni VDs then plug in one ==="

$interfaceGuid = [Guid]"b5ffd75f-da40-4353-8ff8-b6daf6f1d8ca"
$ioCtlCode = 2307084  # PLUG_MONITOR_IO_CONTROL_CODE

function Invoke-AmyuniIoctl($cmd) {
    $DIGCF = [DisplayHelper]::DIGCF_PRESENT -bor [DisplayHelper]::DIGCF_DEVICEINTERFACE
    $devInfoSet = [DisplayHelper]::SetupDiGetClassDevs([ref]$interfaceGuid, [IntPtr]::Zero, [IntPtr]::Zero, $DIGCF)

    if ($devInfoSet -eq [IntPtr]::new(-1)) {
        Log "  SetupDiGetClassDevs FAILED: $([System.Runtime.InteropServices.Marshal]::GetLastWin32Error())"
        return $false
    }

    $interfaceData = New-Object DisplayHelper+SP_DEVICE_INTERFACE_DATA
    $interfaceData.cbSize = [System.Runtime.InteropServices.Marshal]::SizeOf($interfaceData) -as [uint32]

    if (-not [DisplayHelper]::SetupDiEnumDeviceInterfaces($devInfoSet, [IntPtr]::Zero, [ref]$interfaceGuid, 0, [ref]$interfaceData)) {
        Log "  SetupDiEnumDeviceInterfaces FAILED: $([System.Runtime.InteropServices.Marshal]::GetLastWin32Error())"
        [DisplayHelper]::SetupDiDestroyDeviceInfoList($devInfoSet) | Out-Null
        return $false
    }

    # Get required size
    [uint32]$reqSize = 0
    [DisplayHelper]::SetupDiGetDeviceInterfaceDetail($devInfoSet, [ref]$interfaceData, [IntPtr]::Zero, 0, [ref]$reqSize, [IntPtr]::Zero) | Out-Null

    # Allocate buffer (cbSize = 8 for x64)
    $detailBuf = [System.Runtime.InteropServices.Marshal]::AllocHGlobal($reqSize)
    if ([IntPtr]::Size -eq 8) {
        [System.Runtime.InteropServices.Marshal]::WriteInt32($detailBuf, 8)
    } else {
        [System.Runtime.InteropServices.Marshal]::WriteInt32($detailBuf, 6)
    }

    if (-not [DisplayHelper]::SetupDiGetDeviceInterfaceDetail($devInfoSet, [ref]$interfaceData, $detailBuf, $reqSize, [ref]$reqSize, [IntPtr]::Zero)) {
        Log "  SetupDiGetDeviceInterfaceDetail FAILED: $([System.Runtime.InteropServices.Marshal]::GetLastWin32Error())"
        [System.Runtime.InteropServices.Marshal]::FreeHGlobal($detailBuf)
        [DisplayHelper]::SetupDiDestroyDeviceInfoList($devInfoSet) | Out-Null
        return $false
    }

    $devicePath = [System.Runtime.InteropServices.Marshal]::PtrToStringUni([IntPtr]::Add($detailBuf, 4))
    [System.Runtime.InteropServices.Marshal]::FreeHGlobal($detailBuf)
    [DisplayHelper]::SetupDiDestroyDeviceInfoList($devInfoSet) | Out-Null

    Log "  Device path: $devicePath"

    $handle = [DisplayHelper]::CreateFile($devicePath, [DisplayHelper]::GENERIC_WRITE, 0, [IntPtr]::Zero, [DisplayHelper]::OPEN_EXISTING, 0, [IntPtr]::Zero)
    if ($handle.IsInvalid) {
        Log "  CreateFile FAILED: $([System.Runtime.InteropServices.Marshal]::GetLastWin32Error())"
        return $false
    }

    [byte[]]$cmdBytes = @($cmd, 0, 0, 0)
    [uint32]$bytesReturned = 0
    $ok = [DisplayHelper]::DeviceIoControl($handle, $ioCtlCode, $cmdBytes, 4, [IntPtr]::Zero, 0, [ref]$bytesReturned, [IntPtr]::Zero)
    $handle.Close()

    if (-not $ok) {
        Log "  DeviceIoControl FAILED: $([System.Runtime.InteropServices.Marshal]::GetLastWin32Error())"
        return $false
    }
    Log "  DeviceIoControl OK (cmd=$cmd)"
    return $true
}

# Count current Amyuni displays
$countBefore = $amyuniDisplays.Count
Log "  Current Amyuni VD count: $countBefore"

# Plug out all
if ($countBefore -gt 0) {
    Log "  Plugging out all VDs..."
    for ($i = 0; $i -lt $countBefore; $i++) {
        $ok = Invoke-AmyuniIoctl 0x00
        if (-not $ok) { Log "  Plug out $i failed"; break }
        Start-Sleep -Milliseconds 500
    }
    Start-Sleep -Seconds 1
}

# Plug in one
Log "  Plugging in one VD..."
$ok = Invoke-AmyuniIoctl 0x10
if (-not $ok) {
    Log "  PLUG IN FAILED"
} else {
    Log "  Plug in succeeded, waiting 3s for display to initialize..."
    Start-Sleep -Seconds 3
}

# ============================================================
# STEP 5: Enumerate modes of new Amyuni display
# ============================================================
Log ""
Log "=== STEP 5: Enumerate modes of new Amyuni display ==="

$amyuniDisplays2 = @()
$dd2 = New-Object DisplayHelper+DISPLAY_DEVICEW
$dd2.cb = [System.Runtime.InteropServices.Marshal]::SizeOf($dd2)
$devNum2 = 0

while ([DisplayHelper]::EnumDisplayDevicesW($null, $devNum2, [ref]$dd2, 0)) {
    $devNum2++
    if ($dd2.DeviceString -like "USB Mobile Monitor*") {
        if ($dd2.StateFlags -band [DisplayHelper]::DISPLAY_DEVICE_ACTIVE) {
            $amyuniDisplays2 += @{Name=$dd2.DeviceName; String=$dd2.DeviceString}
            Log "  Found ACTIVE: $($dd2.DeviceName)"
        }
    }
    $dd2.cb = [System.Runtime.InteropServices.Marshal]::SizeOf($dd2)
}

Log "  New Amyuni VD count: $($amyuniDisplays2.Count)"

foreach ($disp in $amyuniDisplays2) {
    Log ""
    Log "  Modes for $($disp.Name):"
    $dm2 = New-Object DisplayHelper+DEVMODEW
    $dm2.dmSize = [System.Runtime.InteropServices.Marshal]::SizeOf($dm2) -as [uint16]
    $modeNum2 = 0
    $modes2 = @()
    while ([DisplayHelper]::EnumDisplaySettingsW($disp.Name, $modeNum2, [ref]$dm2)) {
        $res2 = "$($dm2.dmPelsWidth)x$($dm2.dmPelsHeight)@$($dm2.dmDisplayFrequency)Hz"
        if ($modes2 -notcontains $res2) {
            $modes2 += $res2
        }
        $modeNum2++
        $dm2.dmSize = [System.Runtime.InteropServices.Marshal]::SizeOf($dm2) -as [uint16]
    }
    foreach ($m in ($modes2 | Sort-Object)) { Log "    $m" }
    Log "  Total unique modes: $($modes2.Count)"

    $has2340 = $modes2 | Where-Object { $_ -like "2340x1080*" }
    if ($has2340) {
        Log "  >> SUCCESS: 2340x1080 IS supported after registry injection!"
    } else {
        Log "  >> FAIL: 2340x1080 still NOT supported (driver may not re-read registry)"
    }

    # ============================================================
    # STEP 6: Try ChangeDisplaySettingsEx with 2340x1080
    # ============================================================
    if ($has2340) {
        Log ""
        Log "=== STEP 6: Try ChangeDisplaySettingsEx with 2340x1080 ==="
        $dm3 = New-Object DisplayHelper+DEVMODEW
        $dm3.dmSize = [System.Runtime.InteropServices.Marshal]::SizeOf($dm3) -as [uint16]
        $dm3.dmPelsWidth = $testW
        $dm3.dmPelsHeight = $testH
        $dm3.dmFields = [DisplayHelper]::DM_PELSWIDTH -bor [DisplayHelper]::DM_PELSHEIGHT

        $result = [DisplayHelper]::ChangeDisplaySettingsExW($disp.Name, [ref]$dm3, [IntPtr]::Zero,
            ([DisplayHelper]::CDS_UPDATEREGISTRY -bor [DisplayHelper]::CDS_GLOBAL -bor [DisplayHelper]::CDS_RESET),
            [IntPtr]::Zero)

        switch ($result) {
            0  { Log "  >> SUCCESS: Resolution changed to 2340x1080! (DISP_CHANGE_SUCCESSFUL)" }
            1  { Log "  >> NEEDS RESTART (DISP_CHANGE_RESTART)" }
            -1 { Log "  >> FAIL: DISP_CHANGE_FAILED" }
            -2 { Log "  >> FAIL: DISP_CHANGE_BADMODE (resolution not supported by driver)" }
            -3 { Log "  >> FAIL: DISP_CHANGE_NOTUPDATED" }
            -4 { Log "  >> FAIL: DISP_CHANGE_BADFLAGS" }
            -5 { Log "  >> FAIL: DISP_CHANGE_BADPARAM" }
            default { Log "  >> FAIL: Unknown result $result" }
        }

        # Verify current resolution
        $dmCur = New-Object DisplayHelper+DEVMODEW
        $dmCur.dmSize = [System.Runtime.InteropServices.Marshal]::SizeOf($dmCur) -as [uint16]
        if ([DisplayHelper]::EnumDisplaySettingsW($disp.Name, [DisplayHelper]::ENUM_CURRENT_SETTINGS, [ref]$dmCur)) {
            Log "  Current resolution: $($dmCur.dmPelsWidth)x$($dmCur.dmPelsHeight)"
        }
    } else {
        Log ""
        Log "=== STEP 6: SKIPPED (2340x1080 not in mode list) ==="
        Log "  The Amyuni driver does NOT re-read registry on plug-in."
        Log "  Alternative: need to restart driver service after registry update."
    }
}

# ============================================================
# CLEANUP: Plug out the test display
# ============================================================
Log ""
Log "=== CLEANUP: Plugging out test VD ==="
$ok = Invoke-AmyuniIoctl 0x00
if ($ok) { Log "  Cleaned up." } else { Log "  Cleanup plug-out failed." }

Log ""
Log "=== TEST COMPLETE ==="
Log "Results saved to: $outFile"
