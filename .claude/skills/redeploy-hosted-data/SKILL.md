---
name: redeploy-hosted-data
description: Use when the user asks to 重跑/更新/重新上傳雲端資料庫、出新版資料集、重新部署線上版、redeploy hosted data — e.g. 「重跑雲端資料庫」、「更新線上資料」、「出新的資料版本」、「重新部署 hosted」. ALWAYS starts by asking the user to choose between (A) re-uploading the existing version and (B) producing a new dated dataset, then runs pipeline → upload → version switch → deploy → delete-old-version with confirmation gates.
---

# 雲端資料庫重跑（pipeline → 上傳 → 切版本 → 部署 → 清舊版）

## 步驟 0（一定要先做）：問使用者要走哪一種

⛔ **這是強制的第一步。不要自己猜、不要從上下文推斷就直接開跑**——兩條路差很多，
猜錯的代價是白傳 1 GB 或漏掉版本切換導致線上還是舊資料。

用 **AskUserQuestion** 提問，兩個選項照抄下面的文字：

> **問題**：這次要走哪一種？
> **header**：重跑方式
>
> - **選項 A「只重新上傳現有版本」** — 遊戲資料沒變，只是要把 `out/hosted/<現有版本>/`
>   重新推上 R2（例如上次傳到一半中斷、或懷疑檔案有缺）。不重跑 pipeline、不換版本號、
>   不用重新部署。
> - **選項 B「出新日期的資料集」（完整流程）** — 遊戲更新了或要出新版本。會重跑本地
>   pipeline 產生 `out/hosted/v<新日期>/`、上傳、把前端讀取的版本切過去、重新部署，
>   最後可選擇刪掉舊版本。整套約 15～20 分鐘。

拿到答案後：

- **選 A** → 只做「流程 A」那一節，做完就結束。
- **選 B** → 走步驟 1 到 7。

### 流程 A：只重新上傳現有版本

1. 確認版本號：讀 `vite.config.hosted.ts` 裡 `__HOSTED_DATA_VERSION__` 的預設值，
   跟使用者確認就是要傳這個。
2. 如果是「懷疑檔案有缺、想整包重傳」，要刪掉續傳紀錄
   `out/hosted/.upload/uploaded-<version>.json`，或改用 `--force`；否則會全部被跳過。
3. `npm run upload:hosted-data -- <version>`（上傳的細節與失敗處理見下方「步驟 3」那一節）。
4. **不要**改 `vite.config.hosted.ts`、**不要**重新部署、**不要**刪任何舊版本。
   版本號沒變，線上站讀到的就是剛覆寫上去的內容。

---

以下是 B 的完整流程。**每個「⛔ 確認點」都要停下來等使用者回覆，不要自己往下衝。**

## 關鍵背景：版本是建置期常數

`__HOSTED_DATA_VERSION__` 寫死在 `vite.config.hosted.ts`（預設值那行），編進 JS bundle 成為
`src/hosted/core.ts` 的 `DATA_ROOT`。`workers/hosted.js` 只是把 `/data/<version>/<path>` 原封不動
當成 R2 key，**完全沒有版本判斷、不會自動選最新的**。

所以：**只上傳新版本資料，線上站還是讀舊的**。一定要改 `vite.config.hosted.ts` 再重新 build + deploy
才會切過去。這是刻意的設計——資料全部傳完才切版本，中間不會出現半新半舊的破圖狀態。

---

## 步驟 1：決定版本號、重跑本地 pipeline

版本號用 `vYYYYMMDD`（今天日期）。先確認目標目錄狀況：

```powershell
Get-ChildItem out/hosted -Directory
```

⛔ **確認點**：如果 `out/hosted/<新版本>/` 已經存在，一定要問使用者要不要刪掉重跑。
`hosted_export.rs` 只做 `create_dir_all`，**不會清空目錄**，上一輪的殘留檔案會留著並被一起上傳。

跑 pipeline（bundle 路徑要跟使用者確認，不要沿用文件裡的範例路徑就直接跑）：

```powershell
cargo run -p msclassic --release --example hosted_export -- `
  "<遊戲 StreamingAssets\aa\w 路徑>" `
  out/hosted --version v20260810
bash scripts/hosted_to_webp.sh out/hosted/v20260810
```

- 第一段約 81 秒（`out/hosted/.catalog-cache/` 讓重跑秒開，**不要刪這個目錄**）。
- 第二段 PNG → lossless WebP 約 9 分鐘，需要 `cwebp`。
- 兩段都用背景任務跑並等完成通知。
- `--version` 省略時預設是「今天日期」，但還是明寫比較不會出錯。

跑完核對產出（對照上一版的量級，差太多代表 pipeline 有問題，要先查清楚）：

