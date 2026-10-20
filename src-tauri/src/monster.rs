//! 怪物圖鑑（`MonsterBook`）聚合：解析圖鑑的 reward/map 清單，
//! 補上 `String/TW/Mob`、`String/TW/MonsterBook`、`String/TW/Map` 的文字
//! 與道具 icon、Mob 動畫幀，回傳前端圖鑑檢視所需的一切。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::Engine;
use serde::Serialize;

use crate::catalog::{DataService, Result};
use crate::commodity::load_names;
use crate::wzjs::{Value, WzNode};

/// 圖鑑清單快取（只需組裝一次；跟隨 DataService 生命週期，更換資料集自動重建）。
#[derive(Default)]
pub(crate) struct MonsterBookCache(Mutex<Option<Arc<Vec<MonsterSummary>>>>);

/// 圖鑑相關名稱表懶載入快取。
#[derive(Default)]
pub(crate) struct MonsterNameCache(Mutex<Option<Arc<MonsterNameTables>>>);

/// 圖鑑用到的各種文字表。
pub(crate) struct MonsterNameTables {
    mobs: HashMap<u32, String>,
    /// `String/TW/MonsterBook` 解析後的圖鑑文字（等級 / 型態 / 描述）。
    book_texts: HashMap<u32, BookText>,
    /// `String/TW/Map`：地圖 id → 顯示名稱（「地區 : 地圖名」）。
    maps: HashMap<u32, String>,
    /// 道具名稱（Item + Pet）。
    items: HashMap<u32, String>,
}

/// `String/TW/MonsterBook` 一筆圖鑑文字解析後的結果。
struct BookText {
    level: Option<i64>,
    category: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DropRef {
    pub item_id: u32,
    pub name: Option<String>,
}

/// 圖鑑清單中的一隻怪物。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonsterSummary {
    pub mob_id: u32,
    pub name: Option<String>,
    pub level: Option<i64>,
    pub drops: Vec<DropRef>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapRef {
    pub map_id: u32,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MobDropItem {
    pub item_id: u32,
    pub name: Option<String>,
    /// 圖示的 PNG base64：`info/icon`（備援 `info/iconRaw`）；取圖失敗時為 None。
    pub icon_base64: Option<String>,
}

/// Mob 動畫的一幀（base64 PNG + 播放資訊）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MobFrame {
    pub base64: String,
    pub delay: i64,
    pub origin: [i64; 2],
    pub width: u32,
    pub height: u32,
}

/// 單一怪物的完整明細。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MobDetail {
    pub mob_id: u32,
    pub name: Option<String>,
    pub level: Option<i64>,
    pub category: Option<String>,
    pub description: Option<String>,
    pub max_hp: i64,
    pub max_mp: i64,
    pub exp: i64,
    pub pad: i64,
    pub pdd: i64,
    pub mad: i64,
    pub mdd: i64,
    pub acc: i64,
    pub eva: i64,
    pub speed: i64,
    pub pushed: i64,
    pub body_attack: bool,
    pub undead: bool,
    pub maps: Vec<MapRef>,
    pub drops: Vec<MobDropItem>,
    pub frames: Vec<MobFrame>,
}

impl DataService {
    /// 圖鑑清單：解析 `MonsterBook` 的 reward，補名稱與等級；結果快取。
    /// 資料集沒有此資產時回傳空清單。
    pub fn get_monster_book(&self) -> Result<Arc<Vec<MonsterSummary>>> {
        let mut guard = self
            .monster_book_cache
            .0
            .lock()
            .map_err(|error| crate::catalog::CatalogError::CacheInvalid(error.to_string()))?;
        if let Some(book) = guard.as_ref() {
            return Ok(book.clone());
        }
        let book = Arc::new(self.load_monster_book()?);
        *guard = Some(book.clone());
        Ok(book)
    }

