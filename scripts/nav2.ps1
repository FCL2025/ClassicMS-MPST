param(
  [string]$Query = "Afterimage/blank/axe"
)
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class NavApi {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, int e);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  public struct R { public int L, T, Rt, B; }
}
"@
$proc = Get-Process msclassic | Where-Object { $_.MainWindowTitle -ne '' } | Select-Object -First 1
if (-not $proc) { Write-Host "NO_WINDOW"; exit 1 }
$h = $proc.MainWindowHandle
$rect = New-Object NavApi+R
function Click($x, $y) {
  [NavApi]::SetCursorPos($x, $y) | Out-Null
  Start-Sleep -Milliseconds 150
  [NavApi]::mouse_event(2, 0, 0, 0, 0)
  Start-Sleep -Milliseconds 60
  [NavApi]::mouse_event(4, 0, 0, 0, 0)
}
function Shot($name) {
  Start-Sleep -Milliseconds 700
  & powershell -ExecutionPolicy Bypass -File "C:/Users/cl202/Desktop/經典服查看工具/scripts/screenshot.ps1" -OutFile "C:/Users/cl202/Desktop/經典服查看工具/out/$name" | Out-Null
  Write-Host "shot $name"
}
[NavApi]::GetWindowRect($h, [ref]$rect) | Out-Null
[NavApi]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 400
Click ($rect.L + 550) ($rect.T + 52)
Shot "nav_1_focus.png"
Set-Clipboard $Query
[System.Windows.Forms.SendKeys]::SendWait("^v")
Start-Sleep -Seconds 2
Shot "nav_2_typed.png"
[NavApi]::GetWindowRect($h, [ref]$rect) | Out-Null
Click ($rect.L + 300) ($rect.T + 100)
Start-Sleep -Seconds 3
Shot "nav_3_picked.png"
Write-Host done
