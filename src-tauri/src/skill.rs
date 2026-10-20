//! 技能瀏覽器（`Skill/*`）聚合：走訪各職業 img 的 `skill/<id>` 節點，
//! 補上 `String/TW/Skill` 的名稱 / 說明文字與 `Character/00002000`
//! 的動作延遲（各影格 |delay| 加總），回傳前端技能瀏覽所需的資料。

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::catalog::{CatalogError, DataService, Result};
use crate::wzjs::{Value, WzNode};

/// 技能字串表資產路徑（TextAsset，UTF-8 明文 JSON：id → {Name, Desc, H, LevelDesc}）。
const STRING_SKILL: &str = "String/TW/Skill";
/// 技能動作影格來源（`<動作名>/<影格>/delay`）。
const CHARACTER: &str = "Character/00002000";

/// 排除的非玩家 Skill img（`Skill/` 之後的 job 段完全比對）。
/// `Skill/Dragon/*` 另外以前綴排除（見 `is_excluded_job`）。
const EXCLUDED_JOBS: &[&str] = &[
    "Attacktype",
    "BFSkill",
    "ItemSkill",
    "MCGuardian",
    "MCSkill",
    "MobSkill",
];

/// 技能清單快取（只需組裝一次；跟隨 DataService 生命週期，更換資料集自動重建）。
#[derive(Default)]
pub(crate) struct SkillBookCache(pub Mutex<Option<Arc<Vec<SkillJobGroup>>>>);

/// 技能字串表懶載入快取（book 與 detail 共用）。
#[derive(Default)]
pub(crate) struct SkillStringCache(pub Mutex<Option<Arc<SkillStringTables>>>);

/// `String/TW/Skill` 解析後的字串表：技能 id（無補零）→ 文字。
pub(crate) type SkillStringTables = HashMap<String, SkillStrings>;

/// 單一技能的文字（名稱 / 說明 / 簡述 / 各等級說明）。
/// 字串中的換行是字面 `\n` 兩字元，保留原文由前端處理。
#[derive(Default)]
pub(crate) struct SkillStrings {
    name: Option<String>,
    desc: Option<String>,
    h: Option<String>,
    level_desc: BTreeMap<String, String>,
}

/// 技能分類（依資料特徵推斷）：被動 / Buff・功能 / 主動。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillCategory {
    Passive,
    Support,
    Active,
}

/// 技能清單中的一個技能。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSummary {
    pub id: String,
    pub job: String,
    pub name: Option<String>,
    pub max_level: i64,
    pub skill_type: Option<i64>,
    pub category: SkillCategory,
}

/// 單一職業（job）的技能群組。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillJobGroup {
    pub job: String,
    pub skills: Vec<SkillSummary>,
}

/// 單一技能的完整明細。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDetail {
    pub id: String,
    pub job: String,
    pub name: Option<String>,
    pub desc: Option<String>,
    pub h: Option<String>,
    pub level_desc: BTreeMap<String, String>,
    pub max_level: i64,
    pub skill_type: Option<i64>,
    /// 技能引用的角色動作名（`action` 節點字串值或子節點 0/1/... 各一個）。
    pub actions: Vec<String>,
    /// 各動作的總延遲（`Character/00002000` 影格 |delay| 加總）；查無該動作則缺項。
    pub action_delays: BTreeMap<String, i64>,
    /// 等級 → 數值 props（數字保留數字、字串保留字串、vector 轉 [x,y]；canvas 略過）。
    pub levels: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
}

impl DataService {
    /// 技能清單：走訪所有未排除的 `Skill/*` img，組出各 job 的技能清單
    /// （名稱查字串表，查不到給 None 仍保留）；結果快取。
    pub fn get_skill_book(&self) -> Result<Arc<Vec<SkillJobGroup>>> {
        let mut guard = self
            .skill_book_cache
            .0
            .lock()
            .map_err(|error| CatalogError::CacheInvalid(error.to_string()))?;
        if let Some(book) = guard.as_ref() {
            return Ok(book.clone());
        }
        let book = Arc::new(self.load_skill_book()?);
        *guard = Some(book.clone());
        Ok(book)
    }

