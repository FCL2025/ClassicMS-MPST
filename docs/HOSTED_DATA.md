# 雲端託管資料計畫（免本機遊戲資料夾）

> 狀態：**M1 完成**（2026-08-04 pipeline 全量跑完，見「M1 實測結果」）
> 目標：Web 版使用者打開網頁即可瀏覽全部遊戲資料，不再需要選擇本機資料夾。

## 背景

目前 Web 版要求使用者選擇自己電腦裡的遊戲資料夾，bundle 複製進 OPFS 後由
WASM 核心本機解析。這對沒有安裝遊戲（或裝在非 Windows 裝置）的使用者是一道
高門檻。本計畫改為：離線預先把 bundles 解析成「衍生資料集」，託管到
Cloudflare，前端純 HTTP 按需抓取。

關鍵決策（2026-08-04 分析結論）：

- **不用 D1 存素材**。D1 是關聯式資料庫，不適合 GB 級二進位資產。
  素材放 **R2**（免 egress 流量費，免費額度 10GB 儲存）；
  D1 只視需要放查詢型索引（資料量小時連 D1 都可省略，索引直接放 R2 當靜態檔）。
- **不託管原始 bundles**，只託管預處理後的衍生資料（WebP 圖片 + JSON）。
  原始 bundles 沒有必要暴露，也能減少體積。
- 前端 API 介面（`src/api.ts` 的 export 契約）不變，新增一個 hosted 實作，
  與 `api.web.ts`（本機資料夾）並存，部署時二選一或讓使用者切換。

## 資料量評估（2026-08-04 實測）

以本機 `out/` 既有匯出產物與 catalog 快取測量：

| 資料類別 | 檔案數 | 原始大小 | 託管形式 | 預估託管大小 |
|---|---|---|---|---|
| Sprite sheets（全遊戲） | 14,027 PNG | 2.18 GB | WebP lossless | **約 1.0 GB** |
| UI sprites（已切單圖） | 1,619 PNG | 45 MB | WebP lossless | 約 20 MB |
| Catalog 樹索引 | 1 JSON | 4.5 MB | JSON（可再切分） | 約 4.5 MB |
| 文字 / 字串表 JSON | 數百 | 5.9 MB | JSON | 約 6 MB |
| 音效 | 未測量 | 未測量 | 原格式或 AAC | TBD |
| **合計** | | | | **約 1.1 GB** |

WebP 壓縮率實測（隨機抽樣 30 張 sheet，`cwebp`）：

- lossless：PNG 的 **45.4%**
- q90 lossy（alpha_q 100）：PNG 的 38.9%（像素圖有損，預設採 lossless）

推論：

- 全量資料集約 1.1 GB，**R2 免費額度（10GB）綽綽有餘**。
- 使用者單次瀏覽只抓看得到的 sheet（平均單張 163 KB PNG → 約 75 KB WebP），
  加上 edge cache，一般 session 流量是 MB 級，體感可接近本機。
- sheet 平均不大，「整張 sheet 下載 + 前端 canvas 切圖」可行
  （canvas→rect 映射已解出，見 README 已知資料限制），不必預切 70 萬張單幀。
  例外：圖鑑 / tooltip 等列表情境會同時顯示大量跨 sheet 的 icon，
  這類「熱點素材」適合預切單圖。

## 目標架構

```
離線 pipeline（開發機，遊戲更新後重跑）
  遊戲 bundles
    ├─→ sprite sheets → WebP（lossless）        ─┐
    ├─→ 熱點素材（item/mob/skill icon）→ 預切 WebP ├─→ R2 bucket
    ├─→ catalog 樹 + canvas→rect 映射 → JSON     ─┤   （key 前綴帶資料版本）
    ├─→ 文字 / tooltip / 圖鑑 / 技能資料 → JSON   ─┘
    └─→ index.json（目前資料版本 + 清單）

線上（Cloudflare）
  R2 bucket ◄── Workers API（快取標頭、CORS、版本路由）
     ▲
  Web 前端（hosted 版 api 實作）
     ├─ 節點樹 / 搜尋 / tooltip：fetch JSON
     ├─ 圖片：fetch sheet WebP + rect，canvas 切圖；熱點圖直接 <img>
     └─ Service Worker：已看過的資源本機快取
```

