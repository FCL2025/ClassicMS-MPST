# 介面功能候選清單

盤點官方遊戲資料（catalog cache v5，14,793 筆 entry）中「資料存在、但介面尚未專門呈現」
的類別，供後續挑選實作。完成後把該項的 `[ ]` 改成 `[x]` 並註記版本。

資料依據：`out/export_catalog_cache.json`（2026-08-02 建置）、`docs/PLAN.md`、
`docs/PAPERDOLL.md`、`src-tauri/src/*.rs` 實際讀取路徑。

## 高價值

- [x] **技能瀏覽器**（2026-08-03 完成，未發布）
  - 資料：`Skill/`（98 img，各職業技能書 + MobSkill）+ `String/TW/Skill`
  - 實作：三欄配置（職業清單／遊戲風格技能視窗／技能詳情）、等級滑桿連動
    LevelDesc/H 模板代入、數值中文化、全域搜尋、動作延遲
  - 後端 `skill.rs`：`get_skill_book`（82 職業群組 / 640 技能）+ `get_skill_detail`

- [ ] **地圖檢視器 / 地圖資料庫**
  - 資料：`Map/`（934 img：Map0~Map9 + Back/Obj/Tile）+ `String/TW/Map`
  - 內容（a）：地圖名、出沒怪物、傳送點連結、BGM、回村點（純資料，難度中）
  - 內容（b）：用 Tile/Obj/Back 拼出整張地圖場景（難度高）
  - 延伸：世界地圖（`Etc/WorldMap` 有地圖連線資料）

- [ ] **NPC 圖鑑 + 對話查看器**
  - 資料：`Npc/`（950 img）+ `String/TW/Npc` + `String/TW/ScriptString/*`（對話腳本）
    + `Etc/NpcLocation`（出沒地點）
  - 內容：NPC 列表、外觀動畫、台詞、所在位置
  - 難度：中（對話腳本格式需先摸清）

- [ ] **任務資料庫**
  - 資料：`Quest/`（Act/Check/Say/QuestInfo 等 7 img）+ `String/TW/QuestData`、`QuestCategory`
  - 內容：任務列表、前置條件、需求道具/怪物、獎勵
  - 難度：中

- [ ] **全物品資料庫**（消耗/裝飾/其他/寵物/特殊）
  - 資料：`Item/`（Cash 40、Consume 31、Etc 16、Pet 42 等 139 img）+ `String/TW/Item`
  - 現況：tooltip 僅做 `Character/*` 裝備
  - 內容：物品總表（分類 tab + 搜尋），非裝備也給 tooltip（效果、說明、售價）
  - 難度：低

## 中價值

- [ ] **套裝效果檢視**（`Etc/SetItemInfo`）：套裝部件與 2/3/4 件效果，難度低
- [ ] **坐騎 / 變身預覽**（`TamingMob` 6、`Morph` 39）：紙娃娃系統延伸，難度中
- [ ] **音效播放器頁**（`Sound/` 2,773 條，含 171 首 BGM 分類表）：BGM 列表試聽，難度低
- [ ] **道具製作 / 合成表**（`Etc/ItemMake`）：材料 → 成品查詢，難度低~中
- [ ] **Reactor 圖鑑**（`Reactor/` 338：箱子、觸發物）：動畫預覽 + 掉落，難度中
- [ ] **怪物出沒反查**：地圖 → 怪物查詢（可併入地圖檢視器）

## 低優先

- [ ] **特效展示**（`Effect/`：BasicEff、ItemEff、SetEff、SkillName）：技能瀏覽器可順便引用
- [ ] **書籍文字閱讀器**（`String/TW/Book`），難度低
- [ ] **副本組資訊**（`String/TW/FieldSet`）
- [ ] **UI 素材展示**（`UI/`：聊天氣泡、公會標記、名牌）
- [ ] **Feature 開關表**（`Feature/`）：純工程參考

## 已完成

- [x] 節點樹瀏覽 / 圖片預覽 / 音效播放 / TextAsset 檢視
- [x] 裝備 tooltip（`Character/*`）
- [x] 商城預覽（`Etc/TW/Commodity`）
- [x] 紙娃娃（含試穿商店、動畫、匯出 PNG/GIF/WebP）
- [x] 怪物圖鑑（含戰利品、出沒地圖）
- [x] 名稱/路徑/id 搜尋
- [x] 匯出 JSON / PNG / XML
