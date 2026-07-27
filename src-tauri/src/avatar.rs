//! 紙娃娃核心：動作/膚色/表情/裝備清單，以及多部位幀圖層合成。
//!
//! 演算法概念移植自 WzComparerR2-Plus 的 Avatar 外掛（AvatarCanvas.cs），
//! 但骨骼對齊改用「單幀靜態等價」的簡化版：維護一張全域錨點絕對位置表，
//! 每張 canvas 用 `map` 中第一個已知錨點對齊（pos = anchor_abs - origin - map），
//! 放置後把自己的錨點登錄進表（先放者為準，不覆蓋同名）。
//!
//! 圖層順序（z 字串 → zIndex）對齊 WzComparerR2 的 zmap 規則：
//! 已知圖層名 = 在 [`ZMAP`] 的位置（index 0 最上層），未知名排最底，
//! 整數 z 則 n<0 時 `len-n`、n≥0 時 `-(1+n)`（最上）。繪製順序 = z 遞減。

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::catalog::{CatalogError, ChildEntry, DataService, Result};
use crate::wzjs::{Value, WzNode};

/// zmap 圖層順序表（對齊 WzComparerR2 的 zmap，共 149 層）。
/// 第 1 行 = 最上層（index 0），越後面越底層。
const ZMAP: &[&str] = &[
    "mobEquipFront",
    "tamingMobFront",
    "mobEquipMid",
    "saddleFront",
    "mobEquipUnderSaddle",
    "tamingMobMid",
    "characterStart",
    "emotionOverBody",
    "weaponWristOverGlove",
    "capeOverHead",
    "weaponOverGlove",
    "gloveWristOverHair",
    "gloveOverHair",
    "handOverHair",
    "weaponOverHand",
    "shieldOverHair",
    "gloveWristBelowWeapon",
    "gloveBelowWeapon",
    "handBelowWeapon",
    "weaponOverArm",
    "gloveWristBelowMailArm",
    "mailArmOverHair",
    "gloveBelowMailArm",
    "armOverHair",
    "mailArmOverHairBelowWeapon",
    "armOverHairBelowWeapon",
    "weaponBelowArm",
    "capOverHair",
    "hairOverHead",
    "accessoryEyeOverCap",
    "capAccessory",
    "cap",
    "hair",
    "accessoryEye",
    "accessoryEyeShadow",
    "accessoryFace",
    "capAccessoryBelowAccFace",
    "accessoryEar",
    "capBelowAccessory",
    "accessoryFaceOverFaceBelowCap",
    "face",
    "accessoryFaceBelowFace",
    "hairShade",
    "head",
    "cape",
    "gloveWrist",
    "mailArm",
    "glove",
    "hand",
    "arm",
    "weapon",
    "shield",
    "weaponOverArmBelowHead",
    "gloveWristBelowHead",
    "mailArmBelowHeadOverMailChest",
    "gloveBelowHead",
    "armBelowHeadOverMailChest",
    "mailArmBelowHead",
    "armBelowHead",
    "weaponOverBody",
    "mailChestTop",
    "gloveWristOverBody",
    "mailChestOverHighest",
    "pantsOverMailChest",
    "mailChest",
    "shoesTop",
    "pantsOverShoesBelowMailChest",
    "shoesOverPants",
    "mailChestOverPants",
    "pants",
    "shoes",
    "pantsBelowShoes",
    "mailChestBelowPants",
    "gloveOverBody",
    "body",
    "gloveWristBelowBody",
    "gloveBelowBody",
    "capAccessoryBelowBody",
    "shieldBelowBody",
    "capeBelowBody",
    "hairBelowBody",
    "weaponBelowBody",
    "backHairOverCape",
    "backWing",
    "backWeaponOverShield",
    "backShield",
    "backCapOverHair",
    "backHair",
    "backCap",
    "backWeaponOverHead",
    "backHairBelowCapWide",
    "backHairBelowCapNarrow",
    "backHairBelowCap",
    "backCape",
    "backAccessoryFaceOverHead",
    "backHead",
    "backMailChestOverPants",
    "backPantsOverMailChest",
    "backMailChest",
    "backPantsOverShoesBelowMailChest",
    "backShoes",
    "backPants",
    "backShoesBelowPants",
    "backPantsBelowShoes",
    "backMailChestBelowPants",
    "backWeaponOverGlove",
    "backGloveWrist",
    "backGlove",
    "backBody",
    "backAccessoryEar",
    "backAccessoryFace",
    "backCapAccessory",
    "backMailChestAccessory",
    "backShieldBelowBody",
    "backHairBelowHead",
    "backWeapon",
    "characterEnd",
    "saddleRear",
    "tamingMobRear",
    "mobEquipRear",
    "backMobEquipFront",
    "backTamingMobFront",
    "backMobEquipMid",
    "backSaddle",
    "backMobEquipUnderSaddle",
    "backTamingMobMid",
    "Sd",
    "Tm",
    "Sr",
    "Wg",
    "Ma",
    "Ws",
    "Pn",
    "So",
    "Si",
    "Wp",
    "Gv",
    "Ri",
    "Cp",
    "Ay",
    "As",
    "Ae",
    "Am",
    "Af",
    "At",
    "Fc",
    "Hr",
    "Hd",
    "Bd",
];

