# Click (or double-click) at a point of the main window of process $Process, given in pixels from
# the window's top left corner (as in a window-shot.ps1 picture).
#
#   pwsh scripts/e2e/click.ps1 ibisOffice 755 410 [-Double]
param(
    [Parameter(Mandatory = $true)][string]$Process,
    [Parameter(Mandatory = $true)][int]$X,
    [Parameter(Mandatory = $true)][int]$Y,
    [switch]$Double
)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Mouse {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, IntPtr e);
    public static void Click() { mouse_event(2, 0, 0, 0, IntPtr.Zero); mouse_event(4, 0, 0, 0, IntPtr.Zero); }
}
'@
[Mouse]::SetProcessDPIAware() | Out-Null
$p = Get-Process -Name $Process | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $p) { throw "no window of $Process" }
[Mouse]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 300
$r = New-Object Mouse+RECT
[Mouse]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
[Mouse]::SetCursorPos($r.Left + $X, $r.Top + $Y) | Out-Null
Start-Sleep -Milliseconds 150
[Mouse]::Click()
if ($Double) { Start-Sleep -Milliseconds 60; [Mouse]::Click() }