## 離線 pipeline 設計

新增 Rust 工具（`src-tauri/examples` 或獨立 binary），輸入 bundle 目錄，
輸出到 `out/hosted/<版本>/`：

- [x] `sheets/`：全部 sprite sheet（11,535 atlas 頁）→ PNG 後以 `scripts/hosted_to_webp.sh`
      平行轉 lossless WebP
- [ ] `icons/`：預切熱點素材（物品 icon、怪物帧、技能 icon、商城圖）
- [x] `tree/`：節點樹 JSON，依 img 切分（11,955 個檔）；canvas 的 rect/origin
      metadata 直接內嵌在 tree JSON 的 canvas 節點上（`canvas: {sheet,page,x,y,w,h,ox,oy}`），
      不需要獨立 rects/ 檔
- [x] `rects/`：併入 tree JSON（見上）
- [x] `strings/`：文字表（70 個 TextAsset 原樣落地）
- [x] `features/`：怪物圖鑑、技能書、商城、紙娃娃清單等衍生 JSON
      （現有 monster_demo / skill_dump 等邏輯可直接複用）
- [ ] `audio/`：音效檔（先測量體積再決定格式）
- [x] `index.json`：資料版本、各類數量與位元組統計（未含逐檔 hash，上傳差異靠
      rclone 自身的 checksum）

版本化：key 前綴用資料版本（如 `v2026-08-04/`），`index.json` 指向目前版本。
遊戲更新時上傳新版本前綴、切換 index，使用者端原子切換、舊版可留可清。
R2 上傳用 `wrangler r2 object put` 或 rclone（萬級檔案建議 rclone 分批）。

## 前端改造

- [x] 新增 `src/api.hosted.ts`：實作與 `api.ts` 相同的 export 契約，
      內部走 fetch + 前端 canvas 切圖（`getImage` 回 base64 的契約維持不變）
- [x] 獨立 `vite.config.hosted.ts`（alias 切換 + define 注入
      `__HOSTED_DATA_BASE__` / `__HOSTED_DATA_VERSION__`），
      `npm run dev:hosted` 一鍵起資料伺服器 + vite，`npm run build:hosted` 建到 `dist-hosted/`
- [x] Service Worker：快取 R2 資源（Cache API，版本前綴即天然快取鍵）
- [x] 首屏：進站直接載入 catalog + index 即開（`initialDataDir` 直接回標籤，
      免選資料夾）；熱門功能資料屬 M3
- [ ] 列表預取：瀏覽某分類時背景預取該分類其餘 sheet
- [x] 保留「讀本機資料夾」作為進階選項（可用自己版本的資料）：
      `api.hosted.ts` 改為雙後端委派層（雲端 `hosted/backend.ts` + 本機 `api.web.ts`），
      header 右上角「雲端資料 / 本機資料夾」分段切換器，偏好存 localStorage、預設本機資料夾

## UX 目標

- 首次進站到可互動 < 3 秒（只抓 catalog 頂層 + index，KB～低 MB 級）
- 點節點看圖：sheet 未快取時 < 500ms（edge cache 命中）；已快取即時
- 重複造訪：Service Worker 命中，接近桌面版體感

## 成本估算（Cloudflare 免費額度）

- R2 儲存 10GB：資料集實測約 1.5GB（見 M1 實測結果）→ 免費
- R2 Class B（讀）每月 1,000 萬次免費：個人工具站級流量大概率免費
- Workers 每日 10 萬請求免費：同上
- 上傳一次性約 1.5 萬個 Class A 操作（免費額度每月 100 萬）→ 免費

## 風險與注意事項

- **版權**：託管 = 公開再散佈 Nexon 遊戲資產，有 DMCA 下架風險。
  現行「讀本機檔案」模式無此問題。此風險已在決策時確認過，仍記錄在此。
