//! 任務資料庫（`Quest/*` + `String/TW/QuestData`）聚合：清單來自 QuestData
//! 有名字的任務；明細組合 Say（對話腳本）、Check（需求）、Act（獎勵）、
//! QuestInfo（地區）四個 img，並補上 NPC / 道具 / 怪物名稱，回傳前端
//! 任務瀏覽所需的資料。

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::catalog::{CatalogError, DataService, Result};
use crate::commodity::load_names;
use crate::wzjs::{Value, WzNode};

/// 任務字串表（String 下的）葉節點名；完整路徑以目前語系組出
/// （`String/<locale>/QuestData`，TextAsset，UTF-8 明文 JSON：
/// id → {name, Info: {階段: 說明, parent}, Say: {鍵: 台詞}}）。
const QUEST_DATA_LEAF: &str = "QuestData";
/// 對話腳本 img。
const QUEST_SAY: &str = "Quest/Say";
/// 需求 img（"0"=開始需求、"1"=完成需求）。
const QUEST_CHECK: &str = "Quest/Check";
/// 獎勵 img（"0"=開始、"1"=完成）。
const QUEST_ACT: &str = "Quest/Act";
/// 地區等補充資訊 img（很多任務沒有此節點）。
const QUEST_INFO: &str = "Quest/QuestInfo";

/// 任務清單快取（只需組裝一次；跟隨 DataService 生命週期，更換資料集自動重建）。
#[derive(Default)]
pub(crate) struct QuestBookCache(pub Mutex<Option<Arc<Vec<QuestSummary>>>>);

impl QuestBookCache {
    /// 清掉快取（切換語系時呼叫）。
    pub(crate) fn reset(&self) {
        *self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

/// 任務字串表懶載入快取（book 與 detail 共用）。
#[derive(Default)]
pub(crate) struct QuestDataCache(pub Mutex<Option<Arc<QuestDataTables>>>);

impl QuestDataCache {
    /// 清掉快取（切換語系時呼叫）。
    pub(crate) fn reset(&self) {
        *self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

/// 任務相關名稱表（NPC / 道具 / 怪物）懶載入快取。
#[derive(Default)]
pub(crate) struct QuestNameCache(pub Mutex<Option<Arc<QuestNameTables>>>);

impl QuestNameCache {
    /// 清掉快取（切換語系時呼叫）。
    pub(crate) fn reset(&self) {
        *self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

/// `String/TW/QuestData` 解析後的字串表：任務 id（無補零）→ 文字。
pub(crate) type QuestDataTables = HashMap<String, QuestStrings>;

/// 單一任務的文字（名稱 / 系列名 / 各階段說明 / 台詞鍵 → 台詞）。
#[derive(Default)]
pub(crate) struct QuestStrings {
    name: Option<String>,
    parent: Option<String>,
    info: BTreeMap<String, String>,
    say: HashMap<String, String>,
}

/// 任務用到的名稱表。
#[derive(Default)]
pub(crate) struct QuestNameTables {
    npcs: HashMap<u32, String>,
    /// 道具名稱（Item + Pet）。
    items: HashMap<u32, String>,
    mobs: HashMap<u32, String>,
    /// 地圖名稱（String/TW/Map 的 mapName）。
    maps: HashMap<u32, String>,
}

/// 任務清單中的一個任務。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestSummary {
    pub id: String,
    pub name: Option<String>,
    pub parent: Option<String>,
    pub area: Option<i64>,
    pub level_min: Option<i64>,
    pub level_max: Option<i64>,
    pub start_npc_id: Option<i64>,
    pub start_npc_name: Option<String>,
    /// 完成後的後續任務（`Act.1.nextQuest`，轉字串）。
    pub next_quest_id: Option<String>,
}

/// 道具引用（需求或獎勵；count 負值＝完成時消耗）。
/// prop 是抽取標記：正值＝隨機獲得（值為權重，無標示時視為等權），
/// -1＝玩家自選一項，無此欄位＝固定給予。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestItemRef {
    pub id: i64,
    pub count: i64,
    pub name: Option<String>,
    pub prop: Option<i64>,
}

/// 怪物引用（完成需求的討伐數量）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestMobRef {
    pub id: i64,
    pub count: i64,
    pub name: Option<String>,
}

/// 任務引用（前置任務或後續任務）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestRef {
    pub id: String,
    pub name: Option<String>,
}

/// 單一階段（開始 / 完成）的對話腳本。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestDialogueStage {
    pub npc_id: Option<i64>,
    pub npc_name: Option<String>,
    pub lines: Vec<String>,
    pub ask: Vec<String>,
    pub yes: Vec<String>,
    pub no: Vec<String>,
    pub stop_npc: Vec<String>,
    pub stop_item: Vec<String>,
    pub lost: Vec<String>,
}

/// 開始需求（`Check.0`）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestStartRequirements {
    pub items: Vec<QuestItemRef>,
    pub pre_quests: Vec<QuestRef>,
}