```powershell
Get-ChildItem out/hosted/v20260810 -Recurse -File | Measure-Object Length -Sum
```

參考值（v20260804）：29,527 檔 / 1.03 GB，其中 `sheets/` 11,535 檔 720 MB、`audio/` 2,767 檔 277 MB、
`tree/` 11,955 檔 37 MB、`textures/` 3,191 檔 22 MB。

## 步驟 2：小範圍實測上傳

正式全量傳之前先驗一次，確認憑證與連線都正常：

```powershell
node scripts/upload_hosted.mjs v20260810 --sample 40
```

`--sample` 會在全部檔案裡等距取樣，涵蓋不同目錄與副檔名。

## 步驟 3：全量上傳到 R2

```powershell
npm run upload:hosted-data -- v20260810
```

- 走 S3 相容 API，約 1 GB 幾分鐘內跑完。中斷直接重跑會續傳（manifest 在
  `out/hosted/.upload/uploaded-<version>.json`，按檔案記 `大小:mtime`）。
- 如果印出「找不到 R2 API token…退回 wrangler REST 路徑」，**先停下來**：那條路受
  Cloudflare REST API「1,200 requests / 5 分鐘」限制，三萬檔要跑兩小時以上。
  請使用者照 `.env.example` 建 R2 API token 填進 `.env`，不要就這樣讓它跑。
- 結束要 exit code 0 且沒有列出失敗檔案。有失敗就重跑（會自動跳過已完成的）。

## 步驟 4：把專案讀取的版本換成新的

改 `vite.config.hosted.ts` 裡 `__HOSTED_DATA_VERSION__` 的預設值：

```ts
__HOSTED_DATA_VERSION__: JSON.stringify(
  process.env.VITE_HOSTED_DATA_VERSION ?? "v20260810",   // ← 改這裡
),
```

順手更新 `docs/HOSTED_DATA.md` 裡提到舊版本號的地方。

## 步驟 5：部署

⛔ **確認點**：這是對外操作，動到正式站，一定要先問過使用者。確認要部署哪一個：

- **預覽站**（`msclassic-hosted.<帳號子域>.workers.dev`）：`npm run deploy:hosted`
- **正式站**（`classic.maplepstutorial.com`）：commit + push 到 master，Cloudflare GitHub 串接會自動
  build（`npm run build:hosted`）+ deploy。**不要**手動對根目錄 `wrangler.jsonc` 跑 wrangler deploy。

建議先部署預覽站驗過再推正式站。

## 步驟 6：確認線上正常

⛔ **確認點**：請使用者實際開站點看過再繼續。至少要確認：

- 資產樹能展開、圖片能顯示（代表 `catalog.json` + `sheets/` 有讀到）
- 怪物圖鑑 / 技能 / 任務頁能開（代表 `features/` 有讀到）
- 音效能播（代表 `audio/` 有讀到）

也可以直接打幾個資料端點確認 200：

```powershell
curl.exe -I https://classic.maplepstutorial.com/data/v20260810/catalog.json
```

## 步驟 7：移除舊版本資料集

⛔ **確認點**：這是不可逆的刪除，一定要使用者明確說可以才做。

先 dry-run 看會刪什麼（`<舊版本>` = 被取代掉的那個，不是剛上線的新版本）：

```powershell
npm run delete:hosted-data -- <舊版本>
```

把物件數量報給使用者，確認後才真的刪：

```powershell
npm run delete:hosted-data -- <舊版本> --yes
```

- 腳本有保護：如果目標版本就是 `vite.config.hosted.ts` 現在指向的那個，會直接拒絕執行。
- 走 S3 的 DeleteObjects，一次 1,000 個 key，三萬個物件只要 30 個請求。
  **不要**改用 `wrangler r2 object delete` 一個個刪，那會撞回 REST API 的 4 req/s 限制。
- 本地舊版本目錄 `out/hosted/<舊版本>/` 要不要一起刪，另外問使用者（那是本地磁碟空間，跟線上無關）。

R2 免費額度 10 GB，一個版本約 1 GB，所以保留 1～2 個舊版本當退路是可以的，不必急著刪。

---

## 通用注意事項

- 每個指令都要 exit code 0；任何一步失敗就停下來報告，**不要繼續跑下一步**。
- pipeline、WebP 轉檔、上傳、build 都是分鐘級，用背景任務執行並等完成通知。
- 把每步的關鍵輸出（檔案數/大小、上傳統計、deploy URL）摘要給使用者，不要只說「完成了」。
- 相關檔案：`scripts/upload_hosted.mjs`、`scripts/r2_delete_version.mjs`、`scripts/lib/r2_s3.mjs`、
  `vite.config.hosted.ts`、`workers/hosted.js`、`docs/HOSTED_DATA.md`。