- 遊戲更新後需重跑 pipeline 並上傳差異；流程要盡量一鍵化。
- 音效實測 277MB（2,767 個 Ogg Vorbis），全量收錄；Ogg 在 Safari 的
  `<audio>` 不支援（與既有本機資料夾網頁版相同的限制），如未來要支援
  Safari 再評估轉 AAC。
- R2 公開 bucket 無法擋爬蟲全量下載；如需基本防護可在 Workers 加
  Referer 檢查或簽發短效 token（但本就打算公開，優先級低）。

## 里程碑

1. [x] **M1 資料管線**：pipeline 產出完整 `out/hosted/<版本>/`（sheets WebP +
       catalog JSON + rects + strings），本機驗證體積與正確性
2. [x] **M2 前端 hosted 模式**：`api.hosted.ts` 跑通節點瀏覽 + 圖片預覽
       （本機起靜態伺服器指向 `out/hosted` 即可開發，不必先上 R2）
3. [x] **M3 功能資料**：tooltip、怪物圖鑑、技能書、商城、紙娃娃接上 hosted 資料
4. [x] **M4 部署**：R2 bucket + Workers API + Service Worker 快取上線
       （鷹架與本機驗證完成；實際 deploy 待 Cloudflare 授權，見「M4 部署步驟」）
5. [x] **M5 音效**：測量、轉檔、接上
6. [ ] **M6 更新流程**：遊戲更新 → 一鍵重跑 pipeline + 差異上傳腳本

## M1 實測結果（2026-08-04）

工具：`src-tauri/examples/hosted_export.rs`（預設 example，納入日常
fmt/clippy/test；catalog 快取放 `out/hosted/.catalog-cache/`，重跑秒開）。

```powershell
cargo run -p msclassic --release --example hosted_export -- `
  "C:\Program Files\Gamania\maplestory_classic\Maplestory_Classic_Data\StreamingAssets\aa\w" `
  out/hosted --version v20260804
bash scripts/hosted_to_webp.sh out/hosted/v20260804   # PNG → lossless WebP（需 cwebp）
```

全量跑 **81 秒**（不含 WebP 轉檔，平行轉檔約 9 分鐘），產出 `out/hosted/v20260804/`：

| 類別 | 檔案數 | 大小 | 備註 |
|---|---|---|---|
| `sheets/`（WebP） | 11,535 | **750 MB** | PNG 2.2GB → WebP，壓到 34%，優於抽樣估的 45% |
| `textures/`（WebP） | 3,191 | 30 MB | spriteset / atlas 等非 sheet Texture2D |
| `tree/`（JSON） | 11,955 | 658 MB | 每 img 一檔，canvas rect/origin 已內嵌；按需抓取，HTTP 壓縮後約再小一個量級 |
| `strings/`（JSON） | 70 | 5.9 MB | |
| `catalog.json` | 1 | 408 KB | 精簡版（路徑 → class_id） |
| **合計** | **26,752** | **約 1.5 GB** | R2 免費 10GB 內綽綽有餘 |

正確性：

- canvas 節點 744,172 個中 **743,691（99.94%）** 成功解析 rect，與桌面版覆蓋率一致
- 抽查 sheet WebP 解碼後畫面正確；`sheets/Character/Coat/01040147_0` 與舊
  `dump_sprites` 產物一致
- 唯一失敗：`Item/Consume/0261` WZJS 解碼報「slot5 應為空但 count = 10」——
  這是解析器本身的邊界 case（桌面版同樣開不了這個 img），不是 pipeline 問題，
  之後可獨立修
- 31 個 sheet key（`Item/Pet/5000xxx`）沒有對應 WZJS img，atlas 頁仍照常匯出，
  只是沒有 tree 可查

## M2 實測結果（2026-08-04）