    fn load_skill_book(&self) -> Result<Vec<SkillJobGroup>> {
        let strings = self.skill_strings()?;
        let mut jobs: Vec<String> = self
            .catalog
            .entries
            .keys()
            .filter_map(|key| key.strip_prefix("Skill/"))
            .filter(|job| !is_excluded_job(job))
            .map(str::to_string)
            .collect();
        jobs.sort_by(|a, b| job_sort_key(a).cmp(&job_sort_key(b)));

        let mut groups = Vec::new();
        for job in jobs {
            let tree = self.tree_for(&format!("Skill/{job}"))?;
            let Some(skill_root) = tree.find("skill") else {
                continue;
            };
            let mut skills: Vec<SkillSummary> = skill_root
                .children
                .iter()
                .map(|skill| SkillSummary {
                    id: skill.name.clone(),
                    job: job.clone(),
                    name: find_strings(&strings, &skill.name).and_then(|s| s.name.clone()),
                    max_level: skill
                        .find("level")
                        .map(|level| level.children.len() as i64)
                        .unwrap_or(0),
                    skill_type: skill
                        .find("skillType")
                        .and_then(|node| value_as_i64(&node.value)),
                    category: categorize(skill),
                })
                .collect();
            skills.sort_by(|a, b| id_sort_key(&a.id).cmp(&id_sort_key(&b.id)));
            groups.push(SkillJobGroup { job, skills });
        }
        Ok(groups)
    }

