# 專案指引

## Commit 慣例（使用者要求，每次 commit 都要做）

1. **更新版本號**：依變更幅度遞增 semver，四個位置要同步：
   - `package.json` 的 `version`
   - `package-lock.json` 的兩處 `version`（根層與 `packages[""]`）
   - `src-tauri/Cargo.toml` 的 `version`（改完跑一次 `cargo check` 讓 `Cargo.lock` 同步）
   - `src-tauri/tauri.conf.json` 的 `version`
   - （`web-wasm/Cargo.toml` 的版本號不參與發布，不用動）
2. **更新 `CHANGELOG.md`**：在最上面（`## [未發布]` 之下）新增一段
   `## vX.Y.Z（YYYY-MM-DD）`，用條列式、一行一件事，寫給使用者看得懂的話。
   應用內的「更新日誌」彈窗（`src/components/ChangelogDialog.tsx`）會直接解析
   這個檔案的 `## ` 標題與 `- ` 條目，格式不要偏離。
   只記錄與軟體本身功能相關、使用者有感的變更；部署方式、網域、Discord 連結、
   介面風格來源等周邊或基礎建設事項不要寫進更新日誌。
3. **commit 訊息**：用中文、簡述本次加了什麼／修了什麼（參考 git log 既有風格）。

## 驗證

提交前至少跑過：

- `cargo fmt --all -- --check && cargo clippy -p msclassic --all-targets -- -D warnings && cargo test -p msclassic --all-targets`
- `npm run build`（桌面前端）；動到 wasm 或 web 版時加跑 `npm run build:web`