/// 完成需求（`Check.1`）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestRequirements {
    pub items: Vec<QuestItemRef>,
    pub mobs: Vec<QuestMobRef>,
    pub pre_quests: Vec<QuestRef>,
}

/// 完成獎勵（`Act.1`）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestRewards {
    pub exp: Option<i64>,
    pub mesos: Option<i64>,
    pub popularity: Option<i64>,
    pub items: Vec<QuestItemRef>,
    pub next_quest: Option<QuestRef>,
}

/// 單一任務的完整明細。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestDetail {
    pub id: String,
    pub name: Option<String>,
    pub parent: Option<String>,
    pub area: Option<i64>,
    pub level_min: Option<i64>,
    pub level_max: Option<i64>,
    pub jobs: Vec<i64>,
    /// QuestData Info 的階段說明（不含 parent）。
    pub info: BTreeMap<String, String>,
    pub start: Option<QuestDialogueStage>,
    pub end: Option<QuestDialogueStage>,
    pub start_requirements: QuestStartRequirements,
    pub requirements: QuestRequirements,
    pub rewards: QuestRewards,
    /// 可重複接取的冷卻（Check 的 interval）。
    pub interval: Option<i64>,
}

impl DataService {
    /// 任務清單：QuestData 中有名字的任務，補上地區 / 等級範圍 /
    /// 起始 NPC / 後續任務；結果快取。資料集缺字串表時回傳空清單。
    pub fn get_quest_book(&self) -> Result<Arc<Vec<QuestSummary>>> {
        let mut guard = self
            .quest_book_cache
            .0
            .lock()
            .map_err(|error| CatalogError::CacheInvalid(error.to_string()))?;
        if let Some(book) = guard.as_ref() {
            return Ok(book.clone());
        }
        let book = Arc::new(self.load_quest_book()?);
        *guard = Some(book.clone());
        Ok(book)
    }

    fn load_quest_book(&self) -> Result<Vec<QuestSummary>> {
        let data = self.quest_data()?;
        if data.is_empty() {
            return Ok(Vec::new());
        }
        let check = self.quest_tree(QUEST_CHECK)?;
        let act = self.quest_tree(QUEST_ACT)?;
        let quest_info = self.quest_tree(QUEST_INFO)?;
        let names = self.quest_names()?;

        let mut list: Vec<QuestSummary> = data
            .iter()
            .filter(|(_, strings)| strings.name.is_some())
            .map(|(id, strings)| {
                let check0 = check
                    .as_ref()
                    .and_then(|tree| tree.find(id))
                    .and_then(|quest| quest.find("0"));
                let start_npc_id = check0.and_then(|node| prop_i64(node, "npc"));
                let next_quest_id = act
                    .as_ref()
                    .and_then(|tree| tree.find(id))
                    .and_then(|quest| quest.find("1"))
                    .and_then(|stage| prop_i64(stage, "nextQuest"));
                QuestSummary {
                    id: id.clone(),
                    name: strings.name.clone(),
                    parent: strings.parent.clone(),
                    area: quest_info
                        .as_ref()
                        .and_then(|tree| tree.find(id))
                        .and_then(|node| prop_i64(node, "area")),
                    level_min: check0.and_then(|node| prop_i64(node, "lvmin")),
                    level_max: check0.and_then(|node| prop_i64(node, "lvmax")),
                    start_npc_id,
                    start_npc_name: start_npc_id
                        .and_then(|npc| names.npcs.get(&(npc as u32)).cloned()),
                    next_quest_id: next_quest_id.map(|id| id.to_string()),
                }
            })
            .collect();
        list.sort_by(|a, b| id_sort_key(&a.id).cmp(&id_sort_key(&b.id)));
        Ok(list)
    }

