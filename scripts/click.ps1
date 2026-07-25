param(
  [int]$X,   # window 相對座標
  [int]$Y
)
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class FgApi {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, int e);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
$proc = Get-Process msclassic -ErrorAction SilentlyContinue |
  Where-Object { $_.MainWindowHandle -ne 0 -and $_.MainWindowTitle -ne '' } |
  Select-Object -First 1
if (-not $proc) { Write-Host "NO_WINDOW"; exit 1 }
[FgApi]::ShowWindow($proc.MainWindowHandle, 9) | Out-Null  # SW_RESTORE
[FgApi]::SetForegroundWindow($proc.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 600
$rect = New-Object FgApi+RECT
[FgApi]::GetWindowRect($proc.MainWindowHandle, [ref]$rect) | Out-Null
$ax = $rect.Left + $X
$ay = $rect.Top + $Y
[FgApi]::SetCursorPos($ax, $ay) | Out-Null
Start-Sleep -Milliseconds 200
[FgApi]::mouse_event(2, 0, 0, 0, 0)
Start-Sleep -Milliseconds 80
[FgApi]::mouse_event(4, 0, 0, 0, 0)
Write-Host "clicked rel($X, $Y) abs($ax, $ay)"
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class FgApi {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, int e);
}
"@
$proc = Get-Process msclassic -ErrorAction SilentlyContinue |
  Where-Object { $_.MainWindowHandle -ne 0 -and $_.MainWindowTitle -ne '' } |
  Select-Object -First 1
if (-not $proc) { Write-Host "NO_WINDOW"; exit 1 }
[FgApi]::ShowWindow($proc.MainWindowHandle, 9) | Out-Null  # SW_RESTORE
[FgApi]::SetForegroundWindow($proc.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 600
[FgApi]::SetCursorPos($X, $Y) | Out-Null
Start-Sleep -Milliseconds 200
[FgApi]::mouse_event(2, 0, 0, 0, 0)
Start-Sleep -Milliseconds 80
[FgApi]::mouse_event(4, 0, 0, 0, 0)
Write-Host "clicked ($X, $Y)"
