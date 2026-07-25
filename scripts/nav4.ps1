param(
  [string]$Query = "Afterimage/blank/axe",
  [string]$OutFile = "C:/Users/cl202/Desktop/經典服查看工具/out/phase4_canvas.png"
)
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class NavApi3 {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, int e);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);
  public struct R { public int L, T, Rt, B; }
}
"@
$proc = Get-Process msclassic | Where-Object { $_.MainWindowTitle -ne '' } | Select-Object -First 1
if (-not $proc) { Write-Host "NO_WINDOW"; exit 1 }
$h = $proc.MainWindowHandle
[NavApi3]::SetWindowPos($h, [IntPtr](-1), 600, 300, 0, 0, 0x51) | Out-Null
Start-Sleep -Milliseconds 500
# AttachThreadInput 繞過前景限制，強制鍵盤焦點
$fg = [NavApi3]::GetForegroundWindow()
$fgThread = [NavApi3]::GetWindowThreadProcessId($fg, [ref]([uint32]$x = 0))
$myThread = [NavApi3]::GetCurrentThreadId()
[NavApi3]::AttachThreadInput($myThread, $fgThread, $true) | Out-Null
[NavApi3]::SetForegroundWindow($h) | Out-Null
[NavApi3]::AttachThreadInput($myThread, $fgThread, $false) | Out-Null
Start-Sleep -Milliseconds 400
$rect = New-Object NavApi3+R
function Click($x, $y) {
  [NavApi3]::SetCursorPos($x, $y) | Out-Null
  Start-Sleep -Milliseconds 150
  [NavApi3]::mouse_event(2, 0, 0, 0, 0)
  Start-Sleep -Milliseconds 60
  [NavApi3]::mouse_event(4, 0, 0, 0, 0)
}
[NavApi3]::GetWindowRect($h, [ref]$rect) | Out-Null
Click ($rect.L + 550) ($rect.T + 52)
Start-Sleep -Milliseconds 400
Set-Clipboard $Query
[System.Windows.Forms.SendKeys]::SendWait("^v")
Start-Sleep -Seconds 2
Click ($rect.L + 300) ($rect.T + 100)
Start-Sleep -Seconds 3
[NavApi3]::GetWindowRect($h, [ref]$rect) | Out-Null
$w = $rect.Rt - $rect.L; $hh = $rect.B - $rect.T
$bmp = New-Object System.Drawing.Bitmap $w, $hh
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
[NavApi3]::PrintWindow($h, $hdc, 2) | Out-Null
$g.ReleaseHdc($hdc); $g.Dispose()
$bmp.Save($OutFile, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "saved $OutFile"