    /// 單一任務完整明細：文字、對話腳本、需求與獎勵。
    /// 任務不在 QuestData 時回 Err；缺個別 img 時該部分留空，不報錯。
    pub fn get_quest_detail(&self, quest_id: &str) -> Result<QuestDetail> {
        let id = normalize_id(quest_id);
        let data = self.quest_data()?;
        let Some(strings) = data.get(id) else {
            return Err(CatalogError::PathNotFound(format!(
                "{}/{id}",
                self.string_path(QUEST_DATA_LEAF)
            )));
        };
        let say = self.quest_tree(QUEST_SAY)?;
        let check = self.quest_tree(QUEST_CHECK)?;
        let act = self.quest_tree(QUEST_ACT)?;
        let quest_info = self.quest_tree(QUEST_INFO)?;
        let names = self.quest_names()?;

        let check_node = check.as_ref().and_then(|tree| tree.find(id));
        let check0 = check_node.and_then(|quest| quest.find("0"));
        let check1 = check_node.and_then(|quest| quest.find("1"));
        let say_node = say.as_ref().and_then(|tree| tree.find(id));

        let npc_ref = |stage: Option<&WzNode>| {
            let npc_id = stage.and_then(|node| prop_i64(node, "npc"));
            (
                npc_id,
                npc_id.and_then(|npc| names.npcs.get(&(npc as u32)).cloned()),
            )
        };
        let (start_npc_id, start_npc_name) = npc_ref(check0);
        let (end_npc_id, end_npc_name) = npc_ref(check1);

        let start = say_node
            .and_then(|quest| quest.find("0"))
            .map(|stage| dialogue_stage(stage, start_npc_id, start_npc_name, strings, &names));
        let end = say_node
            .and_then(|quest| quest.find("1"))
            .map(|stage| dialogue_stage(stage, end_npc_id, end_npc_name, strings, &names));

        let next_quest_id = act
            .as_ref()
            .and_then(|tree| tree.find(id))
            .and_then(|quest| quest.find("1"))
            .and_then(|stage| prop_i64(stage, "nextQuest"));

        Ok(QuestDetail {
            id: id.to_string(),
            name: strings.name.clone(),
            parent: strings.parent.clone(),
            area: quest_info
                .as_ref()
                .and_then(|tree| tree.find(id))
                .and_then(|node| prop_i64(node, "area")),
            level_min: check0.and_then(|node| prop_i64(node, "lvmin")),
            level_max: check0.and_then(|node| prop_i64(node, "lvmax")),
            jobs: job_list(check0),
            info: strings
                .info
                .iter()
                .map(|(key, text)| (key.clone(), substitute_refs(text, &names)))
                .collect(),
            start,
            end,
            start_requirements: QuestStartRequirements {
                items: item_refs(check0, &names.items),
                pre_quests: quest_refs(check0, &data),
            },
            requirements: QuestRequirements {
                items: item_refs(check1, &names.items),
                mobs: mob_refs(check1, &names.mobs),
                pre_quests: quest_refs(check1, &data),
            },
            rewards: QuestRewards {
                exp: act_stage(&act, id).and_then(|stage| prop_i64(stage, "exp")),
                mesos: act_stage(&act, id).and_then(|stage| prop_i64(stage, "money")),
                popularity: act_stage(&act, id).and_then(|stage| prop_i64(stage, "pop")),
                items: item_refs(act_stage(&act, id), &names.items),
                next_quest: next_quest_id.map(|next| QuestRef {
                    id: next.to_string(),
                    name: data.get(&next.to_string()).and_then(|s| s.name.clone()),
                }),
            },
            interval: check0
                .and_then(|node| prop_i64(node, "interval"))
                .or_else(|| check1.and_then(|node| prop_i64(node, "interval"))),
        })
    }

