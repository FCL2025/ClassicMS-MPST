# clean-target.ps1
# Rust target/ 維護工具，避免 incremental cache 與 debug 符號累積到數十 GB。
#
# 用法：
#   pwsh scripts/clean-target.ps1 -Mode sweep    # 預設：清掉 30 天沒碰過的產物（需 cargo-sweep）
#   pwsh scripts/clean-target.ps1 -Mode debug    # 只清 target/debug
#   pwsh scripts/clean-target.ps1 -Mode all      # 等同 cargo clean
#   pwsh scripts/clean-target.ps1 -Mode report   # 不刪，只顯示佔用統計
#   pwsh scripts/clean-target.ps1 -Mode sweep -Days 14
#
# 建議搭配 Windows 工作排程器每週執行一次 -Mode sweep。
#
# 注意：本 repo 根目錄的 `target/` 只對應 src-tauri（`msclassic` crate）；
# `MapleLib-Rust/` 是指向 C:\Users\cl202\Desktop\MapleLib-Rust 的符號連結，
# 有自己獨立的 target/，不受本腳本影響（cargo workspace 已 exclude 它）。
# `tools/maple-oracle/` 也被 workspace exclude，是另一個獨立 target/。

[CmdletBinding()]
param(
    [ValidateSet('sweep', 'debug', 'all', 'report')]
    [string]$Mode = 'sweep',

    [int]$Days = 30
)

$ErrorActionPreference = 'Stop'
# 這台機器的主控台編碼是系統 ANSI code page（例如 Big5），不是 UTF-8。
# 這個檔案本身以帶 BOM 的 UTF-8 存檔（Windows PowerShell 5.1 沒有 BOM 時會用
# ANSI code page 解析 .ps1 內容，中文字元跨 byte 邊界會把後面的引號/大括號吃掉，
# 導致 switch 區塊解析錯亂）；這裡另外把輸出編碼轉成 UTF-8，避免 Write-Host 印出
# 亂碼（純粹顯示問題，不影響邏輯）。
try {
    [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
} catch {
    # 非互動主控台（例如某些 CI）可能不允許設定，忽略即可。
}
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $repoRoot

function Get-DirSizeGB {
    param([string]$Path)
    if (-not (Test-Path $Path)) { return 0 }
    $bytes = (Get-ChildItem -Path $Path -Recurse -Force -ErrorAction SilentlyContinue |
        Measure-Object -Property Length -Sum).Sum
    if (-not $bytes) { return 0 }
    return [math]::Round($bytes / 1GB, 2)
}

function Show-Report {
    Write-Host '== target/ 佔用統計（src-tauri / msclassic）==' -ForegroundColor Cyan
    $target = Join-Path $repoRoot 'target'
    if (-not (Test-Path $target)) {
        Write-Host '  target/ 不存在（乾淨狀態）' -ForegroundColor Green
    } else {
        $total = Get-DirSizeGB $target
        Write-Host ("  total  : {0,7:N2} GB" -f $total) -ForegroundColor Yellow

        Get-ChildItem -Path $target -Directory -Force -ErrorAction SilentlyContinue |
            ForEach-Object {
                $sz = Get-DirSizeGB $_.FullName
                [PSCustomObject]@{ Name = $_.Name; SizeGB = $sz }
            } |
            Sort-Object SizeGB -Descending |
            ForEach-Object {
                Write-Host ("  {0,-12}: {1,7:N2} GB" -f $_.Name, $_.SizeGB)
            }
    }

    Write-Host ''
    Write-Host '== 其他獨立 target/（不受本腳本清理，僅供參考）==' -ForegroundColor Cyan
    $oracleTarget = Join-Path $repoRoot 'tools\maple-oracle\target'
    Write-Host ("  tools/maple-oracle/target : {0,7:N2} GB" -f (Get-DirSizeGB $oracleTarget))
    $mapleLibLink = Join-Path $repoRoot 'MapleLib-Rust'
    $mapleLibReal = 'C:\Users\cl202\Desktop\MapleLib-Rust'
    if (Test-Path $mapleLibLink) {
        Write-Host ("  MapleLib-Rust/target (symlink → {0}) : {1,7:N2} GB" -f $mapleLibReal, (Get-DirSizeGB (Join-Path $mapleLibReal 'target')))
    }
}

switch ($Mode) {
    'report' {
        Show-Report
    }
    'all' {
        Write-Host '執行 cargo clean（清空整個 target/）...' -ForegroundColor Yellow
        cargo clean
        Show-Report
    }
    'debug' {
        $debugPath = Join-Path $repoRoot 'target\debug'
        if (Test-Path $debugPath) {
            $before = Get-DirSizeGB $debugPath
            Write-Host ("刪除 target/debug ({0:N2} GB)..." -f $before) -ForegroundColor Yellow
            Remove-Item -Recurse -Force $debugPath
            Write-Host '  完成' -ForegroundColor Green
        } else {
            Write-Host 'target/debug 不存在，跳過。' -ForegroundColor Green
        }
        # 順手把孤兒 check target 也清掉
        Get-ChildItem -Path (Join-Path $repoRoot 'target') -Directory -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match '-check$' -or $_.Name -match '^maplelib-' } |
            ForEach-Object {
                Write-Host ("  順手清掉孤兒目錄 {0}" -f $_.Name) -ForegroundColor DarkGray
                Remove-Item -Recurse -Force $_.FullName
            }
        Show-Report
    }
    'sweep' {
        $sweep = Get-Command cargo-sweep -ErrorAction SilentlyContinue
        if (-not $sweep) {
            Write-Host 'cargo-sweep 尚未安裝。' -ForegroundColor Yellow
            Write-Host '安裝方式：cargo install cargo-sweep' -ForegroundColor Yellow
            Write-Host '或改用：pwsh scripts/clean-target.ps1 -Mode debug' -ForegroundColor Yellow
            exit 1
        }
        Write-Host ("執行 cargo sweep --time {0}（清掉 {0} 天沒碰過的編譯產物）..." -f $Days) -ForegroundColor Yellow
        cargo sweep --time $Days
        Show-Report
    }
}
