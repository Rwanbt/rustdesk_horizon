Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

public class DisplayHelper {
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern bool EnumDisplayDevicesW(string lpDevice, uint iDevNum, ref DISPLAY_DEVICE lpDisplayDevice, uint dwFlags);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern bool EnumDisplaySettingsW(string deviceName, int modeNum, ref DEVMODE devMode);

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct DISPLAY_DEVICE {
        public int cb;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string DeviceName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string DeviceString;
        public int StateFlags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string DeviceID;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string DeviceKey;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct DEVMODE {
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string dmDeviceName;
        public short dmSpecVersion;
        public short dmDriverVersion;
        public short dmSize;
        public short dmDriverExtra;
        public int dmFields;
        public int dmPositionX;
        public int dmPositionY;
        public int dmDisplayOrientation;
        public int dmDisplayFixedOutput;
        public short dmColor;
        public short dmDuplex;
        public short dmYResolution;
        public short dmTTOption;
        public short dmCollate;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string dmFormName;
        public short dmLogPixels;
        public int dmBitsPerPel;
        public int dmPelsWidth;
        public int dmPelsHeight;
        public int dmDisplayFlags;
        public int dmDisplayFrequency;
    }

    public static List<string[]> GetAllDevices() {
        var result = new List<string[]>();
        DISPLAY_DEVICE dev = new DISPLAY_DEVICE();
        dev.cb = Marshal.SizeOf(dev);
        for (uint i = 0; i < 20; i++) {
            if (EnumDisplayDevicesW(null, i, ref dev, 0)) {
                result.Add(new string[] { dev.DeviceName, dev.DeviceString, dev.StateFlags.ToString(), dev.DeviceID });
            }
        }
        return result;
    }

    public static List<string> GetModes(string deviceName) {
        var result = new List<string>();
        var seen = new HashSet<string>();
        DEVMODE dm = new DEVMODE();
        dm.dmSize = (short)Marshal.SizeOf(dm);
        for (int i = 0; i < 10000; i++) {
            if (!EnumDisplaySettingsW(deviceName, i, ref dm)) break;
            string key = dm.dmPelsWidth + "x" + dm.dmPelsHeight;
            if (seen.Add(key)) {
                result.Add(key + " @ " + dm.dmDisplayFrequency + "Hz (" + dm.dmBitsPerPel + "bpp)");
            }
        }
        return result;
    }
}
"@

Write-Host "=== All Display Devices ===" -ForegroundColor Cyan
$devices = [DisplayHelper]::GetAllDevices()
foreach ($d in $devices) {
    $flags = [int]$d[2]
    $active = if ($flags -band 1) { "ACTIVE" } else { "inactive" }
    $attached = if ($flags -band 2) { "ATTACHED" } else { "" }
    Write-Host "  $($d[0]) | $($d[1]) | $active $attached | $($d[3])" -ForegroundColor $(if ($d[1] -match 'USB Mobile|Amyuni') { 'Yellow' } else { 'White' })
}

Write-Host ""
Write-Host "=== Amyuni VD Supported Modes ===" -ForegroundColor Cyan
foreach ($d in $devices) {
    if ($d[1] -match 'USB Mobile|Amyuni') {
        Write-Host "Device: $($d[0]) ($($d[1]))" -ForegroundColor Yellow
        $modes = [DisplayHelper]::GetModes($d[0])
        if ($modes.Count -eq 0) {
            Write-Host "  (no modes enumerated)" -ForegroundColor Red
        } else {
            foreach ($m in $modes) {
                $highlight = $m -match '2732x2048|2388x1668|2360x1640'
                Write-Host "  $m" -ForegroundColor $(if ($highlight) { 'Green' } else { 'Gray' })
            }
            Write-Host "  Total: $($modes.Count) unique resolutions" -ForegroundColor DarkGray
        }
    }
}