    /// Quest img 懶載入：資料集缺此 img 時回 None（呼叫端留空處理）。
    fn quest_tree(&self, img_key: &str) -> Result<Option<Arc<WzNode>>> {
        if !self.catalog.entries.contains_key(img_key) {
            return Ok(None);
        }
        Ok(Some(self.tree_for(img_key)?))
    }

    /// 懶載入任務字串表；資料集缺字串表時視為空表。
    fn quest_data(&self) -> Result<Arc<QuestDataTables>> {
        let mut guard = self
            .quest_data_cache
            .0
            .lock()
            .map_err(|error| CatalogError::CacheInvalid(error.to_string()))?;
        if let Some(tables) = guard.as_ref() {
            return Ok(tables.clone());
        }
        let tables = Arc::new(load_quest_data(self));
        *guard = Some(tables.clone());
        Ok(tables)
    }

    /// 懶載入 NPC / 道具 / 怪物名稱表；資料集缺字串表時視為空表。
    fn quest_names(&self) -> Result<Arc<QuestNameTables>> {
        let mut guard = self
            .quest_name_cache
            .0
            .lock()
            .map_err(|error| CatalogError::CacheInvalid(error.to_string()))?;
        if let Some(tables) = guard.as_ref() {
            return Ok(tables.clone());
        }
        let mut items = load_names(self, &self.string_path("Item"));
        for (id, name) in load_names(self, &self.string_path("Pet")) {
            items.entry(id).or_insert(name);
        }
        let tables = Arc::new(QuestNameTables {
            npcs: load_names(self, &self.string_path("Npc")),
            items,
            mobs: load_names(self, &self.string_path("Mob")),
            maps: load_map_names(self),
        });
        *guard = Some(tables.clone());
        Ok(tables)
    }
}

/// `Act` 樹中某任務的完成（"1"）節點。
fn act_stage<'a>(act: &'a Option<Arc<WzNode>>, id: &str) -> Option<&'a WzNode> {
    act.as_ref()
        .and_then(|tree| tree.find(id))
        .and_then(|quest| quest.find("1"))
}

/// 組一個階段的對話腳本：編號子節點是台詞鍵，ask/yes/no/stop/lost
/// 是子樹；鍵去 QuestData.Say 查文字，查不到就略過該行。
/// 少數任務的階段是 leaf 字串（視為單行台詞鍵）或空節點（全空）。
fn dialogue_stage(
    stage: &WzNode,
    npc_id: Option<i64>,
    npc_name: Option<String>,
    strings: &QuestStrings,
    names: &QuestNameTables,
) -> QuestDialogueStage {
    let resolve = |node: Option<&WzNode>| -> Vec<String> {
        numbered_strs(node)
            .filter_map(|key| strings.say.get(key))
            .map(|text| substitute_refs(text, names))
            .collect()
    };
    let mut lines = resolve(Some(stage));
    // leaf 字串階段：值本身就是一個台詞鍵。
    if lines.is_empty() {
        if let Some(key) = as_str(&stage.value) {
            if let Some(text) = strings.say.get(key) {
                lines.push(substitute_refs(text, names));
            }
        }
    }
    let stop = stage.find("stop");
    QuestDialogueStage {
        npc_id,
        npc_name,
        lines,
        ask: resolve(stage.find("ask")),
        yes: resolve(stage.find("yes")),
        no: resolve(stage.find("no")),
        stop_npc: resolve(stop.and_then(|node| node.find("npc"))),
        stop_item: resolve(stop.and_then(|node| node.find("item"))),
        lost: resolve(stage.find("lost")),
    }
}

