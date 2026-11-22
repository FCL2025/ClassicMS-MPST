//! 寵物紙娃娃核心：寵物/寵物裝備清單，以及幀圖層合成。
//!
//! 寵物資料比角色簡單很多：
//! - 本體在 `Item/Pet/<id>`（7 碼 id，如 5000000），頂層動作節點的幀
//!   直接就是 canvas（帶 origin/delay，無 map 錨點），頂層另有
//!   info/interact/food/slang 等非動作節點。
//! - 寵物裝備在 `Character/PetEquip/<id>`（8 碼 id），第二層是支援的
//!   寵物 id，再往下才是動作幀；幀只有 origin（delay 常缺席，跟隨本體）。
//! - 對齊方式 = 原點對齊：寵物各幀 origin 落在同一固定點（同角色
//!   紙娃娃的第 0 幀基準做法），寵裝 origin 與寵物幀 origin 重合。

use serde::Serialize;

use crate::avatar::{
    find_action, find_action_at, frame_delay, frame_list, is_cash, item_name, looks_like_action,
    resolve_frame, vector_prop, wrap_index, AvatarFrames, Frame, Layer,
};
use crate::catalog::{CatalogError, ChildEntry, DataService, Result};

/// 寵物本體圖層 z（只有本體+寵裝兩層，值只需相對大小）。
const PET_BODY_Z: i32 = 0;
/// 寵物裝備畫在本體之上（z 遞減排序，小者後畫 = 上層）。
const PET_EQUIP_Z: i32 = -1;

