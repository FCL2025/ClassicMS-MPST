# 紙娃娃（角色試穿）功能計畫與進度追蹤

> 狀態：進行中　開始日期：2026-07-26
> 參考專案：`E:\_HA restart\WzComparerR2-Plus-2023-08-10` 的 `WzComparerR2.Avatar` 外掛（僅移植演算法，UI 全部重新設計）

## 目標

新增「紙娃娃」頁面：從頂部 header 切換（瀏覽 / 紙娃娃），紙娃娃模式下隱藏左側樹狀區。
版面左 20% / 右 80%：

- 左上：正方形畫布預覽（棋盤格背景、動畫播放）。初始 = 裸身（body+head，無裝備/髮型/臉型）。
- 左下：膚色 / 表情 / 動作選擇器 + 播放暫停；分隔線；已穿裝備清單（點擊切換顯示/隱藏即時影響預覽，附移除鈕）。
- 右：仿商城介面，主 tab 依部位分（帽子/上衣/套服/褲裙/鞋子/手套/披風/盾牌/武器/耳環/臉部飾品/眼部飾品/髮型/臉型），grid 卡片 + 分頁，點卡片穿上（同槽取代），已穿高亮。

## 從參考專案移植的核心機制

- **對齊**：每張 PNG 有 `map` 錨點（hand/head/navel...），各部位同名錨點互相咬合（對應 `AvatarCanvas.CreateBone/AppendBone/GenerateLayer`），不靠固定 offset。
- **圖層順序**：`z` 是字串圖層名；參考專案讀 `Base/zmap.img` 決定順序（索引大者先畫）。WZJS 資料無 zmap → 內嵌 v83 標準 zmap 排序表。
- **帽子遮髮**：HairCover 布林切換 `hairOverHead`/`backHairOverCape` 與 `hair`/`backHairBelowCap` 兩組貼圖；預設有裝帽子 → cover，左下「帽子遮髮」核取方塊可手動切換（不勾 = 髮型超出帽子）。
- **longcoat 互斥**：穿上衣/褲裙 → 套服隱藏；穿套服時若上衣/褲裙可見 → 套服隱藏。
- **body↔head 膚色連動**：`headID = 10000 + bodyID % 10000`。
- **動作/表情**：獨立幀序列各自播放再合成；head 找不到動作退回 `front`，hair 退回 `stand1`。

## 範圍外（本期不做）

騎寵/鞍座、技能特效、GIF/PNG 匯出、代碼匯入、多色武器 weaponIndex（先取預設貼圖）、椅子 sit 位移。

## 架構

- 核心邏輯寫在共享 crate `msclassic`（`src-tauri/src/avatar.rs`），桌面 Tauri 與 web-wasm 各加薄介面。
- 後端 3 個聚合 command：
  1. `get_avatar_meta` → 動作清單（掃 `Character/00002000`）、表情清單（face img）、膚色清單（存在的 `00002xxx` body id）。
  2. `get_avatar_equipment` → `[{id, name, slot}]`（不含圖；icon 前端 lazy 載入）。
  3. `get_avatar_frames(parts, action, emotion)` → `{ frames: [{ delay, flip, move, layers: [{ imagePath, x, y, zIndex }] }] }`，位置與圖層由後端算好。
- 前端 canvas 2D 疊圖 + rAF 依 delay 播放；圖走既有 `get_image`（LRU + 前端 Map 快取）。

## 進度

- [x] 參考專案分析（WzComparerR2.Avatar 渲染管線、z 排序、動作/表情、試穿邏輯）
- [x] 本專案現況調查（共享 crate、現有 API、UI 可複用元件）
- [x] 探查 Character WZJS 實際節點結構（z 字串集合、map 錨點、UOL、子目錄實名、名稱表涵蓋率）
- [x] 核心 `src-tauri/src/avatar.rs`（meta/equipment/frames + 骨骼對齊 + zmap 表 + 單元測試）
- [x] 桌面介面 `gui.rs` 三個 command + `api.ts` wrapper
- [x] Web 介面 `web-wasm/src/lib.rs` + `worker.ts` RPC + `api.web.ts`
- [x] 前端 `App.tsx` view 切換、`AvatarPage`/`AvatarPreview`/`AvatarEquipPanel`/`AvatarShop` + 樣式
- [x] 驗證：`cargo fmt/clippy/test`（61 測試全綠）、`npm run build`、`npm run build:web`、**桌面板人工比對（已截圖確認：裸身初始、成套試穿對齊正確）**、web 版抽驗（build 過，互動未驗）

