## Minimal test: fix null parameter issue
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public class WinDisp3 {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct DISPLAY_DEVICEW {
        [MarshalAs(UnmanagedType.U4)]
        public uint cb;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string DeviceName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string DeviceString;
        [MarshalAs(UnmanagedType.U4)]
        public uint StateFlags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string DeviceID;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string DeviceKey;
    }

    // Use IntPtr instead of string to avoid PowerShell null marshalling issues
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern bool EnumDisplayDevicesW(
        IntPtr lpDevice, uint iDevNum, ref DISPLAY_DEVICEW lpDisplayDevice, uint dwFlags);

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

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern bool EnumDisplaySettingsW(
        string lpszDeviceName, int iModeNum, ref DEVMODEW lpDevMode);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int ChangeDisplaySettingsExW(
        string lpszDeviceName, ref DEVMODEW lpDevMode, IntPtr hwnd, uint dwflags, IntPtr lParam);
}
"@

Write-Host "=== Enumerating all display adapters ==="
$dd = New-Object WinDisp3+DISPLAY_DEVICEW
$dd.cb = [System.Runtime.InteropServices.Marshal]::SizeOf($dd)
Write-Host "Struct size: $($dd.cb)"

$devNum = 0
$amyuniDisplays = @()
while ([WinDisp3]::EnumDisplayDevicesW([IntPtr]::Zero, $devNum, [ref]$dd, 0)) {
    $active = if ($dd.StateFlags -band 1) {"ACTIVE"} else {"inactive"}
    $isAmyuni = $dd.DeviceString -like "USB Mobile Monitor*"
    $tag = if ($isAmyuni) {" [AMYUNI]"} else {""}
    Write-Host "  [$devNum] $($dd.DeviceName) | $($dd.DeviceString) | $active$tag"
    if ($isAmyuni -and ($dd.StateFlags -band 1)) {
        $amyuniDisplays += $dd.DeviceName
    }
    $devNum++
    $dd.cb = [System.Runtime.InteropServices.Marshal]::SizeOf($dd)
}
Write-Host "Total adapters: $devNum"
Write-Host ""

# If we found Amyuni displays, enumerate their modes
foreach ($dispName in $amyuniDisplays) {
    Write-Host "=== Modes for $dispName ==="
    $dm = New-Object WinDisp3+DEVMODEW
    $dm.dmSize = [System.Runtime.InteropServices.Marshal]::SizeOf($dm) -as [uint16]
    $modeNum = 0
    $modes = @()
    while ([WinDisp3]::EnumDisplaySettingsW($dispName, $modeNum, [ref]$dm)) {
        $res = "$($dm.dmPelsWidth)x$($dm.dmPelsHeight)"
        if ($modes -notcontains $res) { $modes += $res }
        $modeNum++
        $dm.dmSize = [System.Runtime.InteropServices.Marshal]::SizeOf($dm) -as [uint16]
    }
    foreach ($m in ($modes | Sort-Object)) { Write-Host "  $m" }
    $has2340 = $modes | Where-Object { $_ -eq "2340x1080" }
    if ($has2340) { Write-Host "  >> 2340x1080 SUPPORTED!" -ForegroundColor Green }
    else { Write-Host "  >> 2340x1080 not in list" -ForegroundColor Yellow }
}

Write-Host ""
Write-Host "Press Enter..."
Read-Host
