//! 商城（`Etc/TW/Commodity`）聚合：解析商品清單，依道具 id 規則
//! 補上名稱（`String/TW/Item`、`String/TW/Pet`）與 icon 圖，
//! 回傳前端商城檢視所需的一切。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::Engine;
use serde::Serialize;

use crate::catalog::{DataService, Result};

/// 名稱表懶載入快取（字串表只需解析一次；跟隨 DataService 生命週期，
/// 更換資料集時自動重建）。
#[derive(Default)]
pub(crate) struct CommodityNameCache(Mutex<Option<Arc<NameTables>>>);

/// 道具 id → 名稱；寵物名稱在獨立的 `String/TW/Pet` 表。
struct NameTables {
    items: HashMap<u32, String>,
    pets: HashMap<u32, String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommodityItem {
    /// 商城序號（前幾碼即分類代碼，清單依此排序）。
    pub sn: i64,
    pub item_id: u32,
    pub count: i64,
    pub price: i64,
    /// 道具名稱；字串表查無時為 None。
    pub name: Option<String>,
    /// 圖示的 PNG base64：`info/icon`（備援 `info/iconRaw`）；取圖失敗時為 None。
    pub icon_base64: Option<String>,
}

impl DataService {
    /// 商城商品清單：解析 `Etc/TW/Commodity`，依 SN 排序並補齊名稱與 icon。
    /// 資料集沒有此資產時回傳空清單。
    pub fn get_commodity_items(&self) -> Result<Vec<CommodityItem>> {
        let Ok(node) = self.get_node("Etc/TW/Commodity") else {
            return Ok(Vec::new());
        };
        let Some(text) = node.text_asset else {
            return Ok(Vec::new());
        };
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str(strip_bom(&text)) else {
            return Ok(Vec::new());
        };

        let mut raw: Vec<(i64, u32, i64, i64)> = map
            .values()
            .filter_map(|entry| {
                let sn = entry.get("SN")?.as_i64()?;
                let item_id = entry.get("ItemId")?.as_u64()? as u32;
                let count = entry.get("Count").and_then(|v| v.as_i64()).unwrap_or(1);
                let price = entry.get("Price").and_then(|v| v.as_i64()).unwrap_or(0);
                Some((sn, item_id, count, price))
            })
            .collect();
        raw.sort_by_key(|(sn, ..)| *sn);

        let names = self.commodity_names()?;
        Ok(raw
            .into_iter()
            .map(|(sn, item_id, count, price)| {
                let name = names
                    .items
                    .get(&item_id)
                    .or_else(|| names.pets.get(&item_id))
                    .cloned();
                let icon_base64 = self
                    .icon_candidates(item_id)
                    .into_iter()
                    .find_map(|path| self.get_image(&path).ok().flatten().map(encode_png));
                CommodityItem {
                    sn,
                    item_id,
                    count,
                    price,
                    name,
                    icon_base64,
                }
            })
            .collect())
    }

    /// 依道具 id 推出 icon 的候選節點路徑（按優先順序，逐一嘗試）。
    fn icon_candidates(&self, item_id: u32) -> Vec<String> {
        // 9 開頭（套裝組、欄位擴充等）：Special img 內的節點是 7 碼 id，
        // 且 icon 直接掛在節點下（沒有 info 層），與其他 img 不同。
        if item_id / 1_000_000 == 9 {
            let base = format!("Item/Special/0{}/{item_id:07}", item_id / 10_000);
            return ["/icon", "/iconRaw", "/info/icon", "/info/iconRaw"]
                .into_iter()
                .map(|suffix| format!("{base}{suffix}"))
                .collect();
        }
        item_path_candidates(item_id, |path| self.catalog.entries.contains_key(path))
            .into_iter()
            .flat_map(|base| [format!("{base}/info/icon"), format!("{base}/info/iconRaw")])
            .collect()
    }

    /// 懶載入名稱表；資料集沒有字串表時視為空表。
    fn commodity_names(&self) -> Result<Arc<NameTables>> {
        let mut guard = self
            .commodity_name_cache
            .0
            .lock()
            .map_err(|error| crate::catalog::CatalogError::CacheInvalid(error.to_string()))?;
        if let Some(tables) = guard.as_ref() {
            return Ok(tables.clone());
        }
        let tables = Arc::new(NameTables {
            items: load_names(self, "String/TW/Item"),
            pets: load_names(self, "String/TW/Pet"),
        });
        *guard = Some(tables.clone());
        Ok(tables)
    }
}

/// 依道具 id 推出可能的（img 或 img 內節點）路徑，按優先順序。
/// `has_path` 用來判斷 5 開頭的寵物獨立 img 是否存在。
pub(crate) fn item_path_candidates(item_id: u32, has_path: impl Fn(&str) -> bool) -> Vec<String> {
    let mut bases: Vec<String> = Vec::new();
    match item_id / 1_000_000 {
        // 裝備：Character/<分類目錄>/<8 碼 id>。
        1 => {
            let dir = match item_id / 10_000 {
                100 => Some("Cap"),
                101..=103 => Some("Accessory"),
                104 => Some("Coat"),
                105 => Some("Longcoat"),
                106 => Some("Pants"),
                107 => Some("Shoes"),
                108 => Some("Glove"),
                109 => Some("Shield"),
                110 => Some("Cape"),
                111 => Some("Ring"),
                112..=115 => Some("Accessory"),
                121..=179 => Some("Weapon"),
                180 => Some("PetEquip"),
                _ => None,
            };
            if let Some(dir) = dir {
                bases.push(format!("Character/{dir}/{item_id:08}"));
            }
        }
        2 => bases.push(format!("Item/Consume/{:04}/{item_id:08}", item_id / 10_000)),
        3 => bases.push(format!("Item/Install/{:04}/{item_id:08}", item_id / 10_000)),
        4 => bases.push(format!("Item/Etc/{:04}/{item_id:08}", item_id / 10_000)),
        // 5 開頭：寵物是一 id 一 img，其餘落在 Cash 分區 img。
        5 => {
            let pet = format!("Item/Pet/{item_id}");
            if has_path(&pet) {
                bases.push(pet);
            }
            bases.push(format!("Item/Cash/{:04}/{item_id:08}", item_id / 10_000));
        }
        // 9 開頭（套裝組、欄位擴充等）：Special img 內的節點是 7 碼 id。
        9 => bases.push(format!("Item/Special/0{}/{item_id:07}", item_id / 10_000)),
        _ => {}
    }
    bases
}

/// icon PNG → base64（與 item_tooltip 的圖示輸出一致）。
fn encode_png(png: Vec<u8>) -> String {
    base64::engine::general_purpose::STANDARD.encode(png)
}

/// 部分 text_asset 帶 UTF-8 BOM（如 Etc/TW/Commodity），serde_json 不接受，先去掉。
fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// 解析字串表 text_asset（`{id: {name: ...}}`）成 id → 名稱；失敗時回空表。
pub(crate) fn load_names(service: &DataService, path: &str) -> HashMap<u32, String> {
    let mut table = HashMap::new();
    let Ok(node) = service.get_node(path) else {
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
        let Some(name) = value.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        table.insert(id, name.to_string());
    }
    table
}