## 驗證記錄（2026-07-26）

- 桌面版截圖驗證（`out/avatar_check_8.png` / `avatar_check_9.png`）：裸身初始狀態正確（無裝備/髮型/臉型）；成套試穿（髮 00030000 + 臉 00020000 + 上衣 01040036 + 褲 01060026 + 武器 01302000）對齊、圖層、持武手勢全正確；已穿清單眼形切換與移除鈕正常顯示；右側 14 部位 tab 數量與圖示載入正常。
- 驗證過程修了一個真 bug：`App.tsx` 紙娃娃掛載條件從 `avatarActivated` 改為 `avatarActivated || view === "avatar"`（否則初始就在 avatar view 時永不掛載）。
- 工作階段在 session 0，無法注入滑鼠點擊（SetCursorPos 被拒）；互動流程（點卡片穿上/再點脫下、長coat 互斥的 UI 連動）僅程式審查，未實機點擊驗證——留待使用者實機確認。
- `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` 對本環境的 WebView2 無效（port 未開），CDP 路線不可用。
- 名稱表缺名時顯示 `#id`（如 #01000036），屬預期 fallback。

## 修正記錄（2026-07-26，使用者回報）

- **問題 1：播放時人物亂跳（每幀像獨立的圖）**。原因：每幀都把 body canvas 放 (0,0)，各幀 origin 不同、無共同基準點。修法：`avatar_frames` 以第 0 幀 body origin 為固定基準，第 i 幀 shift = `origin_0 - origin_i + move`（幀無 body canvas 不位移；`move` 屬性加總）。smoke 實測 walk1 四幀 body origin 絕對位置完全一致 (19,32)。
- **問題 2：表情不跟身體動**。原因：face 獨立幀序列原本用「第 j%主幀數 個身體幀」的錨點算絕對位置，但前端 body/face 各自計時任意配對，i≠j 即錯位。修法：face 層改輸出**相對 brow 錨點的相對座標**（`-(origin+map_brow)`）；身體幀新增 `faceAnchor`（當幀 brow 絕對位置）與 `showFace`（幀 `face` 屬性）；前端 `composeLayers` 繪製當下即時配對，並把表情層按 z 與身體層交錯排序（瀏海正確蓋臉，修正原本 face 永遠畫最上的錯誤）。
- 複驗截圖（`out/avatar_fix_a/b/c.png`，walk1 播放連拍）：基準點穩定無跳動、臉跟頭走。

## 第二輪功能（2026-07-27）

1. **裝備 tab 移除數量**：tab 只留部位名。
2. **點裝開關**：`EquipItem` 新增 `cash` 旗標（讀各 img `info/cash`，非零即 true）；header 移除資產摘要，原位置放「點裝」switch（僅紙娃娃頁顯示），ON 時商店只列 cash 裝備，全濾光的部位顯示空態。實測：cash=true 4439/6957（63.8%，經抽查是經典服資料本身把低等裝也標 cash，非誤讀）；開關 ON 帽子 793→350 件。效能：首次掃描（逐 img 解碼 info）release 冷/熱皆約 1.8-2s；注意 `TREE_CACHE_CAP=32` 讓熱掃描無加速，日後可考慮在 DataService 加 equipment 整表快取。
3. **暫停鈕改版**：改為動作下拉右側的正方形圖示鈕（⏸/▶ 無文字）；暫停改為凍結當前幀（timeRef 記錄播放時間，再播放從凍結點繼續）。
4. **下載預覽圖**：預覽右上角絕對定位 ⬇ 鈕。暫停中直接下載當前畫面 PNG；播放中彈窗選 GIF 或 WebP。
   - 匯出工具 `src/components/avatarExport.ts`：與預覽共用合成 helper；lcm 合併時間軸（封頂 10s、30ms 步長、≤200 幀）、bbox 整數倍放大（≤512、寬高偶數）。
   - GIF：npm 套件 `gifenc@1.0.3`（唯一新依賴，純 JS；`src/gifenc.d.ts` 自補型別），保留透明。
   - 動畫 WebP：手寫 RIFF muxer（每幀 canvas toBlob 靜態 WebP → 取 ALPH+VP8/VP8L → VP8X+ANIM+ANMF 封裝，no-blend、偶數對齊）。
   - 實機驗證：WebView2 實際下載並解析檔案——GIF 330×462、120 幀、GIF89a、透明保留；WebP 330×462、120 個 ANMF、VP8X flags 0x12、RIFF size 與檔長一致。兩測試檔留在 `C:\Users\cl202\Downloads`（avatar_walk1_*.gif/webp，可刪）。
   - 注意：無使用者手勢的連續下載會被 WebView2 擋（測試時連發 GIF→WebP 第二個沒出現）；真實操作按鈕觸發不受影響。

