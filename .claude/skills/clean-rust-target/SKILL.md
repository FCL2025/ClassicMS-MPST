---
name: clean-rust-target
description: Use when the user asks to clean Rust target/, free disk space taken by build artifacts, investigate why the project folder is huge, or diagnose a slow/stuck `npm run tauri -- dev` — says "清理 target"、"清理 Rust 編譯產物"、"專案太大"、"磁碟快滿了"、"target 爆炸"、"幫我瘦身"、"dev 卡白畫面". Reports current target/ usage, executes the chosen clean mode via scripts/clean-target.ps1, and verifies space reclaimed.
license: MIT
allowed-tools: Bash, PowerShell, Read, AskUserQuestion
---

# Rust target/ 清理與瘦身

Rust/Cargo 的 `target/` 在這個專案會持續累積（`src-tauri` 的 debug target 曾量到 20+ GB）。主要兇手：

- `target/debug/incremental/` — 每次編譯都會多一個 session 目錄，舊的不會自動清。
- `target/debug/deps/` 與 `*.pdb` — Tauri 依賴龐大 + debug 符號完整保留。
- 孤兒目錄如 `target/<crate>-check/` — `cargo check` 偶爾用奇怪參數產出。

工具已就緒，**不要手動 `Remove-Item -Recurse`**，全部走 `scripts/clean-target.ps1`：

| npm 指令 | 對應 mode | 行為 |
|----------|-----------|------|
| `npm run clean:report` | report | 列出 target/ 各子目錄大小（不刪） |
| `npm run clean:sweep`  | sweep  | `cargo sweep --time 30`，需先 `cargo install cargo-sweep` |
| `npm run clean:debug`  | debug  | 只刪 `target/debug/` 與孤兒 `*-check/` 目錄 |
| `npm run clean:target` | all    | `cargo clean`，全清 |

## 相鄰/獨立的其他 target（本腳本不會動）

這個 repo 根目錄的 workspace（`Cargo.toml` 的 `[workspace]`）只包含 `src-tauri`，並明確 `exclude = ["MapleLib-Rust", "tools/maple-oracle"]`。這兩個各自有自己獨立的 `target/`：

- **`MapleLib-Rust/`**：這是一個**符號連結**，指向 `C:\Users\cl202\Desktop\MapleLib-Rust`（獨立 repo，非本專案內容的複本）。它的 `target/` 通常不大（近期量到 ~0.5 GB），一般不需要清；`src-tauri` 的正常 build 也**不依賴**它。
- **`tools/maple-oracle/`**：獨立的 Cargo package（見 `package.json` 的 `check:oracle`），有自己的 `target/`（近期量到 ~1.6 GB）。

`scripts/clean-target.ps1 -Mode report` 會把這兩個的大小一併列出（僅供參考），但 `debug` / `all` / `sweep` 只動本 repo 根目錄的 `target/`。如果使用者想連這兩個一起清，用同樣的 mode 手動下指令：

```powershell
# maple-oracle
cargo clean --manifest-path 'tools/maple-oracle/Cargo.toml'                       # all
Remove-Item -Recurse -Force 'tools/maple-oracle/target/debug'                     # debug only

# MapleLib-Rust（符號連結目標）
$mlrt = 'C:\Users\cl202\Desktop\MapleLib-Rust'
cargo clean --manifest-path "$mlrt\Cargo.toml"                                     # all
Remove-Item -Recurse -Force "$mlrt\target\debug"                                   # debug only
```

## `npm run tauri -- dev` 卡白畫面的常見成因（清 target 之外）

如果使用者的主訴是「開發模式視窗長時間白畫面」，target/ 太大通常只是**放大**問題，真正常見的根因是下列兩者，**清 target 前先檢查**：

1. **`vite.config.ts` 缺少 `server.watch.ignored`**：Tauri 官方模板都會加
   `watch: { ignored: ["**/src-tauri/**"] }`，避免 Vite 的 chokidar 監看 `src-tauri/target`
   （debug build 動輒上萬個檔案）。缺了這行，dev server 啟動要掃過整個 target/，
   且 cargo 每次寫入編譯產物都會觸發不必要的檔案事件。**這是本專案已修過的問題**，
   之後改動 `vite.config.ts` 時不要把這段拿掉。
