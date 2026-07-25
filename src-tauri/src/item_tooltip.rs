//! 裝備 tooltip 聚合：給定節點路徑，彙整 info 屬性、icon 圖與
//! `String/TW/Item` 的名稱/說明，回傳前端 tooltip 卡片所需的一切。
//! 欄位解碼邏輯對齊 Harepacker-MPST 的 `wz/item_tooltip.rs`（gear 部分）。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::Engine;
use serde::Serialize;

use crate::catalog::{DataService, Result};
use crate::wzjs::{Value, WzNode};

/// 對齊 MPST 的 GEAR_STAT_LABELS（順序即顯示順序）。
const GEAR_STAT_LABELS: &[(&str, &str)] = &[
    ("incSTR", "STR"),
    ("incDEX", "DEX"),
    ("incINT", "INT"),
    ("incLUK", "LUK"),
    ("incMHP", "MaxHP"),
    ("incMMP", "MaxMP"),
    ("incPAD", "攻擊力"),
    ("incMAD", "魔法攻擊力"),
    ("incPDD", "物理防禦力"),
    ("incMDD", "魔法防禦力"),
    ("incACC", "命中值"),
    ("incEVA", "迴避值"),
    ("incSpeed", "移動速度"),
    ("incJump", "跳躍力"),
    ("tuc", "可升級次數"),
];

const ALL_JOB_LABELS: &[&str] = &["初心者", "劍士", "法師", "弓箭手", "盜賊", "海盜"];
const JOB_MASK_LABELS: &[(i64, &str)] = &[
    (1, "劍士"),
    (2, "法師"),
    (4, "弓箭手"),
    (8, "盜賊"),
    (16, "海盜"),
];

/// `String/TW/Item` text_asset 解析後的名稱表：道具 id →（名稱, 說明）。
type ItemNameTable = HashMap<u32, (String, Option<String>)>;

/// 名稱表懶載入快取（1.2MB JSON 只需解析一次；跟隨 DataService 生命週期，
/// 更換資料集時自動重建）。
#[derive(Default)]
pub(crate) struct TooltipNameCache(Mutex<Option<Arc<ItemNameTable>>>);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TooltipStatLine {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemTooltip {
    pub item_id: String,
    /// 裝備 img 根路徑（例如 `Character/Weapon/01302000`）。
    pub img_path: String,
    pub title: String,
    pub description: Option<String>,
    pub category_line: String,
    /// `info/icon` 的 PNG base64；取圖失敗時為 None。
    pub icon_base64: Option<String>,
    pub required_level: i64,
    pub required_str: i64,
    pub required_dex: i64,
    pub required_int: i64,
    pub required_luk: i64,
    pub required_jobs: Vec<String>,
    pub attribute_lines: Vec<String>,
    pub attack_speed_label: Option<String>,
    pub scroll_count: Option<i64>,
    pub stat_lines: Vec<TooltipStatLine>,
}

impl DataService {
    /// 路徑指向裝備（Character 下 8 碼數字 img 或其內部節點）時回傳 tooltip；
    /// 非裝備路徑回 Ok(None)。
    pub fn get_item_tooltip(&self, path: &str) -> Result<Option<ItemTooltip>> {
        let Some((img_path, category_hint)) = self.gear_img_path(path) else {
            return Ok(None);
        };
        let tree = self.tree_for(&img_path)?;
        let Some(info) = tree.find("info") else {
            return Ok(None);
        };
        let item_id = img_path
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string();

        let numeric_id = item_id.parse::<u32>().unwrap_or(0);
        let names = self.item_names()?;
        let (name, description) = names
            .get(&numeric_id)
            .map(|(n, d)| (n.clone(), d.clone()))
            .unwrap_or_else(|| (item_id.clone(), None));
        let mut title = name;
        match prop_i64(info, "reqGender") {
            Some(0) => title.push_str(" (男)"),
            Some(1) => title.push_str(" (女)"),
            _ => {}
        }

        let icon_base64 = self
            .get_image(&format!("{img_path}/info/icon"))?
            .map(|png| base64::engine::general_purpose::STANDARD.encode(png));

        let category_line = gear_category_line(&item_id, &category_hint);
        let attack_speed_label = prop_i64(info, "attackSpeed").map(attack_speed_label);
        let scroll_count = prop_i64(info, "tuc");

        Ok(Some(ItemTooltip {
            item_id,
            img_path,
            title,
            description: description.filter(|d| !d.trim().is_empty()),
            category_line,
            icon_base64,
            required_level: prop_i64(info, "reqLevel").unwrap_or(0),
            required_str: prop_i64(info, "reqSTR").unwrap_or(0),
            required_dex: prop_i64(info, "reqDEX").unwrap_or(0),
            required_int: prop_i64(info, "reqINT").unwrap_or(0),
            required_luk: prop_i64(info, "reqLUK").unwrap_or(0),
            required_jobs: required_job_labels(prop_i64(info, "reqJob")),
            attribute_lines: gear_attribute_lines(info),
            attack_speed_label,
            scroll_count,
            stat_lines: gear_stat_lines(info),
        }))
    }