## 修正記錄（2026-07-27，使用者回報）

- **臉型位置跑掉**：第二輪把預覽繪製抽進 `avatarExport.ts` 共用時，`composeFrame`/`computeBoundingBox` 退回舊邏輯（face 直接當絕對座標、疊最上），把第一輪「face 相對 brow + faceAnchor 即時配對 + z 交錯」的修復弄丟了。修法：把 `composeLayers`（faceAnchor 配對 + z 遞減合併）放回 `avatarExport.ts` 作為唯一合成入口，`composeFrame` 與 `computeBoundingBox` 都走它——預覽與匯出（PNG/GIF/WebP）從此不可能再分叉。
- **部分髮型圖示空白、穿上變光頭（如 00030160 一線頭）**：這類髮型 `default` 只有一張 z="hair" 的 canvas、沒有 `hairOverHead` 變體。兩個問題：① icon 路徑寫死 `default/hairOverHead` 取不到圖 → `loadAvatarIcon` 加 fallback 退用 `default/hair`；② 無帽遮髮規則把 z="hair" 當「帽下變體」藏掉，整個髮型被藏光 → `avatar_frames` 改為無帽時若該幀髮型 canvas 會全被藏掉則全部保留（有帽行為不變）。三張 canvas 匯出 PNG 實測確認都是真貼圖。
- **footer 麵包屑不跟紙娃娃選擇**：點商店卡片（穿上或脫下）時回報該裝備 img 路徑（如 `Character/Hair/00030160`），avatar view 的 footer 改顯示它；`slotImgPath` 對齊後端 `img_key_of`（飾品類都在 Accessory）。

## 第三輪功能（2026-07-27）

1. **髮型/臉型顏色**：商店列表同造型只顯示一張卡（預設色，即顏色 0）；左下「動作」下方新增左右並排的「臉型顏色 / 髮型顏色」下拉（支援滑鼠滾輪直接換色，不用點開）。穿上時套用選取色、換色時已穿部位即時換色（該造型無此色退回顏色 0）；未穿臉型時預設臉也跟著臉色走。邏輯集中在前端 `src/components/avatarColor.ts`。
   - **顏色編碼（對正式資料驗證）**：髮型末碼是顏色（0003XXX**0–7**，名稱依序 黑/紅/橙/黃/綠/藍/紫/褐）；臉型**百位數**是顏色（0002C**X**SS 的 X，即倒數第三碼，+100 一色，如 很專心臉型 = 00020016/00020116…00020716、女版系列 00021016–00021716，0005xxxx 臉同規則）。臉色 0–8，名稱依序 **黑/青/紅/綠/黃/藍/紫/粉/白**（使用者實機核對：20200=紅、20500=藍）；8 白色非每個造型都有，沒有該 id 就不顯示白色選項。注意臉型**不是**末碼（00020000–00020023 是連號的不同造型）；同名不同系列（如 企鵝臉型 00025023/00026029）是不同造型，分組要用「去掉顏色位的基底 id」而非名稱。
   - 髮型/臉型卡片顯示名去掉開頭顏色詞（如 黑色短中分 → 短中分）；title 保留全名+id。
2. **帽子遮髮（hairCover）核取方塊**：左下顏色選擇下方，預設勾選。對齊 WzComparerR2 `AvatarCanvas` 的 HairCover 過濾（`AvatarCanvas.cs:723-727`）：勾選 + 有戴帽 → 藏 `hairOverHead`/`backHairOverCape`/`backHair`；不勾 → 藏 `hair`/`backHairBelowCap*`（髮型超出帽子）。無帽時無效果。實作：`get_avatar_frames` 全鏈路（gui.rs / wasm lib.rs / api.ts / api.web.ts / worker.ts）加 `hair_cover` 參數，後端 `有效遮髮 = 有戴帽 && 勾選`。

## 實作摘要（2026-07-26 完成）