- `npm run dev:hosted` 一鍵啟動（`scripts/dev_hosted.mjs` = 資料伺服器
  `scripts/serve_hosted.mjs`（port 8788，CORS `*`）+ vite（port 1422））；
  `npm run build:hosted` 建到 `dist-hosted/`。資料根與版本由
  `VITE_HOSTED_DATA_BASE` / `VITE_HOSTED_DATA_VERSION` 環境變數覆寫
  （預設 `http://localhost:8788` / `v20260804`）。
- 驗證全過：`npm run build`（桌面）、`npm run build:hosted`、curl 靜態端點
  （MIME/CORS/404/防穿越）、node 端對端（openDataDir→listChildren→getNode→
  searchNodes→UOL 解析）。
- 瀏覽器實機驗證：URL hash 直接導航 `#/Character/00002000/walk1/0/body` 與
  `#/Character/00002000/stabTF/0/body`（UOL），切圖方向與位置正確。
- 目前行為：資產樹瀏覽 / 搜尋 / 圖片預覽 / TextAsset 檢視完整可用；
  怪物圖鑑、技能、紙娃娃、商城顯示「hosted 模式尚未支援…（M3）」；
  匯出與音效同樣提示尚未支援。
- M2 刻意取捨：searchNodes 只搜 catalog 路徑與「已載入過」的 tree 節點
  （不主動抓新 tree，避免爆量下載）；字串名稱搜尋待 M3 隨功能資料一起補。

## M3 實測結果（2026-08-04）

分工原則（依資料性質）：**清單類**（要掃多個 img 才能組出來的）由 pipeline 預算，
**單物件明細**由前端按需從 tree/strings 推導。

- pipeline（hosted_export 新增 features 階段，`--skip-features` 可跳過）：
  `features/` 九檔——monster_book.json（343 隻，含 drops）、skill_book.json
  （82 個 job 分組，category 已分類）、commodity.json（692 件，含 iconBase64）、
  avatar_equipment.json（7,615 件）、avatar_meta.json（254 動作 + 8 膚色）、
  pet_list.json（寵物清單，含動作清單）、pet_equip_list.json（寵物裝備清單，
  含支援寵物 id）、action_delays.json（254 個動作總延遲，供技能詳情）。
  全部直接呼叫 Rust 既有的 pub 聚合函式產生，與桌面版同源。
- 前端：`src/hosted/core.ts`（從 api.hosted.ts 抽出的共用機制：tree/strings 快取、
  sheet 切圖、UOL 解析）+ 五個功能模組：
  - `tooltip.ts`：物品 tooltip（by path / by id），by-id 支援全部類別
    （移植 commodity.rs 的 item_path_candidates 映射；比 Rust 版只查 Character 更廣）
  - `monster.ts`：圖鑑清單（features）+ 怪物明細（stats/frames 切圖/drops+icon/
    出沒地圖，全部前端推導）+ 商城清單（features）
  - `skill.ts`：技能清單（features）+ 技能詳情（levels/actions/actionDelays）
  - `avatar.ts`：紙娃娃合成完整移植（ZMAP 149 層、錨點對齊、hairCover/hairShade、
    face 獨立幀），**與 Rust avatar_smoke 同參數對照逐行一致**
  - `pet.ts`：寵物紙娃娃（清單走 features；幀合成 = 原點對齊，本體 +
    寵裝兩層，寵裝幀 index modulo 對齊本體）
- api.hosted.ts 的 stub 已全部換成模組 re-export；`initialView` 新增
  `?view=tree|avatar|pet|monster|skill` 深連結（也方便無頭瀏覽器測試）。
- 瀏覽器實測（Edge headless + CDP）：怪物圖鑑明細（嫩寶 stats/10 掉落物含 icon/
  35 張出沒地圖）、技能頁（精靈的祝福，等級滑桿數值正確）、紙娃娃（裝備格 + 預覽
  渲染）、物品 tooltip（藍色毛帽）全部正確。
- 剩餘未接：匯出（M4 後評估）、音效（M5）。

## M5 實測結果（2026-08-04）

