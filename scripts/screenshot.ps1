param(
  [string]$OutFile = "$env:TEMP\msclassic_shot.png",
  [int]$WaitSec = 0
)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinApi {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdcBlt, uint nFlags);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
if ($WaitSec -gt 0) { Start-Sleep -Seconds $WaitSec }
$proc = Get-Process msclassic -ErrorAction SilentlyContinue |
  Where-Object { $_.MainWindowHandle -ne 0 -and $_.MainWindowTitle -ne '' } |
  Select-Object -First 1
if (-not $proc) { Write-Host "NO_WINDOW"; exit 1 }
$rect = New-Object WinApi+RECT
[WinApi]::GetWindowRect($proc.MainWindowHandle, [ref]$rect) | Out-Null
$w = $rect.Right - $rect.Left
$h = $rect.Bottom - $rect.Top
Write-Host ("window: {0},{1} {2}x{3}" -f $rect.Left, $rect.Top, $w, $h)
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
# PW_RENDERFULLCONTENT(2)：DWM 合成內容，被遮擋也能截到
$ok = [WinApi]::PrintWindow($proc.MainWindowHandle, $hdc, 2)
$g.ReleaseHdc($hdc)
$g.Dispose()
if (-not $ok) { Write-Host "PrintWindow failed, fallback CopyFromScreen" }
$bmp.Save($OutFile, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "saved: $OutFile"
