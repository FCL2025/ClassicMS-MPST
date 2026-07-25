//! 技能資料匯出工具。
//!
//! 彙整三個來源成單一 JSON（預設輸出到 Discord 素材資料夾）：
//! - `Skill/*`：技能本體（各職業 img 的 `skill/<id>` 節點，含 action 與每級數值）
//! - `String/TW/Skill`：技能名稱 / 說明（TextAsset，UTF-8 明文 JSON）
//! - `Character/00002000`：技能動作影格，加總 |delay|（負號無視）得到技能總延遲
//!
//! 用法：`cargo run --release --example skill_dump -- [out.json]`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use msclassic::catalog::DataService;
use msclassic::unity::bundle::Bundle;
use msclassic::unity::objects;
use msclassic::wzjs::{Value, WzNode};
use serde_json::{json, Map};

const DATA_DIR: &str =
    r"C:\Program Files\Gamania\maplestory_classic\Maplestory_Classic_Data\StreamingAssets\aa\w";
const STRING_SKILL: &str = "String/TW/Skill";
const CHARACTER: &str = "Character/00002000";

/// 預設輸出目的地：楓之谷經典版 Discord 素材資料夾。
const DEFAULT_OUT: &str = r"C:\Users\cl202\Desktop\楓之谷經典版-Discord\經典版技能改動\skills.json";

/// 只匯出這些職業（job）的技能。
const JOB_WHITELIST: &[&str] = &[
    "100", "200", "300", "400", "500", //
    "110", "120", "130", "111", "121", "131", "112", "122", "132", //
    "210", "220", "230", "211", "221", "231", "212", "222", "232", //
    "310", "320", "311", "321", "312", "322", //
    "410", "420", "411", "421", "412", "422", //
    "510", "520", "511", "521", "512", "522",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_OUT));

    let cache = PathBuf::from("out/export_catalog_cache.json");
    let (svc, info) = DataService::open(Path::new(DATA_DIR), &cache)?;
    eprintln!(
        "索引 {} 筆（{}，{}ms）",
        info.entry_count,
        if info.from_cache { "快取" } else { "重建" },
        info.elapsed_ms
    );

    // 1. 技能名稱/說明（TextAsset 明文 JSON）
    let strings = load_skill_strings(&svc)?;
    eprintln!("技能字串 {} 筆", strings.len());

    // 2. Character/00002000 動作 → 總延遲（|delay| 加總）
    let char_tree = svc.tree_for(CHARACTER)?;
    let mut action_delays: BTreeMap<String, i64> = BTreeMap::new();
    for action in &char_tree.children {
        let mut total: i64 = 0;
        let mut frames = 0usize;
        for frame in &action.children {
            if let Some(d) = frame.find("delay").and_then(|n| as_i64(&n.value)) {
                total += d.abs();
                frames += 1;
            }
        }
        if frames > 0 {
            action_delays.insert(action.name.clone(), total);
        }
    }
    eprintln!("角色動作 {} 個", action_delays.len());

    // 3. 走訪所有 Skill/* img
    let mut skills: Vec<serde_json::Value> = Vec::new();
    let mut missing_action: Vec<(String, String)> = Vec::new(); // (skill id, action)
    let mut skipped_img: Vec<String> = Vec::new();
    let skill_keys: Vec<String> = svc
        .catalog
        .entries
        .keys()
        .filter(|k| k.starts_with("Skill/"))
        .cloned()
        .collect();
    for key in &skill_keys {
        let tree = match svc.tree_for(key) {
            Ok(t) => t,
            Err(_) => {
                skipped_img.push(key.clone());
                continue;
            }
        };
        let Some(skill_root) = tree.find("skill") else {
            skipped_img.push(key.clone());
            continue;
        };
        let job = key.trim_start_matches("Skill/");
        if !JOB_WHITELIST.contains(&job) {
            continue;
        }
        for skill in &skill_root.children {
            let sid = &skill.name;

            // action 節點：子節點（0/1/...）各指一個動作名；或直接是字串值
            let mut actions: Vec<String> = Vec::new();
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

            let mut per_action = Map::new();
            for name in &actions {
                match action_delays.get(name) {
                    Some(d) => {
                        per_action.insert(name.clone(), json!(d));
                    }
                    None => missing_action.push((sid.clone(), name.clone())),
                }
            }

            let levels = skill
                .find("level")
                .map(node_to_json)
                .unwrap_or(serde_json::Value::Null);

            // Skill img 的 id 有補零（如 0001000），String/TW/Skill 的鍵沒有 → 兩種都試
            let strinfo = strings
                .get(sid)
                .or_else(|| strings.get(sid.trim_start_matches('0')));
            let mut entry = Map::new();
            entry.insert("id".into(), json!(sid));
            entry.insert("job".into(), json!(job));
            entry.insert(
                "action".into(),
                actions
                    .first()
                    .map(|a| json!(a))
                    .unwrap_or(serde_json::Value::Null),
            );
            entry.insert("actionDelays".into(), serde_json::Value::Object(per_action));
            entry.insert(
                "name".into(),
                strinfo
                    .and_then(|m| m.get("Name").cloned())
                    .unwrap_or(serde_json::Value::Null),
            );
            entry.insert(
                "desc".into(),
                strinfo
                    .and_then(|m| m.get("Desc").cloned())
                    .unwrap_or(serde_json::Value::Null),
            );
            entry.insert(
                "h".into(),
                strinfo
                    .and_then(|m| m.get("H").cloned())
                    .unwrap_or(serde_json::Value::Null),
            );
            entry.insert(
                "levelDesc".into(),
                strinfo
                    .and_then(|m| m.get("LevelDesc").cloned())
                    .unwrap_or(serde_json::Value::Null),
            );
            entry.insert("levels".into(), levels);
            skills.push(serde_json::Value::Object(entry));
        }
    }

    skills.sort_by(|a, b| {
        (
            a["job"].as_str().unwrap_or(""),
            a["id"].as_str().unwrap_or(""),
        )
            .cmp(&(
                b["job"].as_str().unwrap_or(""),
                b["id"].as_str().unwrap_or(""),
            ))
    });

    let output = json!({
        "source": DATA_DIR,
        "skillCount": skills.len(),
        "skills": skills,
    });
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&out, serde_json::to_string_pretty(&output)?)?;

    eprintln!("技能 {} 筆 → {}", skills.len(), out.display());
    if !skipped_img.is_empty() {
        eprintln!("跳過 img {} 個: {:?}", skipped_img.len(), skipped_img);
    }
    if !missing_action.is_empty() {
        eprintln!(
            "查無動作的技能 {} 筆（前 10）: {:?}",
            missing_action.len(),
            &missing_action[..missing_action.len().min(10)]
        );
    }
    Ok(())
}

