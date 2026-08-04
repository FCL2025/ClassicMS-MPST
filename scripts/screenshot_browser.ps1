param(
  [string]$OutFile = "$env:TEMP\msclassic_browser_shot.png",
  [int]$WaitSec = 0,
  [string]$TitleMatch = "localhost"
)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinApiB {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdcBlt, uint nFlags);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
if ($WaitSec -gt 0) { Start-Sleep -Seconds $WaitSec }
$proc = Get-Process msedge, chrome, brave, firefox -ErrorAction SilentlyContinue |
  Where-Object { $_.MainWindowHandle -ne 0 -and $_.MainWindowTitle -like "*$TitleMatch*" } |
  Select-Object -First 1
if (-not $proc) { Write-Host "NO_WINDOW ($TitleMatch)"; exit 1 }
Write-Host ("process: {0} title: {1}" -f $proc.ProcessName, $proc.MainWindowTitle)
[WinApiB]::SetForegroundWindow($proc.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 500
$rect = New-Object WinApiB+RECT
[WinApiB]::GetWindowRect($proc.MainWindowHandle, [ref]$rect) | Out-Null
$w = $rect.Right - $rect.Left
$h = $rect.Bottom - $rect.Top
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
$ok = [WinApiB]::PrintWindow($proc.MainWindowHandle, $hdc, 2)
$g.ReleaseHdc()
$g.Dispose()
if (-not $ok) { Write-Host "PrintWindow failed" }
$bmp.Save($OutFile, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "saved: $OutFile"