    /// 單一技能完整明細：文字、動作與延遲、每級數值 props。
    /// 找不到 img 或技能時回 Err。
    pub fn get_skill_detail(&self, job: &str, skill_id: &str) -> Result<SkillDetail> {
        let img_key = format!("Skill/{job}");
        if !self.catalog.entries.contains_key(&img_key) || is_excluded_job(job) {
            return Err(CatalogError::PathNotFound(img_key));
        }
        let tree = self.tree_for(&img_key)?;
        let skill = tree
            .find("skill")
            .and_then(|root| {
                root.children
                    .iter()
                    .find(|node| same_skill_id(&node.name, skill_id))
            })
            .ok_or_else(|| CatalogError::PathNotFound(format!("{img_key}/skill/{skill_id}")))?;

        let strings = self.skill_strings()?;
        let strinfo = find_strings(&strings, &skill.name);
        let actions = skill_actions(skill);
        let action_delays = self.action_delays(&actions);
        let levels = skill
            .find("level")
            .map(|level| {
                level
                    .children
                    .iter()
                    .map(|lv| {
                        let props = lv
                            .children
                            .iter()
                            .filter(|prop| !matches!(prop.value, Value::Canvas))
                            .map(|prop| (prop.name.clone(), node_to_json(prop)))
                            .collect();
                        (lv.name.clone(), props)
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(SkillDetail {
            id: skill.name.clone(),
            job: job.to_string(),
            name: strinfo.and_then(|s| s.name.clone()),
            desc: strinfo.and_then(|s| s.desc.clone()),
            h: strinfo.and_then(|s| s.h.clone()),
            level_desc: strinfo.map(|s| s.level_desc.clone()).unwrap_or_default(),
            max_level: skill
                .find("level")
                .map(|level| level.children.len() as i64)
                .unwrap_or(0),
            skill_type: skill
                .find("skillType")
                .and_then(|node| value_as_i64(&node.value)),
            actions,
            action_delays,
            levels,
        })
    }

    /// 各動作的總延遲：`Character/00002000/<動作>/<影格>/delay` 的 |delay| 加總。
    /// 資料集缺角色 img 或查無該動作時，該動作缺項（不報錯）。
    fn action_delays(&self, actions: &[String]) -> BTreeMap<String, i64> {
        let mut out = BTreeMap::new();
        if actions.is_empty() || !self.catalog.entries.contains_key(CHARACTER) {
            return out;
        }
        let Ok(tree) = self.tree_for(CHARACTER) else {
            return out;
        };
        for name in actions {
            let Some(action) = tree.find(name) else {
                continue;
            };
            let total: i64 = action
                .children
                .iter()
                .filter_map(|frame| frame.find("delay").and_then(|n| value_as_i64(&n.value)))
                .map(i64::abs)
                .sum();
            out.insert(name.clone(), total);
        }
        out
    }

    /// 懶載入技能字串表；資料集缺字串表時視為空表。
    fn skill_strings(&self) -> Result<Arc<SkillStringTables>> {
        let mut guard = self
            .skill_string_cache
            .0
            .lock()
            .map_err(|error| CatalogError::CacheInvalid(error.to_string()))?;
        if let Some(tables) = guard.as_ref() {
            return Ok(tables.clone());
        }
        let tables = Arc::new(load_skill_strings(self));
        *guard = Some(tables.clone());
        Ok(tables)
    }
}

/// 非玩家 Skill img 排除規則：完全比對 EXCLUDED_JOBS，或 `Dragon/*` 前綴。
fn is_excluded_job(job: &str) -> bool {
    EXCLUDED_JOBS.contains(&job) || job == "Dragon" || job.starts_with("Dragon/")
}

/// 技能 id 正規化：去掉補零（全零視為 "0"），讓 `0001000` 與 `1000` 視為同一 id。
fn normalize_id(id: &str) -> &str {
    let trimmed = id.trim_start_matches('0');
    if trimmed.is_empty() {
        "0"
    } else {
        trimmed
    }
}

fn same_skill_id(a: &str, b: &str) -> bool {
    a == b || normalize_id(a) == normalize_id(b)
}

/// 字串表查找：Skill img 的 id 有補零（如 `0001000`），
/// `String/TW/Skill` 的鍵沒有 → 兩種都試。
fn find_strings<'a>(table: &'a SkillStringTables, id: &str) -> Option<&'a SkillStrings> {
    table.get(id).or_else(|| table.get(normalize_id(id)))
}

/// job 排序鍵：可解析成數字的按數字排前面，其餘按字典序排在後。
fn job_sort_key(job: &str) -> (u8, i64, &str) {
    match job.parse::<i64>() {
        Ok(n) => (0, n, ""),
        Err(_) => (1, 0, job),
    }
}

/// 技能 id 排序鍵：同上，數字 id 按數值排序。
fn id_sort_key(id: &str) -> (u8, i64, &str) {
    job_sort_key(id)
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

/// 技能的 action 清單：`action` 節點的字串值，或子節點 0/1/... 各一個字串。
fn skill_actions(skill: &WzNode) -> Vec<String> {
    let mut actions = Vec::new();
    if let Some(action_node) = skill.find("action") {
        if let Some(s) = as_str(&action_node.value) {
            actions.push(s.to_string());
        }
        for child in &action_node.children {
            if let Some(s) = as_str(&child.value) {
                actions.push(s.to_string());
            }
        }
    }
    actions
}

/// 分類規則（依實際資料特徵歸納，以現版全部技能驗證過）：
/// 1. skillType 1（武器專精）/ 3（終極攻擊等觸發類）→ 被動
/// 2. skillType 2（快速武器等增益）→ Buff・功能
/// 3. 無消耗且無 action → 被動（含 強力投擲/霸王箭 等觸發類：
///    有 damage/prop 但無 mpCon、無 action）
/// 4. 有攻擊特徵（hit/ball 節點，或 damage 類 prop）且有消耗或 action → 主動
/// 5. 其餘（有消耗或 action 但無攻擊特徵）→ Buff・功能
fn categorize(skill: &WzNode) -> SkillCategory {
    match skill
        .find("skillType")
        .and_then(|node| value_as_i64(&node.value))
    {
        Some(1) | Some(3) => return SkillCategory::Passive,
        Some(2) => return SkillCategory::Support,
        _ => {}
    }
    // 全等級 prop 名稱聯集（部分 prop 只出現在高等級）。
    let mut keys = std::collections::BTreeSet::new();
    if let Some(level) = skill.find("level") {
        for lv in &level.children {
            for prop in &lv.children {
                keys.insert(prop.name.as_str());
            }
        }
    }
    const COST_PROPS: &[&str] = &["mpCon", "hpCon", "bulletCon", "moneyCon", "itemCon"];
    const ATTACK_PROPS: &[&str] = &[
        "damage",
        "damagepc",
        "fixdamage",
        "attackCount",
        "bulletCount",
        "ball",
        "hit",
    ];
    let has_cost = COST_PROPS.iter().any(|key| keys.contains(key));
    let has_attack = ATTACK_PROPS.iter().any(|key| keys.contains(key))
        || skill.find("hit").is_some()
        || skill.find("ball").is_some();
    let has_action = skill.find("action").is_some();
    if !has_cost && !has_action {
        return SkillCategory::Passive;
    }
    if has_attack {
        return SkillCategory::Active;
    }
    SkillCategory::Support
}

/// 節點 → JSON 葉值（數字保留數字、字串保留字串、vector 轉 [x,y]）；
/// 帶子節點的 prop 轉巢狀物件；canvas 葉略過（回 None）。
fn node_to_json(node: &WzNode) -> serde_json::Value {
    if node.children.is_empty() {
        return leaf_json(&node.value);
    }
    let obj: serde_json::Map<String, serde_json::Value> = node
        .children
        .iter()
        .filter(|child| !matches!(child.value, Value::Canvas))
        .map(|child| (child.name.clone(), node_to_json(child)))
        .collect();
    serde_json::Value::Object(obj)
}

fn leaf_json(value: &Value) -> serde_json::Value {
    match value {
        Value::SubTree | Value::Canvas => serde_json::Value::Null,
        Value::U8(n) => serde_json::json!(n),
        Value::U16(n) => serde_json::json!(n),
        Value::Int(n) => serde_json::json!(n),
        Value::Long(n) => serde_json::json!(n),
        Value::Double(f) => serde_json::json!(f),
        Value::Float2(x, y) | Value::Vector(x, y) => serde_json::json!([x, y]),
        Value::Int2(a, b) => serde_json::json!([a, b]),
        Value::Str(s) | Value::Uol(s) => serde_json::json!(s),
    }
}

/// 部分 text_asset 帶 UTF-8 BOM，serde_json 不接受，先去掉。
fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// 解析 `String/TW/Skill`（`{id: {Name, Desc, H, LevelDesc}}`）。
fn load_skill_strings(service: &DataService) -> SkillStringTables {
    let mut table = HashMap::new();
    let Ok(node) = service.get_node(STRING_SKILL) else {
        return table;
    };
    let Some(text) = node.text_asset else {
        return table;
    };
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str(strip_bom(&text)) else {
        return table;
    };
    for (id, value) in map {
        let get_str = |key: &str| value.get(key).and_then(|v| v.as_str()).map(str::to_string);
        let level_desc = value
            .get("LevelDesc")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        table.insert(
            id,
            SkillStrings {
                name: get_str("Name"),
                desc: get_str("Desc"),
                h: get_str("H"),
                level_desc,
            },
        );
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_id_strips_zero_padding() {
        assert_eq!(normalize_id("0001000"), "1000");
        assert_eq!(normalize_id("1000"), "1000");
        assert_eq!(normalize_id("000"), "0");
        assert!(same_skill_id("0001000", "1000"));
        assert!(same_skill_id("1000", "1000"));
        assert!(!same_skill_id("1000", "1001"));
    }

    #[test]
    fn find_strings_tries_padded_and_unpadded_keys() {
        let mut table = SkillStringTables::new();
        table.insert(
            "1000".to_string(),
            SkillStrings {
                name: Some("魔力恢復".to_string()),
                ..Default::default()
            },
        );
        // 補零 id 能查到無補零的鍵
        assert_eq!(
            find_strings(&table, "0001000").and_then(|s| s.name.as_deref()),
            Some("魔力恢復")
        );
        assert!(find_strings(&table, "9999").is_none());
    }

    #[test]
    fn job_sort_numeric_before_lexicographic() {
        let mut jobs = vec!["abc", "200", "20", "100", "Dragon/X"];
        jobs.sort_by(|a, b| job_sort_key(a).cmp(&job_sort_key(b)));
        assert_eq!(jobs, vec!["20", "100", "200", "Dragon/X", "abc"]);
    }

    #[test]
    fn skill_ids_sort_by_numeric_value() {
        let mut ids = vec!["0001001", "0001000", "1000000"];
        ids.sort_by(|a, b| id_sort_key(a).cmp(&id_sort_key(b)));
        assert_eq!(ids, vec!["0001000", "0001001", "1000000"]);
    }

    #[test]
    fn excluded_jobs_match_rules() {
        for job in [
            "Attacktype",
            "BFSkill",
            "ItemSkill",
            "MCGuardian",
            "MCSkill",
            "MobSkill",
        ] {
            assert!(is_excluded_job(job), "{job} 應排除");
        }
        assert!(is_excluded_job("Dragon/2200"));
        assert!(!is_excluded_job("100"));
        assert!(!is_excluded_job("000"));
        assert!(!is_excluded_job("800104"));
    }

    #[test]
    fn leaf_json_keeps_scalars_and_vectors() {
        assert_eq!(leaf_json(&Value::Int(30)), serde_json::json!(30));
        assert_eq!(
            leaf_json(&Value::Str("hs".to_string())),
            serde_json::json!("hs")
        );
        assert_eq!(
            leaf_json(&Value::Vector(1.0, -2.0)),
            serde_json::json!([1.0, -2.0])
        );
        assert_eq!(leaf_json(&Value::Canvas), serde_json::Value::Null);
    }

    fn make_node(name: &str, value: Value, children: Vec<WzNode>) -> WzNode {
        WzNode {
            name: name.to_string(),
            value,
            path: None,
            children,
            type_tag: 0,
        }
    }

    fn leaf(name: &str) -> WzNode {
        make_node(name, Value::Int(1), Vec::new())
    }

    fn level1(props: Vec<WzNode>) -> WzNode {
        make_node(
            "level",
            Value::SubTree,
            vec![make_node("1", Value::SubTree, props)],
        )
    }

    fn skill_with(children: Vec<WzNode>) -> WzNode {
        make_node("1000000", Value::SubTree, children)
    }

    #[test]
    fn categorize_skill_type_overrides_features() {
        for (st, expected) in [
            (1, SkillCategory::Passive),
            (2, SkillCategory::Support),
            (3, SkillCategory::Passive),
        ] {
            let skill = skill_with(vec![
                make_node("skillType", Value::Int(st), vec![]),
                level1(vec![leaf("mpCon"), leaf("damage")]),
            ]);
            assert_eq!(categorize(&skill), expected, "skillType {st}");
        }
    }

    #[test]
    fn categorize_passive_without_cost_or_action() {
        // 強力投擲型：有 damage/prop 但無消耗、無 action → 觸發類被動
        let skill = skill_with(vec![level1(vec![leaf("damage"), leaf("prop")])]);
        assert_eq!(categorize(&skill), SkillCategory::Passive);
        // 無任何特徵（向下跳躍型）→ 被動
        assert_eq!(categorize(&skill_with(vec![])), SkillCategory::Passive);
    }

    #[test]
    fn categorize_active_with_attack_markers() {
        // 一般攻擊技：damage + mpCon
        let skill = skill_with(vec![level1(vec![leaf("damage"), leaf("mpCon")])]);
        assert_eq!(categorize(&skill), SkillCategory::Active);
        // 無 damage 但有 hit 節點（斷魂箭型）→ 主動
        let skill = skill_with(vec![
            level1(vec![leaf("mpCon")]),
            make_node("hit", Value::SubTree, vec![]),
        ]);
        assert_eq!(categorize(&skill), SkillCategory::Active);
    }

    #[test]
    fn categorize_support_with_cost_but_no_attack() {
        // 瞬間移動型：mpCon + range，無攻擊特徵 → Buff・功能
        let skill = skill_with(vec![level1(vec![leaf("mpCon"), leaf("range")])]);
        assert_eq!(categorize(&skill), SkillCategory::Support);
        // 無消耗但有 action（魔力再生型）→ Buff・功能
        let skill = skill_with(vec![
            make_node("action", Value::Str("alert".to_string()), vec![]),
            level1(vec![leaf("x")]),
        ]);
        assert_eq!(categorize(&skill), SkillCategory::Support);
    }
}