/// 節點的數字名子節點（"0","1",...）依數值排序後的字串值。
/// 傳入 None 或子節點值非字串時略過。
fn numbered_strs(node: Option<&WzNode>) -> impl Iterator<Item = &str> {
    let mut children: Vec<&WzNode> = node
        .map(|node| {
            node.children
                .iter()
                .filter(|child| child.name.bytes().all(|b| b.is_ascii_digit()))
                .collect()
        })
        .unwrap_or_default();
    children.sort_by_key(|child| child.name.parse::<u64>().unwrap_or(u64::MAX));
    children
        .into_iter()
        .filter_map(|child| as_str(&child.value))
}

/// Check/Act 階段的 item 清單：{idx: {id, count, prop?}}，count 缺省為 1。
fn item_refs(stage: Option<&WzNode>, names: &HashMap<u32, String>) -> Vec<QuestItemRef> {
    let Some(items) = stage.and_then(|node| node.find("item")) else {
        return Vec::new();
    };
    items
        .children
        .iter()
        .filter_map(|entry| {
            let id = prop_i64(entry, "id")?;
            Some(QuestItemRef {
                id,
                count: prop_i64(entry, "count").unwrap_or(1),
                name: names.get(&(id as u32)).cloned(),
                prop: prop_i64(entry, "prop"),
            })
        })
        .collect()
}

/// Check 階段的 mob 清單：{idx: {id, count}}。
fn mob_refs(stage: Option<&WzNode>, names: &HashMap<u32, String>) -> Vec<QuestMobRef> {
    let Some(mobs) = stage.and_then(|node| node.find("mob")) else {
        return Vec::new();
    };
    mobs.children
        .iter()
        .filter_map(|entry| {
            let id = prop_i64(entry, "id")?;
            Some(QuestMobRef {
                id,
                count: prop_i64(entry, "count").unwrap_or(1),
                name: names.get(&(id as u32)).cloned(),
            })
        })
        .collect()
}

/// Check 階段的前置任務清單：{idx: {id, state}}；名稱查 QuestData。
fn quest_refs(stage: Option<&WzNode>, data: &QuestDataTables) -> Vec<QuestRef> {
    let Some(quests) = stage.and_then(|node| node.find("quest")) else {
        return Vec::new();
    };
    quests
        .children
        .iter()
        .filter_map(|entry| {
            let id = prop_i64(entry, "id")?;
            Some(QuestRef {
                id: id.to_string(),
                name: data.get(&id.to_string()).and_then(|s| s.name.clone()),
            })
        })
        .collect()
}

/// Check.0 的 job 清單：{idx: int}；少數資料是單一 int leaf。
fn job_list(stage: Option<&WzNode>) -> Vec<i64> {
    let Some(job) = stage.and_then(|node| node.find("job")) else {
        return Vec::new();
    };
    if job.children.is_empty() {
        return value_as_i64(&job.value).into_iter().collect();
    }
    job.children
        .iter()
        .filter_map(|child| value_as_i64(&child.value))
        .collect()
}

fn prop<'a>(node: &'a WzNode, name: &str) -> Option<&'a WzNode> {
    node.children
        .iter()
        .find(|child| child.name.eq_ignore_ascii_case(name))
}

fn prop_i64(node: &WzNode, name: &str) -> Option<i64> {
    prop(node, name).and_then(|child| value_as_i64(&child.value))
}

fn value_as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::U8(v) => Some(i64::from(*v)),
        Value::U16(v) => Some(i64::from(*v)),
        Value::Int(v) => Some(i64::from(*v)),
        Value::Long(v) => Some(*v),
        Value::Int2(a, _) => Some(i64::from(*a)),
        Value::Double(v) => Some(*v as i64),
        _ => None,
    }
}

fn as_str(value: &Value) -> Option<&str> {
    match value {
        Value::Str(s) | Value::Uol(s) => Some(s),
        _ => None,
    }
}