/// 寵物 img 頂層的非動作節點（掃動作清單時排除）。
const NON_ACTION_NODES: &[&str] = &["info", "interact", "food", "slang"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetInfo {
    /// img 名（7 碼，如 "5000000"）。
    pub id: String,
    pub name: String,
    /// icon 節點路徑（前端取圖失敗時自行 fallback iconRaw）。
    pub icon_path: String,
    /// 該寵物支援的動作清單（含表情類動作，分類由前端處理）。
    pub actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PetEquipInfo {
    /// img 名（8 碼，如 "01802000"）。
    pub id: String,
    pub name: String,
    pub icon_path: String,
    /// 支援的寵物 id（img 內的數位子節點）。
    pub pet_ids: Vec<u32>,
    /// 點裝：img 的 `info/cash` 數值非零。
    pub cash: bool,
}

/// 寵物清單：`Item/Pet` 目錄掃描 + `String/TW/Pet` 名稱表。
pub fn pet_list(svc: &DataService) -> Result<Vec<PetInfo>> {
    let names = crate::commodity::load_names(svc, &svc.string_path("Pet"));
    let mut pets = Vec::new();
    for child in svc.list_children("Item/Pet")? {
        let ChildEntry::Img { name: id, .. } = child else {
            continue;
        };
        let img_key = format!("Item/Pet/{id}");
        let Ok(tree) = svc.tree_for(&img_key) else {
            continue;
        };
        let actions = tree
            .children
            .iter()
            .filter(|c| !NON_ACTION_NODES.contains(&c.name.as_str()) && looks_like_action(c))
            .map(|c| c.name.clone())
            .collect();
        let name = id
            .parse::<u32>()
            .ok()
            .and_then(|n| names.get(&n).cloned())
            .unwrap_or_else(|| id.clone());
        pets.push(PetInfo {
            icon_path: format!("{img_key}/info/icon"),
            id,
            name,
            actions,
        });
    }
    pets.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(pets)
}

/// 寵物裝備清單：`Character/PetEquip` 目錄掃描 + `String/TW/Item` 名稱表。
pub fn pet_equip_list(svc: &DataService) -> Result<Vec<PetEquipInfo>> {
    let names = svc.item_names()?;
    let mut items = Vec::new();
    for child in svc.list_children("Character/PetEquip")? {
        let ChildEntry::Img { name: id, .. } = child else {
            continue;
        };
        let img_key = format!("Character/PetEquip/{id}");
        let Ok(tree) = svc.tree_for(&img_key) else {
            continue;
        };
        let pet_ids: Vec<u32> = tree
            .children
            .iter()
            .filter_map(|c| c.name.parse::<u32>().ok())
            .collect();
        items.push(PetEquipInfo {
            icon_path: format!("{img_key}/info/icon"),
            name: item_name(&names, &id),
            id,
            pet_ids,
            cash: is_cash(&tree),
        });
    }
    items.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(items)
}

/// 合成寵物一個動作的所有幀圖層。
///
/// 幀間定位同角色紙娃娃：以第 0 幀 origin 為固定基準點，第 i 幀
/// pos = origin_0 - origin_i，播放時不會亂跳。寵裝幀 index 對齊本體
/// （modulo 本體幀數），pos = origin_0 - 寵裝 origin（原點重合）。
///
/// `face_frames`/`effect_frames` 恆空（寵物無表情分離與 ItemEff）。
pub fn pet_frames(
    svc: &DataService,
    pet_id: &str,
    equip_id: Option<&str>,
    action: &str,
) -> Result<AvatarFrames> {
    let img_key = format!("Item/Pet/{pet_id}");
    let tree = svc.tree_for(&img_key)?;
    // fallback 鏈：指定動作 → stand0 → 第一個像動作的子節點（find_action 內建）
    let Some(action_rel) = find_action(&tree, &[action, "stand0"]) else {
        return Err(CatalogError::PathNotFound(format!(
            "寵物 {pet_id} 無可用動作 {action}"
        )));
    };
    let body_frames: Vec<String> = frame_list(&tree, &action_rel)
        .iter()
        .filter_map(|rel| resolve_frame(&tree, rel))
        .collect();
    if body_frames.is_empty() {
        return Err(CatalogError::PathNotFound(format!(
            "寵物 {pet_id} 動作 {action} 無幀"
        )));
    }

    // 每幀的（delay, origin）；寵物幀節點本身就是 canvas
    let body_meta: Vec<(i32, (f32, f32))> = body_frames
        .iter()
        .map(|rel| {
            let node = tree.find(rel);
            let delay = node
                .map(frame_delay)
                .unwrap_or(crate::avatar::DEFAULT_DELAY);
            let origin = node
                .and_then(|n| vector_prop(n, "origin"))
                .unwrap_or((0.0, 0.0));
            (delay, origin)
        })
        .collect();
    let origin_0 = body_meta.first().map(|(_, o)| *o).unwrap_or((0.0, 0.0));

    // 寵物裝備：<equip>/<pet_id>/<action>，幀 index 對齊本體（modulo）
    let equip = equip_id
        .and_then(|eid| resolve_pet_equip(svc, eid, pet_id, action))
        .and_then(|(key, frames)| svc.tree_for(&key).ok().map(|tree| (key, tree, frames)));

    let frames: Vec<Frame> = body_frames
        .iter()
        .enumerate()
        .map(|(i, rel)| {
            let (delay, origin) = body_meta[i];
            let base = origin_align_pos(origin_0, origin);
            let mut layers = vec![Layer {
                path: format!("{img_key}/{rel}"),
                x: base.0,
                y: base.1,
                z: PET_BODY_Z,
            }];
            if let Some((equip_key, equip_tree, equip_frames)) = &equip {
                let erel = &equip_frames[wrap_index(i, equip_frames.len())];
                if let Some(node) = equip_tree.find(erel) {
                    let eorigin = vector_prop(node, "origin").unwrap_or((0.0, 0.0));
                    let epos = origin_align_pos(origin_0, eorigin);
                    layers.push(Layer {
                        path: format!("{equip_key}/{erel}"),
                        x: epos.0,
                        y: epos.1,
                        z: PET_EQUIP_Z,
                    });
                }
            }
            layers.sort_by_key(|l| std::cmp::Reverse(l.z));
            Frame {
                delay,
                layers,
                face_anchor: None,
                show_face: false,
            }
        })
        .collect();

    Ok(AvatarFrames {
        frames,
        face_frames: Vec::new(),
        effect_frames: Vec::new(),
    })
}

/// 解析寵物裝備幀列表：`<equip>/<pet id>/<action>`，動作 fallback 同本體。
/// 回傳（equip img 路徑, 解析後幀相對路徑列表）；裝備不支援該寵物或
/// 無可用動作時回 None（當沒穿，不視為錯誤）。
fn resolve_pet_equip(
    svc: &DataService,
    equip_id: &str,
    pet_id: &str,
    action: &str,
) -> Option<(String, Vec<String>)> {
    let img_key = format!("Character/PetEquip/{equip_id}");
    let tree = svc.tree_for(&img_key).ok()?;
    // 第二層 key 是寵物 id（資料為 7 碼無前導零格式，如 "5000000"）
    let pet_key = pet_id.trim_start_matches('0');
    let pet_node = tree.find(pet_key)?;
    let action_rel = find_action_at(pet_node, pet_key, &[action, "stand0"])?;
    let frames: Vec<String> = frame_list(&tree, &action_rel)
        .iter()
        .filter_map(|rel| resolve_frame(&tree, rel))
        .collect();
    if frames.is_empty() {
        return None;
    }
    Some((img_key, frames))
}

/// 原點對齊：pos = base - origin（canvas 左上角，讓 origin 落在 base）。
fn origin_align_pos(base: (f32, f32), origin: (f32, f32)) -> (f32, f32) {
    (base.0 - origin.0, base.1 - origin.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wzjs::{Value, WzNode};

    fn leaf(name: &str, value: Value) -> WzNode {
        WzNode {
            name: name.into(),
            value,
            path: None,
            children: vec![],
            type_tag: 0,
        }
    }

    /// 造一個動作節點：幀 = 編號 canvas（origin + delay）。
    fn action_node(name: &str, frames: &[(f32, f32)]) -> WzNode {
        WzNode {
            name: name.into(),
            value: Value::SubTree,
            path: None,
            children: frames
                .iter()
                .enumerate()
                .map(|(i, &(ox, oy))| WzNode {
                    name: i.to_string(),
                    value: Value::Canvas,
                    path: None,
                    children: vec![
                        leaf("origin", Value::Vector(ox, oy)),
                        leaf("delay", Value::Int(120)),
                    ],
                    type_tag: 18,
                })
                .collect(),
            type_tag: 2,
        }
    }

    #[test]
    fn origin_align_math() {
        // 第 0 幀 origin = (19, 39) 為基準；第 i 幀 pos = origin_0 - origin_i
        assert_eq!(origin_align_pos((19.0, 39.0), (19.0, 39.0)), (0.0, 0.0));
        assert_eq!(origin_align_pos((19.0, 39.0), (18.0, 37.0)), (1.0, 2.0));
    }

    #[test]
    fn pet_action_fallback() {
        // 無 jump 動作 → 退回 stand0；都沒有 → 第一個像動作的節點
        let tree = WzNode {
            name: "img".into(),
            value: Value::SubTree,
            path: None,
            children: vec![
                leaf("info", Value::SubTree),
                action_node("stand0", &[(19.0, 39.0)]),
                action_node("move", &[(19.0, 39.0)]),
            ],
            type_tag: 2,
        };
        assert_eq!(
            find_action(&tree, &["jump", "stand0"]).as_deref(),
            Some("stand0")
        );
        assert_eq!(
            find_action(&tree, &["move", "stand0"]).as_deref(),
            Some("move")
        );
        // info 不是動作，不會被當 fallback
        let bare = WzNode {
            children: vec![leaf("info", Value::SubTree)],
            ..tree
        };
        assert_eq!(find_action(&bare, &["jump", "stand0"]), None);
    }
}