/// 讀取 String/TW/Skill 的原始 bytes 並解析成 JSON（id → {Name, Desc, H, LevelDesc}）。
/// DataService.get_node 會做 UTF-8 lossy 轉換，這裡直接拿原始 bytes 保留完整內容。
fn load_skill_strings(
    svc: &DataService,
) -> Result<BTreeMap<String, Map<String, serde_json::Value>>, Box<dyn std::error::Error>> {
    let loc = svc
        .catalog
        .entries
        .get(STRING_SKILL)
        .ok_or("索引查無 String/TW/Skill")?
        .clone();
    let meta = &svc.catalog.bundles[loc.bundle_index as usize];
    let bundle = Bundle::open(&svc.catalog.root.join(format!("{}.bundle", meta.name)))?;
    let data = bundle.read_node_range(
        loc.node_index as usize,
        loc.abs_offset,
        loc.byte_size as usize,
    )?;
    let text_asset = objects::parse_text_asset(&data, true)?;
    let parsed: serde_json::Value = serde_json::from_slice(&text_asset.script)?;
    let mut out = BTreeMap::new();
    if let serde_json::Value::Object(map) = parsed {
        for (k, v) in map {
            if let serde_json::Value::Object(obj) = v {
                out.insert(k, obj);
            }
        }
    }
    Ok(out)
}

fn as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::U8(n) => Some(i64::from(*n)),
        Value::U16(n) => Some(i64::from(*n)),
        Value::Int(n) => Some(i64::from(*n)),
        Value::Double(f) => Some(*f as i64),
        _ => None,
    }
}

fn as_str(v: &Value) -> Option<&str> {
    match v {
        Value::Str(s) | Value::Uol(s) => Some(s),
        _ => None,
    }
}

/// 節點子樹 → 巢狀 JSON（純資料；canvas 葉略過不輸出）。
fn node_to_json(node: &WzNode) -> serde_json::Value {
    if node.children.is_empty() {
        return leaf_json(&node.value);
    }
    let mut obj = Map::new();
    for child in &node.children {
        obj.insert(child.name.clone(), node_to_json(child));
    }
    serde_json::Value::Object(obj)
}

fn leaf_json(v: &Value) -> serde_json::Value {
    match v {
        Value::SubTree => serde_json::Value::Null,
        Value::Canvas => serde_json::Value::Null,
        Value::U8(n) => json!(n),
        Value::U16(n) => json!(n),
        Value::Int(n) => json!(n),
        Value::Double(f) => json!(f),
        Value::Float2(x, y) | Value::Vector(x, y) => json!([x, y]),
        Value::Int2(a, b) => json!([a, b]),
        Value::Str(s) | Value::Uol(s) => json!(s),
    }
}