    /// 把任意路徑收斂成裝備 img 根路徑；順便回傳分類提示（目錄名）。
    fn gear_img_path(&self, path: &str) -> Option<(String, String)> {
        let segments: Vec<&str> = path.split('/').collect();
        if segments.first() != Some(&"Character") {
            return None;
        }
        let is_id = |s: &str| s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit());
        let (img_path, hint) = if segments.len() >= 2 && is_id(segments[1]) {
            (format!("Character/{}", segments[1]), String::new())
        } else if segments.len() >= 3 && is_id(segments[2]) {
            (
                format!("Character/{}/{}", segments[1], segments[2]),
                segments[1].to_string(),
            )
        } else {
            return None;
        };
        if !self.catalog.entries.contains_key(&img_path) {
            return None;
        }
        Some((img_path, hint))
    }

    /// 懶載入 `String/TW/Item` 名稱表；資料集沒有此資產時視為空表。
    fn item_names(&self) -> Result<Arc<ItemNameTable>> {
        let mut guard = self
            .tooltip_name_cache
            .0
            .lock()
            .map_err(|error| crate::catalog::CatalogError::CacheInvalid(error.to_string()))?;
        if let Some(table) = guard.as_ref() {
            return Ok(table.clone());
        }
        let table = Arc::new(self.load_item_names()?);
        *guard = Some(table.clone());
        Ok(table)
    }

    fn load_item_names(&self) -> Result<ItemNameTable> {
        let mut table = ItemNameTable::new();
        let Ok(node) = self.get_node("String/TW/Item") else {
            return Ok(table);
        };
        let Some(text) = node.text_asset else {
            return Ok(table);
        };
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&text) else {
            return Ok(table);
        };
        for (key, value) in map {
            let Ok(id) = key.parse::<u32>() else {
                continue;
            };
            let Some(name) = value.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            let desc = value
                .get("desc")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            table.insert(id, (name.to_string(), desc));
        }
        Ok(table)
    }
}

fn prop<'a>(info: &'a WzNode, name: &str) -> Option<&'a WzNode> {
    info.children
        .iter()
        .find(|child| child.name.eq_ignore_ascii_case(name))
}

fn prop_i64(info: &WzNode, name: &str) -> Option<i64> {
    prop(info, name).and_then(|node| value_as_i64(&node.value))
}

fn prop_bool(info: &WzNode, name: &str) -> bool {
    prop_i64(info, name).is_some_and(|value| value != 0)
}

fn value_as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::U8(v) => Some(i64::from(*v)),
        Value::U16(v) => Some(i64::from(*v)),
        Value::Int(v) => Some(i64::from(*v)),
        Value::Int2(a, _) => Some(i64::from(*a)),
        Value::Double(v) => Some(*v as i64),
        _ => None,
    }
}

fn gear_attribute_lines(info: &WzNode) -> Vec<String> {
    let mut lines = Vec::new();
    if prop_bool(info, "only") {
        lines.push("專屬道具".to_string());
    }
    if prop_bool(info, "cash") {
        lines.push("點數道具".to_string());
    }
    if prop_bool(info, "timeLimited") {
        lines.push("限時道具".to_string());
    }
    if prop_bool(info, "tradeBlock") {
        lines.push("不可交易".to_string());
    } else if prop_bool(info, "accountSharable") {
        lines.push("帳號內可移動".to_string());
    }
    if prop_bool(info, "onlyEquip") {
        lines.push("只能單獨使用".to_string());
    }
    lines
}

fn gear_stat_lines(info: &WzNode) -> Vec<TooltipStatLine> {
    GEAR_STAT_LABELS
        .iter()
        .filter_map(|(name, label)| {
            let value = prop_i64(info, name)?;
            if value == 0 {
                return None;
            }
            Some(TooltipStatLine {
                label: (*label).to_string(),
                value: if value > 0 {
                    format!("+{value}")
                } else {
                    value.to_string()
                },
            })
        })
        .collect()
}

