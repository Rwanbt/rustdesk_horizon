## Minimal test: just EnumDisplayDevices with struct size verification
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public class WinDisp {
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

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern bool EnumDisplayDevicesW(
        string lpDevice, uint iDevNum, ref DISPLAY_DEVICEW lpDisplayDevice, uint dwFlags);
}
"@

$dd = New-Object WinDisp+DISPLAY_DEVICEW
$structSize = [System.Runtime.InteropServices.Marshal]::SizeOf($dd)
Write-Host "Struct size: $structSize (expected: 840)"
$dd.cb = $structSize

$devNum = 0
Write-Host ""
Write-Host "Enumerating displays..."
while ([WinDisp]::EnumDisplayDevicesW($null, $devNum, [ref]$dd, 0)) {
    $active = if ($dd.StateFlags -band 1) {"ACTIVE"} else {"inactive"}
    Write-Host "  [$devNum] $($dd.DeviceName) | $($dd.DeviceString) | $active | flags=0x$($dd.StateFlags.ToString('X'))"
    $devNum++
    $dd.cb = $structSize
}
$lastErr = [System.Runtime.InteropServices.Marshal]::GetLastWin32Error()
Write-Host ""
Write-Host "Total: $devNum displays (LastError=$lastErr)"
Write-Host ""
Write-Host "Press Enter to exit..."
Read-Host