/// 任務 id 正規化：去掉補零（全零視為 "0"），讓 `0001000` 與 `1000` 視為同一 id。
fn normalize_id(id: &str) -> &str {
    let trimmed = id.trim_start_matches('0');
    if trimmed.is_empty() {
        "0"
    } else {
        trimmed
    }
}

/// 任務 id 排序鍵：可解析成數字的按數值排，其餘按字典序排在後。
fn id_sort_key(id: &str) -> (u8, i64, &str) {
    match id.parse::<i64>() {
        Ok(n) => (0, n, ""),
        Err(_) => (1, 0, id),
    }
}

/// 部分 text_asset 帶 UTF-8 BOM，serde_json 不接受，先去掉。
fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// 台詞中的引用標記換成名字：`#p<id>#` NPC、`#o<id>#` 怪物、`#m<id>#` 地圖
/// （查不到保留原標記，前端會去掉）。名字落在 `#b…#k` 強調區間內時仍保留強調效果。
fn substitute_refs(text: &str, names: &QuestNameTables) -> String {
    fn lookup(names: &QuestNameTables, kind: char, id: u32) -> Option<&str> {
        let table = match kind {
            'p' => &names.npcs,
            'o' => &names.mobs,
            'm' => &names.maps,
            _ => return None,
        };
        table.get(&id).map(String::as_str)
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('#') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if after.is_empty() {
            out.push('#');
            rest = after;
            break;
        }
        let kind = after.chars().next().unwrap_or('\0');
        // kind 必須先確認是 ASCII 的 p/o/m，才能用位元組索引切 after[1..]
        // （否則像「#長」這種標記後直接接中文的情況會切到字元中段而 panic）。
        if !matches!(kind, 'p' | 'o' | 'm') {
            out.push('#');
            rest = after;
            continue;
        }
        let digits = after[1..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .count();
        let name = if digits > 0 && after[1 + digits..].starts_with('#') {
            after[1..1 + digits]
                .parse::<u32>()
                .ok()
                .and_then(|id| lookup(names, kind, id))
        } else {
            None
        };
        match name {
            Some(name) => {
                out.push_str(name);
                rest = &after[1 + digits + 1..];
            }
            None => {
                out.push('#');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// 解析 `String/TW/Map`（`{id: {mapName, ...}}`），地圖 id → 地圖名。
fn load_map_names(service: &DataService) -> HashMap<u32, String> {
    let mut table = HashMap::new();
    let Ok(node) = service.get_node(&service.string_path("Map")) else {
        return table;
    };
    let Some(text) = node.text_asset else {
        return table;
    };
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str(strip_bom(&text)) else {
        return table;
    };
    for (key, value) in map {
        let Ok(id) = key.parse::<u32>() else {
            continue;
        };
        let Some(name) = value.get("mapName").and_then(|v| v.as_str()) else {
            continue;
        };
        table.insert(id, name.to_string());
    }
    table
}

/// 解析 `String/TW/QuestData`（`{id: {name, Info: {階段, parent}, Say: {鍵: 台詞}}}`）。
fn load_quest_data(service: &DataService) -> QuestDataTables {
    let mut table = HashMap::new();
    let Ok(node) = service.get_node(&service.string_path(QUEST_DATA_LEAF)) else {
        return table;
    };
    let Some(text) = node.text_asset else {
        return table;
    };
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str(strip_bom(&text)) else {
        return table;
    };
    for (id, value) in map {
        let info_value = value.get("Info").and_then(|v| v.as_object());
        let mut info = BTreeMap::new();
        let mut parent = None;
        if let Some(obj) = info_value {
            for (key, val) in obj {
                let Some(text) = val.as_str() else {
                    continue;
                };
                if key == "parent" {
                    parent = Some(text.to_string());
                } else if key.bytes().all(|b| b.is_ascii_digit()) {
                    // 只收數字階段鍵；demandSummary / rewardSummary 是遊戲內
                    // 追蹤摘要（需求/獎勵在結構化區塊已有），不重複顯示。
                    info.insert(key.clone(), text.to_string());
                }
            }
        }
        let say = value
            .get("Say")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        table.insert(
            id,
            QuestStrings {
                name: value
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                parent,
                info,
                say,
            },
        );
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(name: &str, value: Value, children: Vec<WzNode>) -> WzNode {
        WzNode {
            name: name.to_string(),
            value,
            path: None,
            children,
            type_tag: 0,
        }
    }

    fn subtree(name: &str, children: Vec<WzNode>) -> WzNode {
        make_node(name, Value::SubTree, children)
    }

    fn str_leaf(name: &str, value: &str) -> WzNode {
        make_node(name, Value::Str(value.to_string()), vec![])
    }

    fn int_leaf(name: &str, value: i32) -> WzNode {
        make_node(name, Value::Int(value), vec![])
    }

    fn strings_with_say(pairs: &[(&str, &str)]) -> QuestStrings {
        QuestStrings {
            say: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn normalize_id_strips_zero_padding() {
        assert_eq!(normalize_id("0001000"), "1000");
        assert_eq!(normalize_id("1000"), "1000");
        assert_eq!(normalize_id("000"), "0");
    }

    #[test]
    fn id_sort_numeric_before_lexicographic() {
        let mut ids = vec!["abc", "2002", "1000", "6100"];
        ids.sort_by(|a, b| id_sort_key(a).cmp(&id_sort_key(b)));
        assert_eq!(ids, vec!["1000", "2002", "6100", "abc"]);
    }

    #[test]
    fn dialogue_stage_resolves_say_keys_and_skips_unknown() {
        let stage = subtree(
            "0",
            vec![
                str_leaf("0", "0_0"),
                str_leaf("2", "0_missing"),
                str_leaf("1", "0_1"),
                subtree("ask", vec![str_leaf("0", "0_ask_0")]),
                subtree("yes", vec![str_leaf("0", "0_yes_0")]),
                subtree("no", vec![str_leaf("0", "0_no_0")]),
                subtree(
                    "stop",
                    vec![
                        subtree("npc", vec![str_leaf("0", "0_stop_npc_0")]),
                        subtree("item", vec![str_leaf("0", "0_stop_item_0")]),
                    ],
                ),
                subtree("lost", vec![str_leaf("0", "0_lost_0")]),
            ],
        );
        let strings = strings_with_say(&[
            ("0_0", "第一句"),
            ("0_1", "第二句"),
            ("0_ask_0", "問句"),
            ("0_yes_0", "接受"),
            ("0_no_0", "拒絕"),
            ("0_stop_npc_0", "去找別人"),
            ("0_stop_item_0", "缺道具"),
            ("0_lost_0", "道具遺失"),
        ]);
        let result = dialogue_stage(
            &stage,
            Some(2101),
            Some("NPC".to_string()),
            &strings,
            &QuestNameTables::default(),
        );
        assert_eq!(result.npc_id, Some(2101));
        // 數字鍵依數值排序；查不到文字的鍵略過
        assert_eq!(result.lines, vec!["第一句", "第二句"]);
        assert_eq!(result.ask, vec!["問句"]);
        assert_eq!(result.yes, vec!["接受"]);
        assert_eq!(result.no, vec!["拒絕"]);
        assert_eq!(result.stop_npc, vec!["去找別人"]);
        assert_eq!(result.stop_item, vec!["缺道具"]);
        assert_eq!(result.lost, vec!["道具遺失"]);
    }

    #[test]
    fn dialogue_stage_tolerates_leaf_string_and_empty_stage() {
        let strings = strings_with_say(&[("0_0", "只有一句")]);
        // leaf 字串階段：值當作單行台詞鍵
        let leaf = str_leaf("0", "0_0");
        let result = dialogue_stage(&leaf, None, None, &strings, &QuestNameTables::default());
        assert_eq!(result.lines, vec!["只有一句"]);
        // 空節點（null 階段）：全部留空
        let empty = subtree("0", vec![]);
        let result = dialogue_stage(&empty, None, None, &strings, &QuestNameTables::default());
        assert!(result.lines.is_empty());
        assert!(result.ask.is_empty());
    }

    #[test]
    fn substitute_refs_replaces_known_ids() {
        let names = QuestNameTables {
            npcs: [(2101, "希娜".to_string())].into_iter().collect(),
            mobs: [(100100, "蝸牛".to_string())].into_iter().collect(),
            maps: [(1010000, "楓葉村".to_string())].into_iter().collect(),
            ..Default::default()
        };
        // 查得到的 id 換成名字（在 #b…#k 區間內照常替換，保留強調效果）
        assert_eq!(
            substitute_refs("去找#b#p2101##k。", &names),
            "去找#b希娜#k。"
        );
        // #o 怪物、#m 地圖一併代換
        assert_eq!(
            substitute_refs("打倒#o100100#後去#m1010000#", &names),
            "打倒蝸牛後去楓葉村"
        );
        // 查不到的 id 保留原標記（前端會去掉）
        assert_eq!(substitute_refs("#p2101#與#p9999#", &names), "希娜與#p9999#");
        // 無引用時原樣返回
        assert_eq!(substitute_refs("普通台詞", &names), "普通台詞");
        // 非 p/o/m 標記後直接接中文（如 #i4031003#長…）：原樣保留且不 panic
        assert_eq!(
            substitute_refs("#i4031003#長這樣的鏡子。", &names),
            "#i4031003#長這樣的鏡子。"
        );
    }

    #[test]
    fn item_refs_parse_id_count_with_default() {
        let stage = subtree(
            "0",
            vec![subtree(
                "item",
                vec![
                    subtree("0", vec![int_leaf("id", 2002), int_leaf("count", 10)]),
                    subtree("1", vec![int_leaf("id", 3000)]),
                    // 缺 id 的條目略過
                    subtree("2", vec![int_leaf("count", 5)]),
                    // 抽取標記 prop（隨機權重 / -1 自選）
                    subtree("3", vec![int_leaf("id", 4000), int_leaf("prop", -1)]),
                ],
            )],
        );
        let mut names = HashMap::new();
        names.insert(2002u32, "紅色藥水".to_string());
        let refs = item_refs(Some(&stage), &names);
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0].id, 2002);
        assert_eq!(refs[0].count, 10);
        assert_eq!(refs[0].name.as_deref(), Some("紅色藥水"));
        assert_eq!(refs[0].prop, None);
        assert_eq!(refs[1].id, 3000);
        assert_eq!(refs[1].count, 1);
        assert_eq!(refs[1].name, None);
        assert_eq!(refs[2].id, 4000);
        assert_eq!(refs[2].prop, Some(-1));
        assert!(item_refs(None, &names).is_empty());
    }

    #[test]
    fn quest_refs_resolve_names_from_quest_data() {
        let stage = subtree(
            "0",
            vec![subtree(
                "quest",
                vec![subtree(
                    "0",
                    vec![int_leaf("id", 1000), int_leaf("state", 1)],
                )],
            )],
        );
        let mut data = QuestDataTables::new();
        data.insert(
            "1000".to_string(),
            QuestStrings {
                name: Some("前置任務".to_string()),
                ..Default::default()
            },
        );
        let refs = quest_refs(Some(&stage), &data);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].id, "1000");
        assert_eq!(refs[0].name.as_deref(), Some("前置任務"));
    }

    #[test]
    fn job_list_handles_subtree_and_leaf() {
        let stage = subtree(
            "0",
            vec![subtree("job", vec![int_leaf("0", 312), int_leaf("1", 322)])],
        );
        assert_eq!(job_list(Some(&stage)), vec![312, 322]);
        let leaf_stage = subtree("0", vec![int_leaf("job", 0)]);
        assert_eq!(job_list(Some(&leaf_stage)), vec![0]);
        assert!(job_list(None).is_empty());
        assert!(job_list(Some(&subtree("0", vec![]))).is_empty());
    }
}