- pipeline 新增 audio 階段（`--skip-audio` 可跳過）：走訪 class_id==83 的
  AudioClip entry，`DataService::get_audio`（FSB5 → Ogg Vorbis 轉封裝）寫到
  `audio/<路徑>.ogg`。全量 **2,767 個、277MB**（平均 102KB；最大 BGM 4.7MB），
  fail=0，2.3 秒跑完。index.json 已帶 audio 統計。
- 格式維持 Ogg Vorbis（與桌面版/本機資料夾網頁版相同）；Safari `<audio>`
  不支援 Ogg 是既有限制，未來要支援 Safari 再轉 AAC（ffmpeg 已確認可用）。
- 前端 `getAudio` 接上：fetch `audio/<path>.ogg` → base64（契約不變）。
- 瀏覽器實測（CDP）：`decodeAudioData` 解出 88.74 秒立體聲（與 ffprobe 一致）；
  選取 `Sound/BgmUI/ShopBgm` 節點後播放器就緒（readyState 4、duration 正確）。
- 排障備忘：vite dev server 長時間 HMR 後曾出現 hash 導航失效的假性 bug，
  重啟 dev server 即恢復；CDP 測試同 origin 換 hash 不會重整頁面，
  測試 URL 要加 dummy query 強制 reload。

## M4 部署步驟（2026-08-04 鷹架完成）

