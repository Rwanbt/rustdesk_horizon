## Final Amyuni test: plug in VD, check modes, try 2340x1080
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

public class ATest {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct DISPLAY_DEVICEW {
        [MarshalAs(UnmanagedType.U4)] public uint cb;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string DeviceName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceString;
        [MarshalAs(UnmanagedType.U4)] public uint StateFlags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceID;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceKey;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct DEVMODEW {
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmDeviceName;
        public ushort dmSpecVersion; public ushort dmDriverVersion;
        public ushort dmSize; public ushort dmDriverExtra;
        public uint dmFields;
        public int dmPositionX; public int dmPositionY;
        public uint dmDisplayOrientation; public uint dmDisplayFixedOutput;
        public short dmColor; public short dmDuplex; public short dmYResolution;
        public short dmTTOption; public short dmCollate;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmFormName;
        public ushort dmLogPixels; public uint dmBitsPerPel;
        public uint dmPelsWidth; public uint dmPelsHeight;
        public uint dmDisplayFlags; public uint dmDisplayFrequency;
        public uint dmICMMethod; public uint dmICMIntent;
        public uint dmMediaType; public uint dmDitherType;
        public uint dmReserved1; public uint dmReserved2;
        public uint dmPanningWidth; public uint dmPanningHeight;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct SP_DEVICE_INTERFACE_DATA {
        public uint cbSize; public Guid InterfaceClassGuid; public uint Flags; public IntPtr Reserved;
    }

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern bool EnumDisplayDevicesW(IntPtr lpDevice, uint iDevNum, ref DISPLAY_DEVICEW dd, uint dwFlags);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern bool EnumDisplaySettingsW(string name, int iModeNum, ref DEVMODEW dm);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int ChangeDisplaySettingsExW(string name, ref DEVMODEW dm, IntPtr hwnd, uint flags, IntPtr lParam);

    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern IntPtr SetupDiGetClassDevs(ref Guid g, IntPtr e, IntPtr h, uint f);
    [DllImport("setupapi.dll", SetLastError = true)]
    public static extern bool SetupDiEnumDeviceInterfaces(IntPtr s, IntPtr d, ref Guid g, uint i, ref SP_DEVICE_INTERFACE_DATA data);
    [DllImport("setupapi.dll", CharSet = CharSet.Auto, SetLastError = true)]
    public static extern bool SetupDiGetDeviceInterfaceDetail(IntPtr s, ref SP_DEVICE_INTERFACE_DATA data, IntPtr buf, uint sz, ref uint req, IntPtr info);
    [DllImport("setupapi.dll", SetLastError = true)]
    public static extern bool SetupDiDestroyDeviceInfoList(IntPtr s);
    [DllImport("kernel32.dll", CharSet = CharSet.Auto, SetLastError = true)]
    public static extern SafeFileHandle CreateFile(string n, uint a, uint sh, IntPtr sec, uint disp, uint fl, IntPtr t);
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool DeviceIoControl(SafeFileHandle h, uint code, byte[] inBuf, uint inSz, IntPtr outBuf, uint outSz, ref uint ret, IntPtr ol);
}
"@

function Get-AmyuniActive {
    $result = @()
    $dd = New-Object ATest+DISPLAY_DEVICEW
    $dd.cb = [System.Runtime.InteropServices.Marshal]::SizeOf($dd)
    $i = 0
    while ([ATest]::EnumDisplayDevicesW([IntPtr]::Zero, $i, [ref]$dd, 0)) {
        if (($dd.DeviceString -like "USB Mobile Monitor*") -and ($dd.StateFlags -band 1)) {
            $result += $dd.DeviceName
        }
        $i++
        $dd.cb = [System.Runtime.InteropServices.Marshal]::SizeOf($dd)
    }
    return $result
}

function Invoke-Ioctl($cmd) {
    $guid = [Guid]"b5ffd75f-da40-4353-8ff8-b6daf6f1d8ca"
    $set = [ATest]::SetupDiGetClassDevs([ref]$guid, [IntPtr]::Zero, [IntPtr]::Zero, 0x12)
    if ($set -eq [IntPtr]::new(-1)) { Write-Host "  FAIL: SetupDiGetClassDevs"; return $false }
    $ifd = New-Object ATest+SP_DEVICE_INTERFACE_DATA
    $ifd.cbSize = [System.Runtime.InteropServices.Marshal]::SizeOf($ifd) -as [uint32]
    if (-not [ATest]::SetupDiEnumDeviceInterfaces($set, [IntPtr]::Zero, [ref]$guid, 0, [ref]$ifd)) {
        [ATest]::SetupDiDestroyDeviceInfoList($set) | Out-Null; return $false
    }
    [uint32]$req = 0
    [ATest]::SetupDiGetDeviceInterfaceDetail($set, [ref]$ifd, [IntPtr]::Zero, 0, [ref]$req, [IntPtr]::Zero) | Out-Null
    $buf = [System.Runtime.InteropServices.Marshal]::AllocHGlobal($req)
    [System.Runtime.InteropServices.Marshal]::WriteInt32($buf, $(if ([IntPtr]::Size -eq 8) {8} else {6}))
    [ATest]::SetupDiGetDeviceInterfaceDetail($set, [ref]$ifd, $buf, $req, [ref]$req, [IntPtr]::Zero) | Out-Null
    $path = [System.Runtime.InteropServices.Marshal]::PtrToStringUni([IntPtr]::Add($buf, 4))
    [System.Runtime.InteropServices.Marshal]::FreeHGlobal($buf)
    [ATest]::SetupDiDestroyDeviceInfoList($set) | Out-Null
    $h = [ATest]::CreateFile($path, 0x40000000, 0, [IntPtr]::Zero, 3, 0, [IntPtr]::Zero)
    if ($h.IsInvalid) { return $false }
    [byte[]]$c = @($cmd, 0, 0, 0); [uint32]$br = 0
    $ok = [ATest]::DeviceIoControl($h, 2307084, $c, 4, [IntPtr]::Zero, 0, [ref]$br, [IntPtr]::Zero)
    $h.Close()
    return $ok
}

# ======= START =======
Write-Host "=== STEP 1: Current state ===" -ForegroundColor Cyan
$before = Get-AmyuniActive
Write-Host "  Active Amyuni VDs: $($before.Count)"

# ======= PLUG IN =======
Write-Host ""
Write-Host "=== STEP 2: Plug in VD ===" -ForegroundColor Cyan
$ok = Invoke-Ioctl 0x10
Write-Host "  DeviceIoControl: $(if ($ok) {'OK'} else {'FAILED'})"

# ======= WAIT =======
Write-Host ""
Write-Host "=== STEP 3: Waiting for display... ===" -ForegroundColor Cyan
$newDisp = $null
for ($t = 1; $t -le 20; $t++) {
    Start-Sleep -Milliseconds 500
    $now = Get-AmyuniActive
    if ($now.Count -gt $before.Count) {
        $newDisp = $now | Where-Object { $before -notcontains $_ } | Select-Object -First 1
        if (-not $newDisp) { $newDisp = $now[-1] }
        Write-Host "  NEW DISPLAY at $($t * 0.5)s: $newDisp" -ForegroundColor Green
        break
    }
    if ($t % 4 -eq 0) { Write-Host "  $($t * 0.5)s: still waiting... (active=$($now.Count))" }
}
if (-not $newDisp) {
    Write-Host "  No new display after 10s" -ForegroundColor Red
    Write-Host ""
    Write-Host "=== CLEANUP ===" -ForegroundColor Cyan
    Invoke-Ioctl 0x00 | Out-Null
    Write-Host "  Done."
    Read-Host "Press Enter..."
    exit
}

# ======= ENUMERATE MODES =======
Write-Host ""
Write-Host "=== STEP 4: Modes for $newDisp ===" -ForegroundColor Cyan
$dm = New-Object ATest+DEVMODEW
$dm.dmSize = [System.Runtime.InteropServices.Marshal]::SizeOf($dm) -as [uint16]
$modeNum = 0; $modes = @()
while ([ATest]::EnumDisplaySettingsW($newDisp, $modeNum, [ref]$dm)) {
    $res = "$($dm.dmPelsWidth)x$($dm.dmPelsHeight)"
    if ($modes -notcontains $res) { $modes += $res }
    $modeNum++
    $dm.dmSize = [System.Runtime.InteropServices.Marshal]::SizeOf($dm) -as [uint16]
}
foreach ($m in ($modes | Sort-Object)) { Write-Host "  $m" }
Write-Host "  Total unique resolutions: $($modes.Count)"

$has2340 = $modes | Where-Object { $_ -eq "2340x1080" }
if ($has2340) {
    Write-Host "  >> 2340x1080 IS SUPPORTED!" -ForegroundColor Green
} else {
    Write-Host "  >> 2340x1080 NOT in modes" -ForegroundColor Yellow
}

# ======= TRY CHANGE RESOLUTION =======
Write-Host ""
Write-Host "=== STEP 5: ChangeDisplaySettingsEx ===" -ForegroundColor Cyan

# First try 2340x1080
$dm2 = New-Object ATest+DEVMODEW
$dm2.dmSize = [System.Runtime.InteropServices.Marshal]::SizeOf($dm2) -as [uint16]
$dm2.dmPelsWidth = 2340; $dm2.dmPelsHeight = 1080
$dm2.dmFields = 0x80000 -bor 0x100000  # DM_PELSWIDTH | DM_PELSHEIGHT
$res = [ATest]::ChangeDisplaySettingsExW($newDisp, [ref]$dm2, [IntPtr]::Zero, (0x01 -bor 0x08 -bor 0x40000000), [IntPtr]::Zero)
switch ($res) {
    0  { Write-Host "  2340x1080: SUCCESS!" -ForegroundColor Green }
    -2 { Write-Host "  2340x1080: BADMODE (not supported)" -ForegroundColor Red }
    default { Write-Host "  2340x1080: result=$res" -ForegroundColor Red }
}

# Verify current resolution
$dmC = New-Object ATest+DEVMODEW
$dmC.dmSize = [System.Runtime.InteropServices.Marshal]::SizeOf($dmC) -as [uint16]
if ([ATest]::EnumDisplaySettingsW($newDisp, -1, [ref]$dmC)) {
    Write-Host "  Current: $($dmC.dmPelsWidth)x$($dmC.dmPelsHeight)"
}

# ======= CLEANUP =======
Write-Host ""
Write-Host "=== CLEANUP ===" -ForegroundColor Cyan
Invoke-Ioctl 0x00 | Out-Null
Write-Host "  Plugged out."

Write-Host ""
Write-Host "Press Enter..."
Read-Host
