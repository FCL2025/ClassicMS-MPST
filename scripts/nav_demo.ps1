param(
  [string]$Query = "Afterimage/blank"
)
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class MouseApi {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, int dx, int dy, uint dwData, int dwExtraInfo);
  public const uint LEFTDOWN = 0x0002, LEFTUP = 0x0004;
  public static void Click(int x, int y) {
    SetCursorPos(x, y);
    System.Threading.Thread.Sleep(150);
    mouse_event(LEFTDOWN, 0, 0, 0, 0);
    System.Threading.Thread.Sleep(60);
    mouse_event(LEFTUP, 0, 0, 0, 0);
  }
}
"@
$proc = Get-Process msclassic -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $proc) { Write-Host "NO_WINDOW"; exit 1 }
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinApi2 {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
}
"@
[WinApi2]::SetForegroundWindow($proc.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 500
# window at 26,26 size 1296x839; search input near top: (26+400, 26+52)
[MouseApi]::Click(426, 78)
Start-Sleep -Milliseconds 300
Set-Clipboard -Value $Query
[System.Windows.Forms.SendKeys]::SendWait("^a")
[System.Windows.Forms.SendKeys]::SendWait("^v")
Start-Sleep -Seconds 2
# first result row: dropdown starts below input (~26+85), row height ~26 -> click (426, 110)
[MouseApi]::Click(426, 115)
Start-Sleep -Seconds 2
Write-Host "clicked"