產出：`workers/hosted.js`（/data/* → R2，含 .json→.json.gz 的 Content-Encoding 處理、
immutable 快取標頭、CORS）、`wrangler.hosted.jsonc`（assets=dist-hosted +
R2 binding DATA）、`public/sw-hosted.js`（/data/ cache-first，production 才註冊）、
`scripts/upload_hosted.mjs`（上傳資料集到 R2，細節見下方「資料上傳的速率限制」）、
`scripts/serve_hosted.mjs` 支援 .json.gz fallback。
vite.config.hosted.ts：build 時資料根預設同源 `/data`（dev 仍是 localhost:8788，
`VITE_HOSTED_DATA_BASE` 可覆寫）。

同次最佳化：pipeline 的 JSON 產物全部改 gzip（.json.gz）——tree/ 由 630MB 降到
**37MB**（17x），JSON 類合計 637MB→40MB；並清掉與 webp 重複的 PNG。
**資料集最終約 1.2GB**（webp 780MB + ogg 277MB + json.gz 40MB）。

使用者操作（一次性）：

1. `npx wrangler login`（瀏覽器授權 Cloudflare）
2. `npx wrangler r2 bucket create msclassic-data`
3. 建立 R2 API token 並填進 `.env`（見下方「資料上傳的速率限制」；沒填會退回很慢的路徑）
4. `npm run upload:hosted-data -- v20260804`（約 3 萬檔，中斷可直接重跑續傳）
5. `npm run deploy:hosted`（部署預覽 Worker 到 `msclassic-hosted.<帳號子域>.workers.dev`）
6. 網域（2026-08-04 已定案）：正式站 `classic.maplepstutorial.com` 直接換成
   hosted 版——根目錄 `wrangler.jsonc` 改為 hosted 設定（`main: workers/hosted.js`
   + dist-hosted + R2 binding），**Worker 名稱維持 `msclassic-viewer`**，
   既有 GitHub 串接與 custom domain 不用動；Cloudflare Build command 由
   `npm run build:web` 改為 `npm run build:hosted` 後，push 即自動部署正式站。
   舊的本機資料夾版（dist-web）退役，其功能由 hosted 版的來源切換器涵蓋；
   `wrangler.hosted.jsonc` 保留作 workers.dev 預覽環境。

### 資料上傳的速率限制（2026-08-08 最佳化）

`scripts/upload_hosted.mjs` 有兩條上傳路徑，差別是打哪個 API：

| 路徑 | 介面 | 速率上限 | 3 萬檔實測/估計 |
| --- | --- | --- | --- |
| S3 相容 API（預設） | `<account>.r2.cloudflarestorage.com` | 帳號層級沒有請求數上限；只有「同一個 key 每秒 1 次寫入」（我們每個 key 只寫一次） | 受限於上傳頻寬 |
| wrangler `r2 bulk put`（備援） | `api.cloudflare.com/.../r2/buckets/.../objects/...` | **1,200 requests / 5 分鐘 = 4 req/s（全帳號共用）** | **2 小時以上** |

REST 那條就是原本很慢的原因：Cloudflare REST API 全帳號硬上限 1,200 requests /
5 分鐘，超過之後**接下來 5 分鐘所有 API 呼叫都會被擋**（HTTP 429）。這是固定速率
限制，不是並發限制，所以調 `--concurrency` 完全沒用。Cloudflare 官方文件也直接寫
「高吞吐量的物件操作請改用 S3 相容 API 或 Workers API」。
參考：<https://developers.cloudflare.com/r2/platform/limits/#cloudflare-rest-api>

走 S3 路徑需要 R2 API token（一次性）：Dashboard → R2 → API → Manage API tokens
→ Create API token，權限 **Object Read & Write**（範圍可只給 `msclassic-data`），
把 Account ID / Access Key ID / Secret 填進專案根目錄 `.env`（格式見 `.env.example`，
`.env` 已被 gitignore）。沒設定時腳本會自動退回 REST 路徑，並主動把速率壓在
4 req/s 以下、429 後冷卻 320 秒，避免整個帳號被鎖。

其他行為：

- 續傳紀錄改成**每個檔案**一筆（`out/hosted/.upload/uploaded-<version>.json`，
  記 `大小:mtime`），中斷後重跑只補沒傳成功的；`--force` 可忽略。
- S3 路徑並發自適應：從 16 起跳，遇到 429/5xx 折半並全體冷卻，連續成功再慢慢加回
  到 `--concurrency`（預設 48）。
- `--sample N` 在全部待傳檔案裡等距取樣，給正式上傳前的小範圍實測用。
- 進度列會即時顯示 檔/s、MB/s 與 ETA。

簽章邏輯抽在 `scripts/lib/r2_s3.mjs`（SigV4 + ListObjectsV2），已用 AWS 官方
PUT Object 參考向量驗證過，沒有拉任何 AWS SDK 依賴。

### 刪除舊版本資料集

`scripts/r2_delete_version.mjs`（`npm run delete:hosted-data -- <version>`）走 S3 的
DeleteObjects，一次 1,000 個 key，三萬個物件只要 30 個請求。預設是 dry-run，加
`--yes` 才真的刪。有保護：目標版本若正是 `vite.config.hosted.ts` 現在指向的那個會直接
拒絕執行（`--force` 可繞過，正常不該用到）。

**不要**用 `wrangler r2 object delete` 一個個刪——那會走 REST API，撞回 4 req/s 的牆。

一個版本約 1 GB，R2 免費額度 10 GB，保留 1～2 個舊版本當退路沒問題。

### 換版本號時要改哪裡（重要）

`__HOSTED_DATA_VERSION__` 是**建置期常數**（`vite.config.hosted.ts` 的預設值，可用
`VITE_HOSTED_DATA_VERSION` 環境變數覆寫），編進 bundle 成為 `src/hosted/core.ts` 的
`DATA_ROOT`。`workers/hosted.js` 只把 `/data/<version>/<path>` 原封不動當 R2 key，
**沒有任何版本判斷、不會自動挑最新的**（版本目錄裡的 `index.json` 是統計檔，不是版本索引）。

所以「上傳新版本」和「網站切到新版本」是兩件獨立的事：只上傳，線上站仍然讀舊的。
順序必須是 **資料全部上傳完 → 改 `vite.config.hosted.ts` → 重新 build + deploy**，
這樣切換是原子的，中間不會出現半新半舊的破圖狀態。整套流程見
`.claude/skills/redeploy-hosted-data/SKILL.md`。

本機驗證已過：gzip 資料集端到端（tooltip、怪物明細、切圖）、serve_hosted 的
.json→.json.gz fallback（curl --compressed 比對一致）、upload dry-run
（44,252 筆 manifest）、`wrangler deploy --dry-run`（binding 正確）。
Service Worker 僅 production 註冊，尚未有正式環境可驗。
