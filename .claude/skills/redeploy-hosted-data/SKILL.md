---
name: redeploy-hosted-data
description: Use when the user asks to 重跑/更新/重新上傳雲端資料庫、重新部署線上版、redeploy hosted data — e.g. 「重跑雲端資料庫」、「更新線上資料」、「重新部署 hosted」. Runs `npm run upload:hosted-data` then `npm run deploy:hosted` in order and verifies both succeeded.
---

# 雲端資料庫重跑（hosted 資料上傳 + 線上版部署）

使用者要求「重跑雲端資料庫」時，依序執行以下兩個指令（順序固定，不可對調）：

1. **`npm run upload:hosted-data`** — 把本機 `out/hosted/<version>/` 資料集上傳到 Cloudflare R2（`scripts/upload_hosted.mjs`）。
2. **`npm run deploy:hosted`** — 重新建置 hosted 前端（`build:hosted`）並部署 Worker + 靜態站（`wrangler deploy -c wrangler.hosted.jsonc`）。

## 確認方式

- 兩個指令都要 exit code 0；任何一個失敗就停下來報告錯誤，不要繼續跑下一個。
- `upload_hosted.mjs` 結尾會印上傳統計（檔案數/跳過數）；`wrangler deploy` 會印部署後的 URL。把這兩段摘要貼給使用者。
- 兩個指令都可能跑幾分鐘（資料集上傳量大、wasm 建置），用背景任務執行並等完成通知。

## 注意事項

- 這是**對外操作**（動到正式線上環境），只有使用者明確要求時才執行。
- 如果這次變更牽涉到資料內容本身（新 feature 資料、新版遊戲資料），單純上傳是不夠的——要先用 hosted pipeline 重產 `out/hosted/<version>/`（`src-tauri/examples/hosted_export.rs`，用法見 `docs/HOSTED_DATA.md`），再跑上面兩步。不確定時先問使用者資料是否需要重產。
- 資料集版本目錄（如 `v20260804`）若要換新，需同步前端 define 與 wrangler 設定，細節見 `docs/HOSTED_DATA.md`；不要自行改版本號。