/// 幀 delay 的預設值（毫秒）。
const DEFAULT_DELAY: i32 = 100;
/// UOL / action+frame 連鎖解析的上限（防循環）。
const RESOLVE_LIMIT: usize = 8;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinInfo {
    pub body_id: String,
    pub head_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarMeta {
    pub actions: Vec<String>,
    pub skins: Vec<SkinInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EquipItem {
    pub id: String,
    pub name: String,
    /// 部位 slot："hair" "face" "cap" "coat" "longcoat" "pants" "shoes"
    /// "glove" "shield" "cape" "weapon" "earrings" "faceAcc" "eyeAcc"。
    pub slot: String,
    /// icon 節點路徑（前端取圖失敗時自行 fallback iconRaw）。
    pub icon_path: String,
    /// 點裝：img 的 `info/cash` 數值非零。
    pub cash: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartInput {
    /// "body" "head" 必有，其餘為裝備 slot。
    pub slot: String,
    pub id: String,
    pub visible: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    /// 完整節點路徑（如 `Character/00002000/stand1/0/body`），前端用 get_image 取圖。
    pub path: String,
    /// canvas 左上角。
    pub x: f32,
    pub y: f32,
    pub z: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub delay: i32,
    /// 已按繪製順序排好（z 遞減，大者先畫 = 底層）。
    pub layers: Vec<Layer>,
    /// 身體幀專用：brow 錨點的絕對位置（來自 head canvas 的 map）。
    /// 前端畫臉：`draw = faceAnchor + faceFrames[j].layers[k].(x, y)`，
    /// 且只在 `showFace && faceAnchor != null` 時畫。
    /// 該幀沒有 brow 時為 None（face_frames 的幀此欄位恆 None）。
    pub face_anchor: Option<[f32; 2]>,
    /// 身體幀專用：幀屬性 face != 0（背面動作不畫臉；face_frames 的幀此欄位恆 true）。
    pub show_face: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarFrames {
    /// 身體動畫幀（主幀數 = body 動作幀數；不含臉）。
    pub frames: Vec<Frame>,
    /// 表情動畫幀（幀數獨立，前端各自計時疊加）。
    pub face_frames: Vec<Frame>,
}

/// 紙娃娃基本資料：body 動作清單 + 膚色清單。
pub fn avatar_meta(svc: &DataService) -> Result<AvatarMeta> {
    let body = svc.tree_for("Character/00002000")?;
    let actions = body
        .children
        .iter()
        .filter(|c| c.name != "info" && looks_like_action(c))
        .map(|c| c.name.clone())
        .collect();

    let names = svc.item_names()?;
    let mut skins = Vec::new();
    for child in svc.list_children("Character")? {
        let ChildEntry::Img { name, .. } = child else {
            continue;
        };
        // 膚色 body img：8 碼數字、範圍 00002000–00002999
        if name.len() != 8 || !name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(num) = name.parse::<u32>() else {
            continue;
        };
        if !(2000..=2999).contains(&num) {
            continue;
        }
        let head_id = head_id_of(&name);
        // 名稱表以 head id（去前導零）為 key，例如 "12000" = 奶油皮膚
        let skin_name = head_id
            .parse::<u32>()
            .ok()
            .and_then(|n| names.get(&n))
            .map(|(n, _)| n.clone())
            .unwrap_or_else(|| name.clone());
        skins.push(SkinInfo {
            body_id: name,
            head_id,
            name: skin_name,
        });
    }
    Ok(AvatarMeta { actions, skins })
}

/// 指定臉型的表情清單（face img 的非 info 子節點名）。
pub fn avatar_emotions(svc: &DataService, face_id: &str) -> Result<Vec<String>> {
    let tree = svc.tree_for(&format!("Character/Face/{face_id}"))?;
    Ok(tree
        .children
        .iter()
        .filter(|c| c.name != "info")
        .map(|c| c.name.clone())
        .collect())
}

/// 全裝備清單。id/名稱/icon 走 catalog 索引 + String 名稱表；
/// cash 旗標需要讀各 img 的 `info/cash`，會逐一解碼 WZJS 樹
/// （tree_for 有 LRU 快取，同程序內第二次呼叫只補 miss 的部分）。
pub fn avatar_equipment(svc: &DataService) -> Result<Vec<EquipItem>> {
    let names = svc.item_names()?;
    let mut items = Vec::new();
    // （目錄, 固定 slot）；None = 依 id 前綴判斷
    const DIRS: &[(&str, Option<&str>)] = &[
        ("Cap", None),
        ("Coat", None),
        ("Longcoat", None),
        ("Pants", None),
        ("Shoes", None),
        ("Glove", None),
        ("Shield", None),
        ("Cape", None),
        ("Weapon", None),
        ("Hair", Some("hair")),
        ("Face", Some("face")),
    ];
    let mut push_dir = |dir: &str, fixed_slot: Option<&str>, prefix_filter: Option<&[&str]>| {
        let Ok(children) = svc.list_children(&format!("Character/{dir}")) else {
            return;
        };
        for child in children {
            let ChildEntry::Img { name: id, .. } = child else {
                continue;
            };
            if let Some(prefixes) = prefix_filter {
                if !prefixes.iter().any(|p| id.starts_with(p)) {
                    continue;
                }
            }
            let Some(slot) = fixed_slot.or_else(|| slot_of(&id)) else {
                continue;
            };
            let img_key = format!("Character/{dir}/{id}");
            let cash = svc.tree_for(&img_key).map(|t| is_cash(&t)).unwrap_or(false);
            let name = item_name(&names, &id);
            let icon_path = match slot {
                "hair" => format!("Character/Hair/{id}/default/hairOverHead"),
                "face" => format!("Character/Face/{id}/default/face"),
                _ => format!("{img_key}/info/icon"),
            };
            items.push(EquipItem {
                id,
                name,
                slot: slot.to_string(),
                icon_path,
                cash,
            });
        }
    };
    for &(dir, fixed_slot) in DIRS {
        push_dir(dir, fixed_slot, None);
    }
    // 飾品目錄：只收臉飾 0101 / 眼飾 0102 / 耳環 0103（0112–0114 等排除）
    push_dir("Accessory", None, Some(&["0101", "0102", "0103"]));
    Ok(items)
}

/// 合成一個動作的所有幀圖層。
///
/// 對齊順序：body → head → face → hair → cap → 其餘固定順序。
/// 主幀數 = body 動作幀數，其他部位幀索引用 modulo 循環。
///
/// 幀間定位（對齊楓之谷 client 的做法）：以第 0 幀 body canvas 的 origin
/// 為整個動作的固定基準點，第 i 幀的 body canvas 放在 `origin_0 - origin_i + move`，
/// 其餘部位照舊用錨點對齊，整個幀剛性平移到位，角色播放時不會亂跳。
///
/// 臉（face）獨立輸出到 `face_frames`（幀數 = 表情幀數），layers 是
/// **相對於 brow 錨點的相對座標**；前端以身體幀的 `face_anchor` + 表情幀
/// 的相對座標在繪製當下配對，臉就會跟著頭動。
///
/// `hair_cover` = UI 的「帽子遮髮」核取方塊（對齊 WzComparerR2 的 HairCover）：
/// 勾選且有戴帽 → 藏 hairOverHead 等蓋頭髮層；不勾 → 髮型超出帽子
/// （藏 hair/backHairBelowCap 等帽下髮層）。沒戴帽時此旗標無效果。
pub fn avatar_frames(
    svc: &DataService,
    parts: Vec<PartInput>,
    action: &str,
    emotion: &str,
    hair_cover: bool,
) -> Result<AvatarFrames> {
    const REST_ORDER: &[&str] = &[
        "coat", "longcoat", "pants", "shoes", "glove", "cape", "shield", "weapon", "earrings",
        "faceAcc", "eyeAcc",
    ];
    let mut ordered: Vec<&PartInput> = Vec::new();
    for slot in ["body", "head", "face", "hair", "cap"]
        .iter()
        .chain(REST_ORDER)
    {
        if let Some(p) = parts.iter().find(|p| p.slot == *slot) {
            ordered.push(p);
        }
    }
    // 有效遮髮 = 有戴帽 + 使用者勾選遮髮（HairCover 規則見 hair_canvas_hidden）。
    let cap_visible = parts.iter().any(|p| p.slot == "cap" && p.visible);
    let hair_cover = cap_visible && hair_cover;

    let mut resolved: Vec<ResolvedPart> = Vec::new();
    for part in ordered {
        if !part.visible {
            continue;
        }
        match resolve_part(svc, part, action, emotion)? {
            Some(rp) => resolved.push(rp),
            // body/head 是紙娃娃本體，缺了直接算錯；其餘部位缺資料就略過
            None if part.slot == "body" || part.slot == "head" => {
                return Err(CatalogError::PathNotFound(format!(
                    "{} {} 無可用動作 {action}",
                    part.slot, part.id
                )));
            }
            None => {}
        }
    }
    let Some(body_pos) = resolved.iter().position(|p| p.slot == "body") else {
        return Err(CatalogError::PathNotFound("缺少 body 部位".into()));
    };
    let body = &resolved[body_pos];
    let body_frame_count = body.frames.len();
    let body_delays: Vec<i32> = body
        .frames
        .iter()
        .map(|rel| {
            body.tree
                .find(rel)
                .map(frame_delay)
                .unwrap_or(DEFAULT_DELAY)
        })
        .collect();

    // 幀間基準點：以第 0 幀 body canvas 的 origin 為整個動作的固定基準；
    // 每幀 shift = origin_0 - origin_i + move（無 body canvas 的幀不位移）。
    let body_origins: Vec<Option<(f32, f32)>> = body
        .frames
        .iter()
        .map(|rel| {
            let frame_node = body.tree.find(rel)?;
            collect_canvases(&body.tree, rel, frame_node)
                .into_iter()
                .find(|(node, _)| node.name == "body")
                .and_then(|(node, _)| vector_prop(node, "origin"))
        })
        .collect();
    let origin_0 = body_origins
        .iter()
        .flatten()
        .next()
        .copied()
        .unwrap_or((0.0, 0.0));
    let body_shifts: Vec<(f32, f32)> = body
        .frames
        .iter()
        .enumerate()
        .map(|(i, rel)| {
            let move_vec = body.tree.find(rel).and_then(|n| vector_prop(n, "move"));
            frame_shift(origin_0, body_origins[i], move_vec)
        })
        .collect();

    let mut frames: Vec<Frame> = Vec::with_capacity(body_frame_count);
    let mut anchor_tables: Vec<HashMap<String, (f32, f32)>> = Vec::with_capacity(body_frame_count);

    for (i, &body_delay) in body_delays.iter().enumerate() {
        // 全域錨點絕對位置表（每幀重算：單幀靜態等價）
        let mut anchors: HashMap<String, (f32, f32)> = HashMap::new();
        let mut layers: Vec<Layer> = Vec::new();
        let mut show_face = true;
        for part in &resolved {
            // 臉不參與主迴圈：改用 face_anchor/show_face 讓前端繪製當下配對
            if part.slot == "face" {
                continue;
            }
            let rel = &part.frames[wrap_index(i, part.frames.len())];
            let Some(frame_node) = part.tree.find(rel) else {
                continue;
            };
            if part.slot == "body" {
                show_face = frame_face_visible(frame_node);
            }
            let mut canvases = collect_canvases(&part.tree, rel, frame_node);
            if part.slot == "body" {
                // body canvas 必須最先放（它的錨點是全角色的對齊基準）
                canvases.sort_by_key(|(node, _)| u8::from(node.name != "body"));
            }
            // 帽子遮髮規則。不遮髮時若整個髮型會被藏光就不藏：有些髮型
            // （如 00030160）只有一張 z="hair" 的本體貼圖、沒有 hairOverHead
            // 變體，藏了會變光頭。
            let hidden: Vec<bool> = canvases
                .iter()
                .map(|(node, _)| part.slot == "hair" && hair_canvas_hidden(z_str(node), hair_cover))
                .collect();
            let keep_all = !hair_cover && !hidden.is_empty() && hidden.iter().all(|&hide| hide);
            for ((node, crel), hide) in canvases.into_iter().zip(hidden) {
                if hide && !keep_all {
                    continue;
                }
                let root_pos =
                    (part.slot == "body" && node.name == "body").then_some(body_shifts[i]);
                let pos = place_canvas(root_pos, node, &mut anchors);
                layers.push(Layer {
                    path: format!("{}/{crel}", part.img_key),
                    x: pos.0,
                    y: pos.1,
                    z: z_index(node),
                });
            }
        }
        layers.sort_by_key(|l| std::cmp::Reverse(l.z));
        // brow 錨點（head canvas 的 map 登錄）= 前端畫臉的基準點
        let face_anchor = anchors.get("brow").map(|&(x, y)| [x, y]);
        frames.push(Frame {
            delay: body_delay,
            layers,
            face_anchor,
            show_face,
        });
        anchor_tables.push(anchors);
    }

    // 表情幀：幀數獨立；layers 是相對於 brow 錨點的相對座標，
    // 配對（身體幀 i × 表情幀 j）延到前端繪製當下。
    let mut face_frames = Vec::new();
    if let Some(face) = resolved.iter().find(|p| p.slot == "face") {
        for (j, rel) in face.frames.iter().enumerate() {
            let delay = face
                .tree
                .find(rel)
                .map(frame_delay)
                .unwrap_or(DEFAULT_DELAY);
            let mut layers = Vec::new();
            if let Some(frame_node) = face.tree.find(rel) {
                for (node, crel) in collect_canvases(&face.tree, rel, frame_node) {
                    let pos = match face_relative_pos(node) {
                        Some(rel_pos) => rel_pos,
                        // fallback：face canvas 的 map 沒有 brow 時，
                        // 退回舊做法（body 幀 j % 主幀數 的錨點表絕對位置）
                        None => {
                            align_to_anchors(node, &anchor_tables[wrap_index(j, body_frame_count)])
                        }
                    };
                    layers.push(Layer {
                        path: format!("{}/{crel}", face.img_key),
                        x: pos.0,
                        y: pos.1,
                        z: z_index(node),
                    });
                }
                layers.sort_by_key(|l| std::cmp::Reverse(l.z));
            }
            face_frames.push(Frame {
                delay,
                layers,
                face_anchor: None,
                show_face: true,
            });
        }
    }

    Ok(AvatarFrames {
        frames,
        face_frames,
    })
}

// ── 部位解析 ────────────────────────────────────────────────────

/// 一個部位在某動作下解析出的幀列表（路徑皆為 UOL/action+frame 解析後的目標）。
struct ResolvedPart {
    slot: String,
    img_key: String,
    tree: Arc<WzNode>,
    /// 每幀在 img 內的相對路徑（已解析）。
    frames: Vec<String>,
}

fn resolve_part(
    svc: &DataService,
    part: &PartInput,
    action: &str,
    emotion: &str,
) -> Result<Option<ResolvedPart>> {
    let Some(img_key) = img_key_of(&part.slot, &part.id) else {
        return Ok(None);
    };
    let Ok(tree) = svc.tree_for(&img_key) else {
        return Ok(None);
    };
    // fallback 鏈：指定動作 → stand1 → default → 第一個像動作的子節點；
    // 臉用 表情 → default（表情集沒有 stand1）
    let chain: [&str; 3] = if part.slot == "face" {
        [emotion, "default", ""]
    } else {
        [action, "stand1", "default"]
    };
    let chain: Vec<&str> = chain.into_iter().filter(|s| !s.is_empty()).collect();
    let Some(action_rel) = find_action(&tree, &chain) else {
        return Ok(None);
    };
    let frames: Vec<String> = frame_list(&tree, &action_rel)
        .iter()
        .filter_map(|rel| resolve_frame(&tree, rel))
        .collect();
    if frames.is_empty() {
        return Ok(None);
    }
    Ok(Some(ResolvedPart {
        slot: part.slot.clone(),
        img_key,
        tree,
        frames,
    }))
}

/// slot + id → img 路徑。
fn img_key_of(slot: &str, id: &str) -> Option<String> {
    let key = match slot {
        "body" | "head" => format!("Character/{id}"),
        "hair" => format!("Character/Hair/{id}"),
        "face" => format!("Character/Face/{id}"),
        "cap" => format!("Character/Cap/{id}"),
        "coat" => format!("Character/Coat/{id}"),
        "longcoat" => format!("Character/Longcoat/{id}"),
        "pants" => format!("Character/Pants/{id}"),
        "shoes" => format!("Character/Shoes/{id}"),
        "glove" => format!("Character/Glove/{id}"),
        "shield" => format!("Character/Shield/{id}"),
        "cape" => format!("Character/Cape/{id}"),
        "weapon" => format!("Character/Weapon/{id}"),
        "earrings" | "faceAcc" | "eyeAcc" => format!("Character/Accessory/{id}"),
        _ => return None,
    };
    Some(key)
}

/// 在 img 樹上找動作節點；找不到時往數字名子節點裡找
/// （cash 武器在 img 下多一層武器類型節點）。
fn find_action(root: &WzNode, chain: &[&str]) -> Option<String> {
    find_action_at(root, "", chain).or_else(|| {
        root.children
            .iter()
            .filter(|c| is_digits(&c.name))
            .find_map(|c| find_action_at(c, &c.name, chain))
    })
}

fn find_action_at(base: &WzNode, base_rel: &str, chain: &[&str]) -> Option<String> {
    let rel = |name: &str| {
        if base_rel.is_empty() {
            name.to_string()
        } else {
            format!("{base_rel}/{name}")
        }
    };
    for cand in chain {
        if base
            .children
            .iter()
            .any(|c| &c.name == cand && looks_like_action(c))
        {
            return Some(rel(cand));
        }
    }
    // fallback：第一個非 info 且含幀/canvas 的子節點
    base.children
        .iter()
        .find(|c| c.name != "info" && looks_like_action(c))
        .map(|c| rel(&c.name))
}

/// 動作節點的幀列表（數字名子節點，依數值排序）；
/// 動作節點直接含 canvas 時（hair/face 的 default、head 的 front）視為單幀。
fn frame_list(root: &WzNode, action_rel: &str) -> Vec<String> {
    let Some(action_node) = root.find(action_rel) else {
        return Vec::new();
    };
    let mut frames: Vec<(u64, String)> = action_node
        .children
        .iter()
        .filter_map(|c| {
            c.name
                .parse::<u64>()
                .ok()
                .map(|n| (n, format!("{action_rel}/{}", c.name)))
        })
        .collect();
    frames.sort();
    if frames.is_empty() {
        vec![action_rel.to_string()]
    } else {
        frames.into_iter().map(|(_, rel)| rel).collect()
    }
}

/// 解析幀節點：跟隨 UOL，以及 action+frame 屬性的跨動作引用。
/// 回傳解析後的相對路徑。
fn resolve_frame(root: &WzNode, rel: &str) -> Option<String> {
    let mut path = rel.to_string();
    for _ in 0..RESOLVE_LIMIT {
        let (node, resolved) = resolve_node(root, &path)?;
        let action = node.find("action").and_then(|n| match &n.value {
            Value::Str(s) => Some(s.as_str()),
            _ => None,
        });
        let frame_no = node.find("frame").and_then(|n| value_as_i32(&n.value));
        if let (Some(action), Some(frame_no)) = (action, frame_no) {
            path = format!("{action}/{frame_no}");
        } else {
            return Some(resolved);
        }
    }
    None
}

/// 解析節點：若為 UOL 則跟隨到目標節點。回傳（目標節點, 目標相對路徑）。
/// 永遠以 UOL 值解析後的目標為準（節點名可能是 `defalutAc` 之類的原始拼字）。
fn resolve_node<'a>(root: &'a WzNode, rel: &str) -> Option<(&'a WzNode, String)> {
    let mut path = rel.to_string();
    for _ in 0..RESOLVE_LIMIT {
        let node = root.find(&path)?;
        match &node.value {
            Value::Uol(uol) => path = resolve_uol(&path, uol)?,
            _ => return Some((node, path)),
        }
    }
    None
}

/// 以 UOL 節點在 img 內的相對路徑為基準解析 `../` 連結
/// （UOL 相對於其父節點；規則對齊 image/render.rs 的 resolve_uol_target）。
fn resolve_uol(source_rel: &str, uol: &str) -> Option<String> {
    let mut segments: Vec<&str> = source_rel.split('/').filter(|s| !s.is_empty()).collect();
    segments.pop()?;
    for part in uol.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            segment => segments.push(segment),
        }
    }
    Some(segments.join("/"))
}

/// 從幀節點收集 canvas（解析 UOL；SubTree 子節點如 hairShade/0..N 再往內收一層）。
/// 回傳（canvas 節點, 解析後相對路徑），順序 = 資料原始順序。
/// 同一解析後路徑只收一次：hairShade 的 1..N 全是 UOL 別名指向 "0"（染髮色版），
/// 不去重的話同一張圖會被疊畫 N 次。
fn collect_canvases<'a>(
    root: &'a WzNode,
    frame_rel: &str,
    frame: &'a WzNode,
) -> Vec<(&'a WzNode, String)> {
    let mut out: Vec<(&'a WzNode, String)> = Vec::new();
    let mut push = |node: &'a WzNode, rel: String| {
        if matches!(node.value, Value::Canvas) && !out.iter().any(|(_, r)| *r == rel) {
            out.push((node, rel));
        }
    };
    for child in &frame.children {
        let child_rel = format!("{frame_rel}/{}", child.name);
        let Some((node, rel)) = resolve_node(root, &child_rel) else {
            continue;
        };
        match node.value {
            Value::Canvas => push(node, rel),
            Value::SubTree => {
                for group_child in &node.children {
                    let group_rel = format!("{rel}/{}", group_child.name);
                    if let Some((canvas, canvas_rel)) = resolve_node(root, &group_rel) {
                        push(canvas, canvas_rel);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

// ── 對齊與圖層 ──────────────────────────────────────────────────

/// 對齊一張 canvas 並把自己的錨點登錄到全域表。
/// body 部位的 body canvas 用 `root_pos`（該幀的 shift，讓各幀 origin 落在同一
/// 絕對位置）；其餘 canvas 用 map 中第一個已知錨點對齊。
/// 錨點先放者為準，不覆蓋同名。
fn place_canvas(
    root_pos: Option<(f32, f32)>,
    node: &WzNode,
    anchors: &mut HashMap<String, (f32, f32)>,
) -> (f32, f32) {
    let origin = vector_prop(node, "origin").unwrap_or((0.0, 0.0));
    let pos = match root_pos {
        Some(pos) => pos,
        None => align_to_anchors(node, anchors),
    };
    if let Some(map) = node.find("map") {
        for m in &map.children {
            if let Value::Vector(mx, my) = m.value {
                anchors
                    .entry(m.name.clone())
                    .or_insert((pos.0 + origin.0 + mx, pos.1 + origin.1 + my));
            }
        }
    }
    pos
}

/// 用全域錨點表對齊：pos = anchor_abs - origin - map_point
/// （找 map 中第一個已知錨點，忽略 muzzle；找不到放 (0,0)）。
fn align_to_anchors(node: &WzNode, anchors: &HashMap<String, (f32, f32)>) -> (f32, f32) {
    let origin = vector_prop(node, "origin").unwrap_or((0.0, 0.0));
    if let Some(map) = node.find("map") {
        for m in &map.children {
            if m.name == "muzzle" {
                continue;
            }
            if let (Some(&abs), Value::Vector(mx, my)) = (anchors.get(&m.name), &m.value) {
                return (abs.0 - origin.0 - *mx, abs.1 - origin.1 - *my);
            }
        }
    }
    (0.0, 0.0)
}

/// 幀位移（楓之谷 client 的做法）：以第 0 幀 body origin 為固定基準點，
/// 第 i 幀 shift = origin_0 - origin_i + move，讓各幀 origin 落在同一絕對位置。
/// 該幀沒有 body canvas（hide 類動作）時不位移。
fn frame_shift(
    origin_0: (f32, f32),
    body_origin: Option<(f32, f32)>,
    move_vec: Option<(f32, f32)>,
) -> (f32, f32) {
    let Some(origin) = body_origin else {
        return (0.0, 0.0);
    };
    let (mx, my) = move_vec.unwrap_or((0.0, 0.0));
    (origin_0.0 - origin.0 + mx, origin_0.1 - origin.1 + my)
}

/// canvas 的 map 中指定錨點。
fn map_point(node: &WzNode, name: &str) -> Option<(f32, f32)> {
    let point = node.find("map")?.children.iter().find(|c| c.name == name)?;
    match point.value {
        Value::Vector(x, y) => Some((x, y)),
        _ => None,
    }
}

/// face canvas 相對於 brow 錨點的相對座標：`-(origin + map_brow)`
/// （canvas 左上角相對 brow 的偏移，與身體幀無關）。
/// map 沒有 brow 時為 None（呼叫端 fallback 到絕對位置）。
fn face_relative_pos(node: &WzNode) -> Option<(f32, f32)> {
    let origin = vector_prop(node, "origin").unwrap_or((0.0, 0.0));
    let brow = map_point(node, "brow")?;
    Some((-(origin.0 + brow.0), -(origin.1 + brow.1)))
}

/// HairCover 規則（對齊 WzComparerR2）：遮髮時藏蓋頭髮層，不遮時藏帽下髮層。
fn hair_canvas_hidden(z: Option<&str>, hair_cover: bool) -> bool {
    let Some(z) = z else {
        return false;
    };
    const WITH_CAP: &[&str] = &["hairOverHead", "backHairOverCape", "backHair"];
    const NO_CAP: &[&str] = &[
        "hair",
        "backHairBelowCap",
        "backHairBelowCapWide",
        "backHairBelowCapNarrow",
    ];
    if hair_cover {
        WITH_CAP.contains(&z)
    } else {
        NO_CAP.contains(&z)
    }
}

/// canvas 的 z 圖層值 → zIndex（規則對齊 WzComparerR2）。
fn z_index(node: &WzNode) -> i32 {
    match node.find("z").map(|n| &n.value) {
        Some(Value::Str(s)) => ZMAP
            .iter()
            .position(|name| name == s)
            .map(|i| i as i32)
            .unwrap_or(ZMAP.len() as i32),
        Some(Value::U16(n)) => int_z_index(i32::from(*n)),
        Some(Value::Int(n)) => int_z_index(*n),
        _ => ZMAP.len() as i32,
    }
}

/// 整數 z：n<0 時排到 `len-n`，n≥0 時 `-(1+n)`（排最上）。
fn int_z_index(n: i32) -> i32 {
    if n < 0 {
        ZMAP.len() as i32 - n
    } else {
        -(1 + n)
    }
}

// ── 小工具 ──────────────────────────────────────────────────────

/// 8 碼裝備 id → 部位 slot（前綴規則；01092x 副武器不算盾牌）。
fn slot_of(id: &str) -> Option<&'static str> {
    let prefix3 = id.parse::<u32>().ok()? / 10000;
    match prefix3 {
        2 => Some("face"),
        3 => Some("hair"),
        100 => Some("cap"),
        101 => Some("faceAcc"),
        102 => Some("eyeAcc"),
        103 => Some("earrings"),
        104 => Some("coat"),
        105 => Some("longcoat"),
        106 => Some("pants"),
        107 => Some("shoes"),
        108 => Some("glove"),
        109 => {
            if id.starts_with("01092") {
                None
            } else {
                Some("shield")
            }
        }
        110 => Some("cape"),
        121..=179 => Some("weapon"),
        _ => None,
    }
}

/// body id → head id：headID = 10000 + bodyID % 10000（皆 8 碼零填充）。
fn head_id_of(body_id: &str) -> String {
    let body: u32 = body_id.parse().unwrap_or(0);
    format!("{:08}", 10000 + body % 10000)
}

/// 名稱表查詢：key 是去前導零的 id；查無用 id 本身。
fn item_name(names: &crate::item_tooltip::ItemNameTable, id: &str) -> String {
    id.parse::<u32>()
        .ok()
        .and_then(|n| names.get(&n))
        .map(|(name, _)| name.clone())
        .unwrap_or_else(|| id.to_string())
}

/// 幀索引循環：部位幀數與主幀數不一致時 modulo。
fn wrap_index(i: usize, len: usize) -> usize {
    i % len
}

fn is_digits(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit())
}

/// 節點是否像個動作：含數字名子節點（幀）或 canvas/UOL。
fn looks_like_action(node: &WzNode) -> bool {
    node.children
        .iter()
        .any(|c| is_digits(&c.name) || matches!(c.value, Value::Canvas | Value::Uol(_)))
}

fn frame_delay(node: &WzNode) -> i32 {
    node.find("delay")
        .and_then(|n| value_as_i32(&n.value))
        .unwrap_or(DEFAULT_DELAY)
}

/// 幀屬性 face=0 → 該幀不畫臉。
fn frame_face_visible(node: &WzNode) -> bool {
    node.find("face").and_then(|n| value_as_i32(&n.value)) != Some(0)
}

/// 該 img 是否點裝：`info/cash` 數值非零（Int/U8/U16 皆可能；
/// 節點不存在或非數值 → false）。
fn is_cash(tree: &WzNode) -> bool {
    let Some(info) = tree.find("info") else {
        return false;
    };
    info.children
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case("cash"))
        .and_then(|n| value_as_i32(&n.value))
        .unwrap_or(0)
        != 0
}

fn z_str(node: &WzNode) -> Option<&str> {
    match node.find("z").map(|n| &n.value) {
        Some(Value::Str(s)) => Some(s),
        _ => None,
    }
}

fn vector_prop(node: &WzNode, name: &str) -> Option<(f32, f32)> {
    match node.find(name).map(|n| &n.value) {
        Some(Value::Vector(x, y)) => Some((*x, *y)),
        _ => None,
    }
}

fn value_as_i32(value: &Value) -> Option<i32> {
    match value {
        Value::U8(v) => Some(i32::from(*v)),
        Value::U16(v) => Some(i32::from(*v)),
        Value::Int(v) => Some(*v),
        Value::Int2(a, _) => Some(*a),
        Value::Double(v) => Some(*v as i32),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(name: &str, value: Value) -> WzNode {
        WzNode {
            name: name.into(),
            value,
            path: None,
            children: vec![],
            type_tag: 0,
        }
    }

    /// 造一張 canvas 節點（z / origin / map 子節點）。
    fn canvas_node(name: &str, z: &str, origin: (f32, f32), map: &[(&str, (f32, f32))]) -> WzNode {
        let map_children = map
            .iter()
            .map(|(n, (x, y))| leaf(n, Value::Vector(*x, *y)))
            .collect();
        WzNode {
            name: name.into(),
            value: Value::Canvas,
            path: None,
            children: vec![
                leaf("z", Value::Str(z.into())),
                leaf("origin", Value::Vector(origin.0, origin.1)),
                WzNode {
                    name: "map".into(),
                    value: Value::SubTree,
                    path: None,
                    children: map_children,
                    type_tag: 2,
                },
            ],
            type_tag: 18,
        }
    }

    #[test]
    fn slot_prefix_rules() {
        assert_eq!(slot_of("00020000"), Some("face"));
        assert_eq!(slot_of("00030000"), Some("hair"));
        assert_eq!(slot_of("01000000"), Some("cap"));
        assert_eq!(slot_of("01010000"), Some("faceAcc"));
        assert_eq!(slot_of("01020000"), Some("eyeAcc"));
        assert_eq!(slot_of("01030000"), Some("earrings"));
        assert_eq!(slot_of("01040036"), Some("coat"));
        assert_eq!(slot_of("01050000"), Some("longcoat"));
        assert_eq!(slot_of("01060026"), Some("pants"));
        assert_eq!(slot_of("01070000"), Some("shoes"));
        assert_eq!(slot_of("01080000"), Some("glove"));
        assert_eq!(slot_of("01090000"), Some("shield"));
        // 01092x 是副武器類，不算盾牌
        assert_eq!(slot_of("01092000"), None);
        assert_eq!(slot_of("01100000"), Some("cape"));
        assert_eq!(slot_of("01210000"), Some("weapon"));
        assert_eq!(slot_of("01302000"), Some("weapon"));
        assert_eq!(slot_of("01702000"), Some("weapon"));
        // 0112–0114 戒指/吊墜/腰帶不收
        assert_eq!(slot_of("01120000"), None);
        assert_eq!(slot_of("01130000"), None);
        assert_eq!(slot_of("01140000"), None);
        assert_eq!(slot_of("01800000"), None);
    }

    #[test]
    fn head_body_id_conversion() {
        assert_eq!(head_id_of("00002000"), "00012000");
        assert_eq!(head_id_of("00002005"), "00012005");
        assert_eq!(head_id_of("00002999"), "00012999");
    }

    #[test]
    fn z_index_rules() {
        let body = canvas_node("c", "body", (0.0, 0.0), &[]);
        assert_eq!(
            z_index(&body),
            ZMAP.iter().position(|z| *z == "body").unwrap() as i32
        );
        // 未知名排最底
        let unknown = canvas_node("c", "noSuchLayer", (0.0, 0.0), &[]);
        assert_eq!(z_index(&unknown), ZMAP.len() as i32);
        // 整數 z：n≥0 → -(1+n)（最上）；n<0 → len-n
        let mut int_z = canvas_node("c", "body", (0.0, 0.0), &[]);
        int_z.children[0].value = Value::Int(2);
        assert_eq!(z_index(&int_z), -3);
        int_z.children[0].value = Value::Int(-2);
        assert_eq!(z_index(&int_z), ZMAP.len() as i32 + 2);
        int_z.children[0].value = Value::U16(0);
        assert_eq!(z_index(&int_z), -1);
        // 缺 z → 最底
        let no_z = WzNode {
            name: "c".into(),
            value: Value::Canvas,
            path: None,
            children: vec![],
            type_tag: 18,
        };
        assert_eq!(z_index(&no_z), ZMAP.len() as i32);
    }

    #[test]
    fn uol_relative_resolution() {
        assert_eq!(
            resolve_uol("stand1/0/head", "../../front/head").as_deref(),
            Some("front/head")
        );
        // cap 動作幀的原始拼字 defalutAc：用 UOL 值解析，不看節點名
        assert_eq!(
            resolve_uol("stand1/0/defalutAc", "../../default/defaultAc").as_deref(),
            Some("default/defaultAc")
        );
        assert_eq!(
            resolve_uol("blink/0/face", "../../default/face").as_deref(),
            Some("default/face")
        );
        assert_eq!(
            resolve_uol("walk1/2/arm", "../0/arm").as_deref(),
            Some("walk1/0/arm")
        );
        // 上溢（.. 超過根）→ None
        assert_eq!(resolve_uol("a", "../../../x"), None);
    }

    #[test]
    fn align_math() {
        let mut anchors = HashMap::new();
        // body canvas 放基準點（此例 shift=(0,0)），錨點絕對值 = pos + origin + map
        let body = canvas_node("body", "body", (20.0, 30.0), &[("neck", (5.0, -10.0))]);
        assert_eq!(
            place_canvas(Some((0.0, 0.0)), &body, &mut anchors),
            (0.0, 0.0)
        );
        assert_eq!(anchors.get("neck"), Some(&(25.0, 20.0)));
        // head：pos = anchor_abs - origin - map = (25-15-3, 20-25-8) = (7, -13)
        let head = canvas_node("head", "head", (15.0, 25.0), &[("neck", (3.0, 8.0))]);
        assert_eq!(place_canvas(None, &head, &mut anchors), (7.0, -13.0));
        // 先放者為準：同名錨點不覆蓋
        let dup = canvas_node("x", "cap", (0.0, 0.0), &[("neck", (99.0, 99.0))]);
        place_canvas(None, &dup, &mut anchors);
        assert_eq!(anchors.get("neck"), Some(&(25.0, 20.0)));
        // muzzle 錨點忽略；找不到已知錨點放 (0,0)
        let mut anchors2 = HashMap::new();
        anchors2.insert("muzzle".to_string(), (50.0, 50.0));
        let only_muzzle = canvas_node("w", "weapon", (1.0, 1.0), &[("muzzle", (2.0, 2.0))]);
        assert_eq!(place_canvas(None, &only_muzzle, &mut anchors2), (0.0, 0.0));
    }

    #[test]
    fn frame_shift_math() {
        // shift = origin_0 - origin_i + move：(16-19+2, 31-32+0) = (-1, -1)
        let origin_0 = (16.0, 31.0); // stand1 body origin
        assert_eq!(
            frame_shift(origin_0, Some((19.0, 32.0)), Some((2.0, 0.0))),
            (-1.0, -1.0)
        );
        // 無 move → 純 origin 差
        assert_eq!(
            frame_shift(origin_0, Some((19.0, 32.0)), None),
            (-3.0, -1.0)
        );
        // 第 0 幀自己：shift = move（通常 0）
        assert_eq!(frame_shift(origin_0, Some(origin_0), None), (0.0, 0.0));
        // 無 body canvas 的幀不位移
        assert_eq!(frame_shift(origin_0, None, Some((5.0, 5.0))), (0.0, 0.0));
    }

    #[test]
    fn face_relative_math() {
        // 相對座標 = -(origin + map_brow) = -(15+2, 14+3) = (-17, -17)
        let face = canvas_node("face", "face", (15.0, 14.0), &[("brow", (2.0, 3.0))]);
        assert_eq!(face_relative_pos(&face), Some((-17.0, -17.0)));
        // map 沒有 brow → None（呼叫端 fallback）
        let no_brow = canvas_node("face", "face", (15.0, 14.0), &[("neck", (2.0, 3.0))]);
        assert_eq!(face_relative_pos(&no_brow), None);
    }

    #[test]
    fn frame_index_wraps() {
        assert_eq!(wrap_index(0, 3), 0);
        assert_eq!(wrap_index(3, 3), 0);
        assert_eq!(wrap_index(5, 3), 2);
        assert_eq!(wrap_index(7, 2), 1);
    }

    #[test]
    fn cash_flag_rules() {
        let tree_with_info = |cash: Option<Value>| WzNode {
            name: "img".into(),
            value: Value::SubTree,
            path: None,
            children: vec![WzNode {
                name: "info".into(),
                value: Value::SubTree,
                path: None,
                children: cash.map(|v| vec![leaf("cash", v)]).unwrap_or_default(),
                type_tag: 2,
            }],
            type_tag: 2,
        };
        // 非零數值（Int/U8/U16）→ true
        assert!(is_cash(&tree_with_info(Some(Value::Int(1)))));
        assert!(is_cash(&tree_with_info(Some(Value::U8(1)))));
        assert!(is_cash(&tree_with_info(Some(Value::U16(2)))));
        // 0 → false
        assert!(!is_cash(&tree_with_info(Some(Value::Int(0)))));
        // 非數值 → false
        assert!(!is_cash(&tree_with_info(Some(Value::Str("1".into())))));
        // 無 cash 節點 → false
        assert!(!is_cash(&tree_with_info(None)));
        // 無 info 節點 → false
        let bare = WzNode {
            name: "img".into(),
            value: Value::SubTree,
            path: None,
            children: vec![],
            type_tag: 2,
        };
        assert!(!is_cash(&bare));
    }

    #[test]
    fn layers_sort_z_descending() {
        let mut layers = [
            Layer {
                path: "a".into(),
                x: 0.0,
                y: 0.0,
                z: 40,
            },
            Layer {
                path: "b".into(),
                x: 0.0,
                y: 0.0,
                z: 74,
            },
            Layer {
                path: "c".into(),
                x: 0.0,
                y: 0.0,
                z: 49,
            },
        ];
        layers.sort_by_key(|l| std::cmp::Reverse(l.z));
        let order: Vec<i32> = layers.iter().map(|l| l.z).collect();
        assert_eq!(order, vec![74, 49, 40]);
    }
}
