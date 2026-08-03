# 技能頁自動化驗收：切職業 → 選技能 → 等級快捷鈕 → 搜尋，各步驟截圖。
# 用法：powershell -ExecutionPolicy Bypass -File scripts/skill_nav.ps1
param(
  [string]$OutDir = "C:/Users/cl202/Desktop/經典服查看工具/out"
)
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class SkillNavApi {
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
[SkillNavApi]::SetWindowPos($h, [IntPtr](-1), 600, 300, 0, 0, 0x51) | Out-Null
Start-Sleep -Milliseconds 500
# AttachThreadInput 繞過前景限制，強制鍵盤/滑鼠焦點
$fg = [SkillNavApi]::GetForegroundWindow()
$fgThread = [SkillNavApi]::GetWindowThreadProcessId($fg, [ref]([uint32]$x = 0))
$myThread = [SkillNavApi]::GetCurrentThreadId()
[SkillNavApi]::AttachThreadInput($myThread, $fgThread, $true) | Out-Null
[SkillNavApi]::SetForegroundWindow($h) | Out-Null
[SkillNavApi]::AttachThreadInput($myThread, $fgThread, $false) | Out-Null
Start-Sleep -Milliseconds 400
$rect = New-Object SkillNavApi+R
function ClickRel($x, $y) {
  [SkillNavApi]::GetWindowRect($h, [ref]$rect) | Out-Null
  # mouse_event ABSOLUTE 模式：座標正規化到 0..65535，對 WebView2 較可靠
  $sw = [System.Windows.Forms.SystemInformation]::PrimaryMonitorSize.Width
  $sh = [System.Windows.Forms.SystemInformation]::PrimaryMonitorSize.Height
  $nx = [int](($rect.L + $x) * 65535 / ($sw - 1))
  $ny = [int](($rect.T + $y) * 65535 / ($sh - 1))
  [SkillNavApi]::mouse_event(0x8001, $nx, $ny, 0, 0)
  Start-Sleep -Milliseconds 200
  [SkillNavApi]::mouse_event(0x8002, $nx, $ny, 0, 0)
  Start-Sleep -Milliseconds 80
  [SkillNavApi]::mouse_event(0x8004, $nx, $ny, 0, 0)
}
function Shot($name) {
  Start-Sleep -Seconds 2
  [SkillNavApi]::GetWindowRect($h, [ref]$rect) | Out-Null
  $w = $rect.Rt - $rect.L; $hh = $rect.B - $rect.T
  $bmp = New-Object System.Drawing.Bitmap $w, $hh
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $hdc = $g.GetHdc()
  [SkillNavApi]::PrintWindow($h, $hdc, 2) | Out-Null
  $g.ReleaseHdc($hdc); $g.Dispose()
  $bmp.Save("$OutDir/$name.png", [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
  Write-Host "saved $name"
}
# 1. 左欄切到「劍士」
ClickRel 60 184
Shot "skill_nav_1_job"
# 2. 技能視窗點第二個技能
ClickRel 446 370
Shot "skill_nav_2_skill"
# 3. 等級快捷鈕「10」
ClickRel 1062 250
Shot "skill_nav_3_level10"
# 4. 搜尋「憤怒」（貼上避免 IME 問題）
ClickRel 130 98
Start-Sleep -Milliseconds 400
Set-Clipboard "憤怒"
[System.Windows.Forms.SendKeys]::SendWait("^v")
Shot "skill_nav_4_search"