2. **Windows Defender 即時保護沒有排除專案資料夾**：`Get-MpPreference` 若列出的
   `ExclusionPath` 不含這幾個專案根目錄（本專案、`Harepacker-MPST`、`MapleLib-Rust`），
   代表每一次 cargo 寫檔都會被即時掃描，對上萬個小檔案的 `target/debug/` 影響非常大。
   加排除需要系統管理員權限，且屬於安全性設定變更，**執行前必須先徵得使用者同意**，
   不要自作主張執行 `Add-MpPreference`。建議指令（供使用者確認後執行）：
   ```powershell
   Add-MpPreference -ExclusionPath 'C:\Users\cl202\Desktop\經典服查看工具'
   Add-MpPreference -ExclusionPath 'C:\Users\cl202\Desktop\Harepacker-MPST'
   Add-MpPreference -ExclusionPath 'C:\Users\cl202\Desktop\MapleLib-Rust'
   ```

## 執行流程

### 1. 先 report

```bash
npm run clean:report
```

如果 target/ 不存在或 < 1 GB，告訴使用者目前已經很乾淨，**不要清**，直接結束。

### 2. 決定 mode

依照 report 結果與使用者語意挑 mode。除非使用者已經明確說了哪一個，否則用 AskUserQuestion 詢問，並把推薦選項放第一個並加「(Recommended)」：

- **`debug` (預設推薦)**：保留 `target/release/`，砍 `target/debug/`。風險最低，最常用。
- **`sweep`**：留住近期 build cache，只清 30 天沒碰的。適合定期維護；如果使用者沒裝 `cargo-sweep` 腳本會給出安裝提示，這時退回 `debug`。
- **`all`**：等於 `cargo clean`。下次冷啟動會慢 5–15 分鐘，但最乾淨。

### 3. 執行清理

跑對應的 npm script。**不要直接 `Remove-Item` 或 `cargo clean`**，一律透過 `scripts/clean-target.ps1`，因為腳本會順手清孤兒 `*-check/` 並再次 report。

### 4. 順手清根目錄殘留

清完 target/ 後檢查根目錄是否有殘留的 `vite-*.log`（通常都是 0 KB 的舊 dev server log）：

```powershell
Get-ChildItem -Path . -File -Filter 'vite-*.log' | Remove-Item -Force
```

只清空檔或 < 1 MB 的；超過 1 MB 先給使用者看再決定。

### 5. 驗證

```powershell
$bytes = (Get-ChildItem -Path . -Recurse -Force -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum
Write-Host ('Total project size: {0:N2} GB' -f ($bytes / 1GB))
```

## 失敗處理

- `cargo clean` / `Remove-Item` 報「file in use」：通常是 `rust-analyzer`、IDE、`cargo` 正在跑，或 `msclassic.exe` / `index.exe` 還在執行。請使用者關閉編輯器、停止背景 build 或關閉正在跑的 app，再重跑。**不要** `taskkill` 對方的 process。
- `cargo-sweep` 不存在且使用者選了 sweep：印出安裝指令 `cargo install cargo-sweep`，並退回 `debug` mode 詢問是否改跑那個。
- 清完後磁碟空間沒明顯下降（< 10% 改變）：報告差異並提示可能是 Windows 還沒回收（重開機或執行 `Optimize-Volume` 可能有幫助）。

## 報告

完成後告訴使用者：

- 清理前、清理後的 `target/` 總大小（GB），以及釋出多少 GB。
- 使用了哪一個 mode、為什麼。
- 如果有跳過什麼（例如使用者沒裝 cargo-sweep 改用 debug mode）。
- 不要建議使用者 commit — 這個 skill 不會修改任何受 git 追蹤的檔案（`vite.config.ts` 的修正除外，那是另一件事，須另外請示）。