fn required_job_labels(req_job: Option<i64>) -> Vec<String> {
    let all = || {
        ALL_JOB_LABELS
            .iter()
            .map(|label| (*label).to_string())
            .collect()
    };
    match req_job {
        None | Some(0) => all(),
        Some(job) => {
            let labels: Vec<String> = JOB_MASK_LABELS
                .iter()
                .filter(|(mask, _)| job & mask != 0)
                .map(|(_, label)| (*label).to_string())
                .collect();
            if labels.is_empty() {
                all()
            } else {
                labels
            }
        }
    }
}

fn attack_speed_label(value: i64) -> String {
    match value {
        2 | 3 => "比較快".to_string(),
        4 | 5 => "快".to_string(),
        6 => "普通".to_string(),
        7 | 8 => "慢".to_string(),
        9 => "比較慢".to_string(),
        value if value < 2 => "非常快".to_string(),
        _ => "非常慢".to_string(),
    }
}

/// 裝備分類行：武器給「武器分類 : 單手劍」，其餘給「裝備分類 : 帽子」。
fn gear_category_line(item_id: &str, category_hint: &str) -> String {
    if let Some(weapon_label) = weapon_type_label(item_id) {
        return format!("武器分類 : {weapon_label}");
    }
    format!(
        "裝備分類 : {}",
        gear_category_label(item_id, category_hint)
    )
}

/// 8 碼裝備 id 的前 3 位分類碼（id 開頭必為 0，例如 01302000 → 130）。
fn id_prefix3(item_id: &str) -> u32 {
    item_id.parse::<u32>().unwrap_or(0) / 10000
}

fn gear_category_label(item_id: &str, category_hint: &str) -> String {
    match id_prefix3(item_id) {
        100 => "帽子".to_string(),
        101 => "臉飾".to_string(),
        102 => "眼飾".to_string(),
        103 => "耳環".to_string(),
        104 => "上衣".to_string(),
        105 => "套服".to_string(),
        106 => "褲子".to_string(),
        107 => "鞋子".to_string(),
        108 => "手套".to_string(),
        109 => "盾牌".to_string(),
        110 => "披風".to_string(),
        111 => "戒指".to_string(),
        112 => "吊墜".to_string(),
        113 => "腰帶".to_string(),
        114 => "勳章".to_string(),
        115 => "肩飾".to_string(),
        121..=159 => "武器".to_string(),
        _ => match category_hint.to_lowercase().as_str() {
            "accessory" => "飾品".to_string(),
            "cap" => "帽子".to_string(),
            "coat" => "上衣".to_string(),
            "longcoat" => "套服".to_string(),
            "pants" => "褲子".to_string(),
            "shoes" => "鞋子".to_string(),
            "glove" => "手套".to_string(),
            "shield" => "盾牌".to_string(),
            "cape" => "披風".to_string(),
            "weapon" => "武器".to_string(),
            "ring" => "戒指".to_string(),
            "petequip" => "寵物裝備".to_string(),
            "tamingmob" => "騎寵".to_string(),
            "face" => "臉型".to_string(),
            "hair" => "髮型".to_string(),
            _ => {
                // Character 直屬的 0002xxxx（臉型）/0003~0005（髮型）。
                match item_id.parse::<u32>().unwrap_or(0) / 10000 {
                    2 => "臉型".to_string(),
                    3..=5 => "髮型".to_string(),
                    _ => "裝備".to_string(),
                }
            }
        },
    }
}

fn weapon_type_label(item_id: &str) -> Option<&'static str> {
    match id_prefix3(item_id) {
        130 => Some("單手劍"),
        131 => Some("單手斧"),
        132 => Some("單手棍"),
        133 => Some("短劍"),
        134 => Some("刀"),
        136 => Some("手杖"),
        137 => Some("短杖"),
        138 => Some("長杖"),
        140 => Some("雙手劍"),
        141 => Some("雙手斧"),
        142 => Some("雙手棍"),
        143 => Some("槍"),
        144 => Some("矛"),
        145 => Some("弓"),
        146 => Some("弩"),
        147 => Some("拳套"),
        148 => Some("指虎"),
        149 => Some("火槍"),
        _ => None,
    }
}