- 核心 `src-tauri/src/avatar.rs`（~1070 行）：4 個 pub 函式（`avatar_meta`/`avatar_emotions`/`avatar_equipment`/`avatar_frames`），內嵌 149 層 ZMAP 常數，7 個單元測試。smoke 實測：stand1 幀 0 共 7 層、head y=-33 在 body 上方、z 遞減正確。
- 資料特例：hairShade 的 UOL 別名連發（1,2,3..→"0"）需按解析後路徑去重，否則同圖疊畫 8 次。
- 接線：`gui.rs` 4 個 Tauri command、`api.ts` 4 wrapper + 型別、`web-wasm/lib.rs` 4 個 wasm 函式、`worker.ts` 4 個 RPC method、`api.web.ts` 對齊。
- 前端 4 新元件：`AvatarPage`（狀態容器：skin/action/emotion/playing/parts，longcoat 互斥與 visible 過濾在 `equip()` 與 frames effect）、`AvatarPreview`（canvas 合成、body/face 雙幀序列獨立計時 rAF、整數倍放大置中）、`AvatarShop`（14 部位 tab + grid + 動態分頁 + icon lazy 含 iconRaw 重試）、`AvatarEquipPanel`（三 select + 播放鈕 + 已穿清單點列切換顯示）。`App.tsx` 加 view tab，紙娃娃頁 lazy 掛載且切換不重置狀態。

## 風險與備忘

- **zmap 表**：內嵌 v83 標準表；若經典版圖層名有出入，對照實際 `z` 字串補表。
- **骨骼對齊**：walk/swing 等手部大動作最易破圖，以 stand1/walk1/swingO1 為對照基準。
- **裝備總量**：equipment 只回 id+name，icon lazy 載入避免一次數千張 base64。
- 探查發現記錄於下方「探查筆記」。

## 探查筆記

2026-07-26 以 `src-tauri/examples/avatar_probe.rs`（對正式遊戲資料）實測：

- **Character 子目錄**：Accessory(402)、Afterimage、Cap(793)、Cape(180)、Coat(443)、Dragon、Face(849)、Glove(259)、Hair(1927)、Longcoat(481)、Pants(392)、PetEquip、Ring(90，皆 0111/0112 開頭的名牌特效戒，不可穿）、Shield(50)、Shoes(425)、TamingMob、Weapon(979)。
- **Accessory id 前綴**：0101=臉部飾品(103)、0102=眼部飾品(77)、0103=耳環(49)、0112/0113/0114=墜飾/腰帶/勳章（不可見，排除）。
- **幀結構**：動作/幀/canvas 三層；canvas 子節點有 `z`(Str type 11)、`origin`(Vector)、`map`(SubTree，錨點 Vector)、`group`(Str，如 "skin")；幀層有 `delay`(Int)、`face`(U16)。
- **UOL 大量存在**（type 12，相對路徑如 `../../front/head`）：
  - head：`front/head` 直接是 Canvas；`stand1/0/head` 是 UOL → `../../front/head`。
  - face：`default/face` 直接是 Canvas；`blink/0/face` + delay。
  - hair/cap：`stand1/0/*` 全是 UOL → `../../default/*`；`default` 節點直接含 canvas（cap 的 UOL 名稱有原始資料拼字錯誤 `defalutAc`，但 UOL 值指向正確）。
  - hairShade 是 SubTree，內有編號 canvas（`hairShade/0`），其餘編號是 UOL → `0`。
- **身體動作 254 個**，前段即標準清單（walk1/walk2/stand1/stand2/alert/swing*/stab*/shoot*/proneStab/prone/heal/fly/jump/sit/ladder/rope/dead/...）。
- **z 圖層名實測集合**（body+各部位範例）：body、arm、head、face、mailChest、mailChestOverHighest、mailArm、pants、shoes、glove、gloveOverBody、shieldBelowBody、capeBelowBody、weapon、cap、capAccessoryBelowBody、hairShade、armBelowHead、armOverHair、handBelowWeapon、backBody 等。
- **zmap 順序**：從 `/e/_原始資料/Ming/Ming_Server/wz/Base.wz/zmap.img.xml`（解包式 XML）抽出 149 層順序，存於 `out/zmap/zmap.txt`，index 0=最上層（mobEquipFront），越後越底層；將內嵌進 `avatar.rs`。
- **名稱表**：`String/TW/Item` JSON key 是**去掉前導零的 id**（如 `"12000"`=奶油皮膚、`"1040036"`）；皮膚/臉型/髮型/裝備名都有。
- Accessory/01010000 無 `stand1`（配件動作集不同）→ 實作時 fallback 鏈：action → stand1 → default → 第一個非 info 子節點。
- cash 武器（Weapon 內 0170 開頭）可能有數位子節點（武器類型）→ fallback 時若直接找不到 action，往數位名子節點下找。