    fn load_monster_book(&self) -> Result<Vec<MonsterSummary>> {
        if !self.catalog.entries.contains_key("MonsterBook") {
            return Ok(Vec::new());
        }
        let tree = self.tree_for("MonsterBook")?;
        let names = self.monster_names()?;
        let mut list = Vec::new();
        for entry in &tree.children {
            let Ok(mob_id) = entry.name.parse::<u32>() else {
                continue;
            };
            let drops = entry
                .find("reward")
                .map(|reward| {
                    reward
                        .children
                        .iter()
                        .filter_map(|node| value_as_i64(&node.value))
                        .filter(|id| *id > 0)
                        .map(|id| {
                            let item_id = id as u32;
                            DropRef {
                                item_id,
                                name: names.items.get(&item_id).cloned(),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            list.push(MonsterSummary {
                mob_id,
                name: names.mobs.get(&mob_id).cloned(),
                level: names.book_texts.get(&mob_id).and_then(|t| t.level),
                drops,
            });
        }
        Ok(list)
    }

    /// 單一怪物明細：能力值（Mob img info）、動畫幀、掉落物（名稱+icon）、
    /// 出沒地圖與圖鑑文字。缺 Mob img 時能力值/動畫留空，不報錯。
    pub fn get_mob_detail(&self, mob_id: u32) -> Result<MobDetail> {
        let names = self.monster_names()?;
        let book_text = names.book_texts.get(&mob_id);

        let img_path = format!("Mob/{mob_id:07}");
        let mob_tree = if self.catalog.entries.contains_key(&img_path) {
            Some(self.tree_for(&img_path)?)
        } else {
            None
        };
        let info = mob_tree.as_ref().and_then(|tree| tree.find("info"));
        let stat = |name: &str| info.and_then(|node| prop_i64(node, name)).unwrap_or(0);

        let frames = match mob_tree.as_ref() {
            Some(tree) => self.mob_frames(tree, &img_path),
            None => Vec::new(),
        };

        // 掉落物與地圖從 MonsterBook 樹取（key 為無補零的 mob id）。
        let book_entry = if self.catalog.entries.contains_key("MonsterBook") {
            let tree = self.tree_for("MonsterBook")?;
            tree.children
                .iter()
                .find(|entry| entry.name.parse::<u32>() == Ok(mob_id))
                .map(|entry| {
                    (
                        reward_ids(entry),
                        entry
                            .find("map")
                            .map(|maps| {
                                maps.children
                                    .iter()
                                    .filter_map(|node| value_as_i64(&node.value))
                                    .filter(|id| *id > 0)
                                    .map(|id| id as u32)
                                    .collect::<Vec<u32>>()
                            })
                            .unwrap_or_default(),
                    )
                })
        } else {
            None
        };
        let (drop_ids, map_ids) = book_entry.unwrap_or_default();

        let encode = |png: Vec<u8>| base64::engine::general_purpose::STANDARD.encode(png);
        let drops = drop_ids
            .into_iter()
            .map(|item_id| MobDropItem {
                item_id,
                name: names.items.get(&item_id).cloned(),
                icon_base64: self
                    .icon_candidates(item_id)
                    .into_iter()
                    .find_map(|path| self.get_image(&path).ok().flatten().map(encode)),
            })
            // 幽靈道具過濾：名稱與 icon 都查不到 = 客戶端對此 id 完全無資料
            // （圖鑑 reward 清單來自伺服器端完整資料庫，會殘留未實裝道具），
            // 直接隱藏；只缺其中一項的視為資料小缺，保留顯示。
            .filter(|drop| drop.name.is_some() || drop.icon_base64.is_some())
            .collect();
        let maps = map_ids
            .into_iter()
            .map(|map_id| MapRef {
                map_id,
                name: names.maps.get(&map_id).cloned(),
            })
            .collect();

        Ok(MobDetail {
            mob_id,
            name: names.mobs.get(&mob_id).cloned(),
            level: prop_i64_opt(info, "level").or(book_text.and_then(|t| t.level)),
            category: book_text.and_then(|t| t.category.clone()),
            description: book_text.and_then(|t| t.description.clone()),
            max_hp: stat("maxHP"),
            max_mp: stat("maxMP"),
            exp: stat("exp"),
            pad: stat("PADamage"),
            pdd: stat("PDDamage"),
            mad: stat("MADamage"),
            mdd: stat("MDDamage"),
            acc: stat("acc"),
            eva: stat("eva"),
            speed: stat("speed"),
            pushed: stat("pushed"),
            body_attack: stat("bodyAttack") != 0,
            undead: stat("undead") != 0,
            maps,
            drops,
            frames,
        })
    }

    /// 取 Mob 動畫幀：優先 stand → move → 第一個含 canvas 子節點的動作。
    fn mob_frames(&self, tree: &WzNode, img_path: &str) -> Vec<MobFrame> {
        let has_canvas = |node: &WzNode| {
            node.children
                .iter()
                .any(|child| matches!(child.value, Value::Canvas))
        };
        let action = ["stand", "move"]
            .into_iter()
            .find_map(|name| tree.find(name).filter(|node| has_canvas(node)))
            .or_else(|| {
                tree.children
                    .iter()
                    .find(|node| node.name != "info" && has_canvas(node))
            });
        let Some(action) = action else {
            return Vec::new();
        };
        let encode = |png: &Vec<u8>| base64::engine::general_purpose::STANDARD.encode(png);
        action
            .children
            .iter()
            .filter(|frame| matches!(frame.value, Value::Canvas))
            .filter_map(|frame| {
                let path = format!("{img_path}/{}/{}", action.name, frame.name);
                let png = self.get_image(&path).ok().flatten()?;
                let (width, height) = png_dimensions(&png)?;
                Some(MobFrame {
                    base64: encode(&png),
                    delay: prop_i64(frame, "delay").unwrap_or(120),
                    origin: vector_prop(frame, "origin").unwrap_or([0, 0]),
                    width,
                    height,
                })
            })
            .collect()
    }

    /// 懶載入圖鑑相關文字表；資料集缺字串表時視為空表。
    fn monster_names(&self) -> Result<Arc<MonsterNameTables>> {
        let mut guard = self
            .monster_name_cache
            .0
            .lock()
            .map_err(|error| crate::catalog::CatalogError::CacheInvalid(error.to_string()))?;
        if let Some(tables) = guard.as_ref() {
            return Ok(tables.clone());
        }
        let mut items = load_names(self, "String/TW/Item");
        for (id, name) in load_names(self, "String/TW/Pet") {
            items.entry(id).or_insert(name);
        }
        let tables = Arc::new(MonsterNameTables {
            mobs: load_names(self, "String/TW/Mob"),
            book_texts: load_book_texts(self),
            maps: load_map_names(self),
            items,
        });
        *guard = Some(tables.clone());
        Ok(tables)
    }
}

/// reward 子節點的道具 id 清單。
fn reward_ids(entry: &WzNode) -> Vec<u32> {
    entry
        .find("reward")
        .map(|reward| {
            reward
                .children
                .iter()
                .filter_map(|node| value_as_i64(&node.value))
                .filter(|id| *id > 0)
                .map(|id| id as u32)
                .collect()
        })
        .unwrap_or_default()
}

fn prop<'a>(node: &'a WzNode, name: &str) -> Option<&'a WzNode> {
    node.children
        .iter()
        .find(|child| child.name.eq_ignore_ascii_case(name))
}

fn prop_i64(node: &WzNode, name: &str) -> Option<i64> {
    prop(node, name).and_then(|child| value_as_i64(&child.value))
}

fn prop_i64_opt(info: Option<&WzNode>, name: &str) -> Option<i64> {
    info.and_then(|node| prop_i64(node, name))
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

/// vector 類節點（origin 等）→ [x, y]。
fn vector_prop(node: &WzNode, name: &str) -> Option<[i64; 2]> {
    match &prop(node, name)?.value {
        Value::Vector(x, y) | Value::Float2(x, y) => Some([*x as i64, *y as i64]),
        Value::Int2(x, y) => Some([i64::from(*x), i64::from(*y)]),
        _ => None,
    }
}

/// 從 PNG 標頭（IHDR，offset 16 起兩個 big-endian u32）讀寬高。
fn png_dimensions(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let width = u32::from_be_bytes(png[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(png[20..24].try_into().ok()?);
    Some((width, height))
}

/// 部分 text_asset 帶 UTF-8 BOM，serde_json 不接受，先去掉。
fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// 解析 `String/TW/MonsterBook`：文字內的換行是字面 `\n` 兩字元，
/// 格式為 `等級：Lv.8\n型態：植物\n\n描述…`。
fn load_book_texts(service: &DataService) -> HashMap<u32, BookText> {
    let mut table = HashMap::new();
    let Ok(node) = service.get_node("String/TW/MonsterBook") else {
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
        let Some(raw) = value.as_str() else {
            continue;
        };
        table.insert(id, parse_book_text(raw));
    }
    table
}

fn parse_book_text(raw: &str) -> BookText {
    let normalized = raw.replace("\\n", "\n");
    let mut level = None;
    let mut category = None;
    for line in normalized.lines() {
        if let Some(rest) = line.strip_prefix("等級：Lv.") {
            level = rest.trim().parse::<i64>().ok();
        } else if let Some(rest) = line.strip_prefix("型態：") {
            let value = rest.trim();
            if !value.is_empty() {
                category = Some(value.to_string());
            }
        }
    }
    let description = normalized
        .split_once("\n\n")
        .map(|(_, body)| body.trim().to_string())
        .filter(|body| !body.is_empty());
    BookText {
        level,
        category,
        description,
    }
}

/// 解析 `String/TW/Map`（`{id: {streetName, mapName}}`）成 id →「地區 : 地圖名」。
pub(crate) fn load_map_names(service: &DataService) -> HashMap<u32, String> {
    let mut table = HashMap::new();
    let Ok(node) = service.get_node("String/TW/Map") else {
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
        let map_name = value.get("mapName").and_then(|v| v.as_str()).unwrap_or("");
        let street = value
            .get("streetName")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let label = match (street.is_empty(), map_name.is_empty()) {
            (true, true) => continue,
            (true, false) => map_name.to_string(),
            (false, true) => street.to_string(),
            (false, false) => format!("{street} : {map_name}"),
        };
        table.insert(id, label);
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_book_text_extracts_fields() {
        let text = parse_book_text("等級：Lv.8\\n型態：植物\\n\\n在楓之島常見的怪物。\\n第二行");
        assert_eq!(text.level, Some(8));
        assert_eq!(text.category.as_deref(), Some("植物"));
        assert_eq!(
            text.description.as_deref(),
            Some("在楓之島常見的怪物。\n第二行")
        );
    }

    #[test]
    fn parse_book_text_tolerates_missing_parts() {
        let text = parse_book_text("等級：Lv.???\\n\\n只有描述");
        assert_eq!(text.level, None);
        assert_eq!(text.category, None);
        assert_eq!(text.description.as_deref(), Some("只有描述"));

        let empty = parse_book_text("");
        assert_eq!(empty.level, None);
        assert_eq!(empty.description, None);
    }

    #[test]
    fn png_dimensions_reads_ihdr() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&13u32.to_be_bytes()); // IHDR length
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&32u32.to_be_bytes());
        png.extend_from_slice(&24u32.to_be_bytes());
        assert_eq!(png_dimensions(&png), Some((32, 24)));
        assert_eq!(png_dimensions(b"not a png"), None);
    }
}
