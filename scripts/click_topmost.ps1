param(
  [int]$X,
  [int]$Y,
  [switch]$OnlyTopmost
)
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class TopApi {
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int X, int Y, int cx, int cy, uint uFlags);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, int e);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
$proc = Get-Process msclassic -ErrorAction SilentlyContinue |
  Where-Object { $_.MainWindowHandle -ne 0 -and $_.MainWindowTitle -ne '' } |
  Select-Object -First 1
if (-not $proc) { Write-Host "NO_WINDOW"; exit 1 }
# HWND_TOPMOST = -1, SWP_NOSIZE|SWP_NOMOVE|SWP_SHOWWINDOW = 0x43
[TopApi]::SetWindowPos($proc.MainWindowHandle, [IntPtr](-1), 0, 0, 0, 0, 0x43) | Out-Null
Start-Sleep -Milliseconds 400
if ($OnlyTopmost) { Write-Host "topmost set"; exit 0 }
$rect = New-Object TopApi+RECT
[TopApi]::GetWindowRect($proc.MainWindowHandle, [ref]$rect) | Out-Null
$ax = $rect.Left + $X
$ay = $rect.Top + $Y
[TopApi]::SetCursorPos($ax, $ay) | Out-Null
Start-Sleep -Milliseconds 200
[TopApi]::mouse_event(2, 0, 0, 0, 0)
Start-Sleep -Milliseconds 80
[TopApi]::mouse_event(4, 0, 0, 0, 0)
Write-Host "clicked rel($X, $Y) abs($ax, $ay)"
