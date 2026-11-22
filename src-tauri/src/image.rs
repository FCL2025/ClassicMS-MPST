//! 圖片索引與取圖服務（Phase 4）。
//!
//! 建立「邏輯路徑 → 圖片來源」索引，跟 Catalog 一起建/快取：
//! - **spritesheet_***：container 裡的 `.wzspritesheet`（舊 WZSS）或 `.asset`
//!   （新版索引物件）+ `_N.png`（atlas 頁 Texture2D）。索引提供每個 sprite 的 rect。
//! - **spriteset_***：container 裡的 `.asset`（sprite 清單 MonoBehaviour，
//!   payload = (count, (string, pathID)×n) 或 (count, PPtr×n)）+ Sprite(213)
//!   物件（name / m_Rect / m_RD.texture）+ Texture2D。
//!
//! WZSS 格式（逆向定案，見 docs/PLAN.md）。header 是 20 個 u32，
//! 所有 offset 以 magic 位置為基準（實作上直接搜 `WZSS` bytes，最穩）：
//! ```text
//! [0]      u32 atlas 頁數
//! [1]      常數 8
//! [2..5)   UOL 字串池（count / blobOff / tableOff）
//! [5..7)   sprite pivot（count / off，8B/筆 = 正規化 2×f32）
//! [7..9)   sprite rect（count / off，16B/筆 = u32 x,y,w,h；座標是「單頁內」的）
//! [9..11)  frame（count / off，20B/筆；第 3 個 i32 是 WZSS atlas 頁號、
//!          第 4 個是 rect index）
//! [11..13) link（count / off，16B/筆 = 槽號 / UOL 字串 index(-1) / ? / frame index）
//! [13..16) 節點鍵池（count / blobOff / tableOff）：每筆 32 hex，以鍵字串排序，
//!          count = 該 img 的 canvas + uol 節點數
//! [16..19) 每個 frame 的 64 hex 池（count / blobOff / tableOff）
//! [19]     path 長度，接著 path 字串（= img 邏輯路徑）
//! ```
//! path 之後（對齊到 4）是 atlas 頁的 Addressables GUID 清單（count u32、
//! 每筆 長度 u32(=32) + hex GUID + 8 bytes 0），再一個 u32 剩餘長度，
//! 接著才是 `WZSS` magic + u32 version(=4)。
//!
//! magic + 8 起是 `頁數` 個 u32 的**頁對照表**：第 i 項 = 「WZSS 頁 i」對應的
//! atlas 檔名 `_N` 頁號。兩套編號常常不同（`Skill/232` 是 [3,4,2,1,0]），
//! 少了這層換算會整頁切錯圖。
//!
//! 節點鍵池是 sprite 查表索引，鍵的算法見 [`wzss`] 模組。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::catalog::{DataService, EntryLocation};
use crate::unity::bundle::Bundle;
use crate::unity::objects;
use crate::unity::serialized_file::SerializedFile;
use crate::unity::Result;

mod sheet_asset;
mod wzss;
pub use sheet_asset::{parse_sheet_asset, SheetAsset};
pub use wzss::{cut_sprite, parse_wzss, AtlasPage, WzssIndex, WzssRect, WzssSprite, WzssUolLink};

/// spritesheet：索引物件 + atlas 頁（依 _0, _1… 排序）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SheetEntry {
    /// container 條目順序不保證 index 早於 atlas，因此建構途中允許尚未看到 index。
    #[serde(default)]
    pub index_loc: Option<EntryLocation>,
    /// 新版圖集用 .asset 保存路徑與裁切資料；舊版仍使用 WZSS。
    #[serde(default)]
    pub index_is_asset: bool,
    /// (page 序號, Texture2D 物件位置)
    pub atlas_pages: Vec<(u32, EntryLocation)>,
}

/// spriteset 的一個 sprite：Sprite 物件位置 + 其 Texture2D 物件位置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetSpriteEntry {
    pub sprite_loc: EntryLocation,
    pub texture_loc: EntryLocation,
}

/// 圖片索引。
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ImageIndex {
    /// 邏輯路徑 → 語系 → 該語系的一組索引與圖頁。
    #[serde(default)]
    pub(crate) sheets_by_path: BTreeMap<String, BTreeMap<String, SheetEntry>>,
    /// "Map/WorldMap/BaseImg"（.asset 路徑去前綴去副檔名）→ sprite 清單（名稱 → sprite）
    pub set_assets: BTreeMap<String, BTreeMap<String, SetSpriteEntry>>,
}

const SHEET_PREFIX: &str = "assets/wzassets/spritesheet/";
const SET_PREFIX: &str = "assets/wzassets/spriteset/";

fn sheet_path(path: &str) -> Option<(&str, &str)> {
    let prefix = path.get(..SHEET_PREFIX.len())?;
    if !prefix.eq_ignore_ascii_case(SHEET_PREFIX) {
        return None;
    }
    let (locale, key) = path[SHEET_PREFIX.len()..].split_once('/')?;
    (!locale.is_empty() && !key.is_empty()).then_some((locale, key))
}

/// 優先使用目前語系，再使用共用圖集。只有一份的素材可供各語系共用。
fn localized_sheet<'a>(
    variants: &'a BTreeMap<String, SheetEntry>,
    locale: &str,
) -> Option<&'a SheetEntry> {
    if variants.len() == 1 {
        return variants.values().next();
    }
    variants
        .iter()
        .find(|(code, _)| code.eq_ignore_ascii_case(locale))
        .map(|(_, sheet)| sheet)
        .or_else(|| variants.get("COMMON"))
}

impl DataService {
    pub fn sheet(&self, key: &str) -> Option<&SheetEntry> {
        localized_sheet(self.catalog.image.sheets_by_path.get(key)?, self.locale())
    }

    pub fn sheets(&self) -> impl Iterator<Item = (&String, &SheetEntry)> {
        self.catalog
            .image
            .sheets_by_path
            .iter()
            .filter_map(|(key, variants)| {
                localized_sheet(variants, self.locale()).map(|sheet| (key, sheet))
            })
    }
}

pub(crate) struct SerializedImageContext<'a> {
    pub bundle: &'a Bundle,
    pub bundle_index: u32,
    pub stem: &'a str,
    pub node_index: usize,
    pub serialized_file: &'a SerializedFile,
    pub container: &'a [objects::ContainerEntry],
    pub by_path_id:
        &'a std::collections::HashMap<i64, &'a crate::unity::serialized_file::ObjectInfo>,
}

impl ImageIndex {
    /// 使用 Catalog 已解析的 SerializedFile 與 AssetBundle container 收錄圖片來源。
    /// 這避免為 JSON 與圖片索引重複讀取、解析同一份 metadata。
    pub(crate) fn collect_from_container(
        &mut self,
        context: SerializedImageContext<'_>,
    ) -> Result<()> {
        let SerializedImageContext {
            bundle,
            bundle_index,
            stem,
            node_index,
            serialized_file: sf,
            container,
            by_path_id,
        } = context;
        let is_sheet = stem.starts_with("spritesheet_");
        let is_set = stem.starts_with("spriteset_");
        if !is_sheet && !is_set {
            return Ok(());
        }
        let loc_of = |path_id: i64| -> Option<EntryLocation> {
            by_path_id.get(&path_id).and_then(|object| {
                sf.data_offset
                    .checked_add(object.byte_start as u64)
                    .map(|abs_offset| EntryLocation {
                        bundle_index,
                        node_index: node_index as u32,
                        abs_offset,
                        byte_size: object.byte_size,
                        class_id: object.class_id,
                    })
            })
        };
        if is_sheet {
            self.collect_sheet(container, &loc_of)?;
        } else {
            self.collect_set(bundle, node_index, sf, container, &loc_of)?;
        }
        Ok(())
    }

    fn collect_sheet(
        &mut self,
        container: &[objects::ContainerEntry],
        loc_of: &dyn Fn(i64) -> Option<EntryLocation>,
    ) -> Result<()> {
        for e in container {
            let Some((locale, rest)) = sheet_path(&e.path) else {
                continue;
            };
            if let Some((key, is_asset)) = rest
                .strip_suffix(".wzspritesheet")
                .map(|key| (key, false))
                .or_else(|| rest.strip_suffix(".asset").map(|key| (key, true)))
            {
                let Some(index_loc) = loc_of(e.asset.path_id) else {
                    continue;
                };
                let entry = self
                    .sheets_by_path
                    .entry(key.to_owned())
                    .or_default()
                    .entry(locale.to_ascii_uppercase())
                    .or_default();
                entry.index_loc = Some(index_loc);
                entry.index_is_asset = is_asset;
            } else if let Some(base) = strip_png_suffix(rest) {
                // "login_2.png" → key "login", page 2
                let Some((key, page)) = split_page_suffix(base) else {
                    continue;
                };
                let Some(loc) = loc_of(e.asset.path_id) else {
                    continue;
                };
                let entry = self
                    .sheets_by_path
                    .entry(key.to_owned())
                    .or_default()
                    .entry(locale.to_ascii_uppercase())
                    .or_default();
                if !entry.atlas_pages.iter().any(|(p, _)| *p == page) {
                    entry.atlas_pages.push((page, loc));
                    entry.atlas_pages.sort_by_key(|(p, _)| *p);
                }
            }
        }
        Ok(())
    }

    fn collect_set(
        &mut self,
        bundle: &Bundle,
        node_index: usize,
        sf: &SerializedFile,
        container: &[objects::ContainerEntry],
        loc_of: &dyn Fn(i64) -> Option<EntryLocation>,
    ) -> Result<()> {
        // .asset path → (名稱 → sprite pid)
        let mut asset_map: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
        for e in container {
            let lower = e.path.to_lowercase();
            if !lower.starts_with(SET_PREFIX) || !lower.ends_with(".asset") {
                continue;
            }
            let key = e.path[SET_PREFIX.len()..e.path.len() - 6].to_string();
            let Some(asset_loc) = loc_of(e.asset.path_id) else {
                continue;
            };
            let data = bundle.read_node_range(
                node_index,
                asset_loc.abs_offset,
                asset_loc.byte_size as usize,
            )?;
            let Ok(mono) = objects::parse_mono_behaviour(&data, sf) else {
                continue;
            };
            let Some(payload) = data.get(mono.header_len..) else {
                continue;
            };
            for (name, pid) in parse_set_asset_payload(payload) {
                asset_map.entry(key.clone()).or_default().insert(name, pid);
            }
        }
        // sprite pid → SetSpriteEntry（解析 Sprite 拿 texture pid）
        for (key, sprites) in asset_map {
            let mut out = BTreeMap::new();
            for (name, pid) in sprites {
                let Some(sprite_loc) = loc_of(pid) else {
                    continue;
                };
                if sprite_loc.class_id != objects::CLASS_SPRITE {
                    continue;
                }
                let data = match bundle.read_node_range(
                    sprite_loc.node_index as usize,
                    sprite_loc.abs_offset,
                    sprite_loc.byte_size as usize,
                ) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                let Ok(sp) = objects::parse_sprite(&data, sf) else {
                    continue;
                };
                if sp.texture.file_id != 0 {
                    continue;
                }
                let Some(texture_loc) = loc_of(sp.texture.path_id) else {
                    continue;
                };
                out.insert(
                    name,
                    SetSpriteEntry {
                        sprite_loc,
                        texture_loc,
                    },
                );
            }
            if !out.is_empty() {
                self.set_assets.insert(key, out);
            }
        }
        Ok(())
    }
}

fn strip_png_suffix(s: &str) -> Option<&str> {
    s.strip_suffix(".png")
}

/// "login_2" → ("login", 2)；無 _N 後綴 → (base, 0)。
fn split_page_suffix(base: &str) -> Option<(&str, u32)> {
    let (key, num) = base.rsplit_once('_')?;
    let page: u32 = num.parse().ok()?;
    Some((key, page))
}

/// spriteset .asset payload：(count, (string, pathID)×n) 或 (count, PPtr×n)；
/// 另有無 count 前綴、直接連排 PPtr 的佈局（UICashShop 等 spriteset）。
/// 回傳 (名稱, sprite pathID) 列表；未命名條目名稱給 ""。
fn parse_set_asset_payload(payload: &[u8]) -> Vec<(String, i64)> {
    let u32_at = |pos: usize| -> Option<u32> {
        let end = pos.checked_add(4)?;
        payload
            .get(pos..end)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    let Some(count) = u32_at(0) else {
        return vec![];
    };
    if count > 100_000 {
        return vec![];
    }
    // count 為 0 有兩種可能：真的是空集合，或根本沒有 count 前綴、
    // 開頭就是 PPtr（fileID = 0）。後者直接連排解析，撞到 pid = 0 或非零
    // fileID 即停（這類 .asset 尾部常帶額外欄位，例如 canvas_event）。
    if count == 0 {
        return parse_bare_pptrs(payload);
    }
    // 判斷 named：count 之後的 u32 是合理字串長度且內容可印
    let named = match u32_at(4) {
        Some(l) if l > 0 && l < 512 => (8usize)
            .checked_add(l as usize)
            .and_then(|end| payload.get(8..end))
            .is_some_and(|b| b.iter().all(|c| c.is_ascii_graphic() || *c == b' ')),
        _ => false,
    };
    let mut out = Vec::with_capacity(count as usize);
    let mut pos = 4usize;
    for _ in 0..count {
        if named {
            let Some(l) = u32_at(pos) else { break };
            let l = l as usize;
            let Some(start) = pos.checked_add(4) else {
                break;
            };
            let Some(end) = start.checked_add(l) else {
                break;
            };
            let Some(bytes) = payload.get(start..end) else {
                break;
            };
            let name = String::from_utf8_lossy(bytes).into_owned();
            let Some(aligned) = end.checked_add(3).map(|n| n & !3) else {
                break;
            };
            pos = aligned;
            let Some(pid_end) = pos.checked_add(8) else {
                break;
            };
            let Some(pid) = payload
                .get(pos..pid_end)
                .map(|b| i64::from_le_bytes(b.try_into().unwrap()))
            else {
                break;
            };
            pos = pid_end;
            out.push((name, pid));
        } else {
            // PPtr: fileID i32 + pathID i64
            let Some(pptr_end) = pos.checked_add(12) else {
                break;
            };
            let Some(file_id_end) = pos.checked_add(4) else {
                break;
            };
            let Some(file_id) = payload
                .get(pos..file_id_end)
                .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
            else {
                break;
            };
            if file_id != 0 {
                break;
            }
            let Some(pid) = payload
                .get(file_id_end..pptr_end)
                .map(|b| i64::from_le_bytes(b.try_into().unwrap()))
            else {
                break;
            };
            pos = pptr_end;
            out.push((String::new(), pid));
        }
    }
    out
}

/// 無 count 前綴的 PPtr 連排：(fileID i32, pathID i64)×n，
/// fileID 非 0 或 pathID 為 0 時結束。
fn parse_bare_pptrs(payload: &[u8]) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while let Some(end) = pos.checked_add(12).filter(|&end| end <= payload.len()) {
        let file_id = i32::from_le_bytes(payload[pos..pos + 4].try_into().unwrap());
        let pid = i64::from_le_bytes(payload[pos + 4..end].try_into().unwrap());
        if file_id != 0 || pid == 0 {
            break;
        }
        out.push((String::new(), pid));
        pos = end;
    }
    out
}

mod render;

#[cfg(test)]
use render::sprite_rect_to_pixels;
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_set_asset_named() {
        // (count=1, aligned string + pathID)
        let mut payload = Vec::new();
        payload.extend_from_slice(&1u32.to_le_bytes());
        let name = b"Util/test/checked";
        payload.extend_from_slice(&(name.len() as u32).to_le_bytes());
        payload.extend_from_slice(name);
        while payload.len() % 4 != 0 {
            payload.push(0);
        }
        payload.extend_from_slice(&123456789i64.to_le_bytes());
        let out = parse_set_asset_payload(&payload);
        assert_eq!(out, vec![("Util/test/checked".to_string(), 123456789)]);
    }

    #[test]
    fn test_parse_set_asset_unnamed() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&2u32.to_le_bytes());
        payload.extend_from_slice(&0i32.to_le_bytes());
        payload.extend_from_slice(&111i64.to_le_bytes());
        payload.extend_from_slice(&0i32.to_le_bytes());
        payload.extend_from_slice(&222i64.to_le_bytes());
        let out = parse_set_asset_payload(&payload);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], (String::new(), 111));
        assert_eq!(out[1], (String::new(), 222));
    }

    #[test]
    fn test_parse_set_asset_bare_pptrs() {
        // 無 count 前綴：直接 PPtr 連排，尾部帶非 PPtr 欄位（參考 canvas_event）。
        // 尾部欄位可能被誤判成一筆多餘 PPtr，但該 pid 在物件表查無，
        // 會在 collect_set 的 loc_of 階段被濾掉，這裡只斷言開頭正確。
        let mut payload = Vec::new();
        payload.extend_from_slice(&0i32.to_le_bytes());
        payload.extend_from_slice(&0x2fd527b174683368i64.to_le_bytes());
        payload.extend_from_slice(&0i64.to_le_bytes());
        payload.extend_from_slice(&1u32.to_le_bytes());
        let out = parse_set_asset_payload(&payload);
        assert_eq!(out.first(), Some(&(String::new(), 0x2fd527b174683368)));
    }

    #[test]
    fn test_split_page_suffix() {
        assert_eq!(split_page_suffix("login_2"), Some(("login", 2)));
        assert_eq!(split_page_suffix("login_0"), Some(("login", 0)));
        assert_eq!(split_page_suffix("login"), None);
    }

    #[test]
    fn test_sprite_rect_requires_exact_positive_pixel_size() {
        assert_eq!(
            sprite_rect_to_pixels((1.0, 2.0, 3.0, 4.0)).unwrap(),
            (1, 2, 3, 4)
        );
        assert!(sprite_rect_to_pixels((0.5, 0.0, 1.0, 1.0)).is_err());
        assert!(sprite_rect_to_pixels((0.0, 0.0, 0.0, 1.0)).is_err());
        assert!(sprite_rect_to_pixels((f32::NAN, 0.0, 1.0, 1.0)).is_err());
    }

    #[test]
    fn test_wzss_huge_rect_count_is_rejected() {
        let mut payload = vec![0u8; 96];
        payload[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        payload[80..84].copy_from_slice(b"WZSS");
        assert!(parse_wzss(&payload).is_err());
    }

    #[test]
    fn test_wzss_uol_frame_is_resolved_to_frame_and_empty_sentinel() {
        let mut payload = vec![0u8; 320];
        {
            let mut set_header = |index: usize, value: u32| {
                payload[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
            };
            set_header(0, 2); // atlas 頁數
            set_header(2, 2); // UOL string count
            set_header(3, 32); // string blob, relative to magic
            set_header(4, 16); // string offset table
            set_header(7, 2); // rect count
            set_header(8, 64); // rect data
            set_header(9, 3); // frame count
            set_header(10, 96); // frame data
            set_header(11, 2); // link count
            set_header(12, 160); // link data
            set_header(19, 4); // path length
        }

        payload[80..84].copy_from_slice(b"root");
        let magic = 128;
        payload[magic..magic + 4].copy_from_slice(b"WZSS");
        payload[magic + 4..magic + 8].copy_from_slice(&4u32.to_le_bytes()); // 版本
                                                                            // 頁對照表：WZSS 頁 0/1 → 檔名頁 1/0。
        payload[magic + 8..magic + 12].copy_from_slice(&1u32.to_le_bytes());
        payload[magic + 12..magic + 16].copy_from_slice(&0u32.to_le_bytes());
        let uol_a = b"../../default/default";
        let uol_b = b"../empty";
        payload[magic + 16..magic + 20].copy_from_slice(&0u32.to_le_bytes());
        payload[magic + 20..magic + 24].copy_from_slice(&(uol_a.len() as u32).to_le_bytes());
        payload[magic + 24..magic + 28]
            .copy_from_slice(&((uol_a.len() + uol_b.len()) as u32).to_le_bytes());
        payload[magic + 32..magic + 32 + uol_a.len()].copy_from_slice(uol_a);
        payload[magic + 32 + uol_a.len()..magic + 32 + uol_a.len() + uol_b.len()]
            .copy_from_slice(uol_b);

        for (index, rect) in [[1u32, 2, 3, 4], [5, 6, 7, 8]].into_iter().enumerate() {
            for (field, value) in rect.into_iter().enumerate() {
                let position = magic + 64 + index * 16 + field * 4;
                payload[position..position + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        // WzSpriteFrame 第三欄是 atlas 頁、第四欄是 rect index；frame 0 刻意
        // 對應 rect 1，且三個 frame 分別落在第 0/1/0 頁。
        for (frame, (page, rect)) in [(0i32, 1i32), (1, 0), (0, 1)].into_iter().enumerate() {
            let position = magic + 96 + frame * 20 + 8;
            payload[position..position + 4].copy_from_slice(&page.to_le_bytes());
            payload[position + 4..position + 8].copy_from_slice(&rect.to_le_bytes());
        }
        // link 0 → frame 0；link 1 → frame_count sentinel → 空圖。
        for (link, (string, frame)) in [(0i32, 0i32), (1, 3)].into_iter().enumerate() {
            let position = magic + 160 + link * 16;
            payload[position + 4..position + 8].copy_from_slice(&string.to_le_bytes());
            payload[position + 12..position + 16].copy_from_slice(&frame.to_le_bytes());
        }

        let parsed = parse_wzss(&payload).unwrap();
        assert_eq!(parsed.path, "root");
        assert_eq!(parsed.rects.len(), 2);
        assert_eq!(
            parsed.uol_links,
            vec![
                WzssUolLink {
                    uol: "../../default/default".into(),
                    frame_index: Some(0),
                },
                WzssUolLink {
                    uol: "../empty".into(),
                    frame_index: None,
                }
            ]
        );
        // frame 0 在 WZSS 頁 0（= 檔名頁 1）、對到 rect 1；
        // frame 1 在 WZSS 頁 1（= 檔名頁 0）、對到 rect 0。
        assert_eq!(
            parsed.sprite_for_frame(0),
            Some(WzssSprite {
                page: 1,
                rect: parsed.rects[1],
            })
        );
        assert_eq!(
            parsed.sprite_for_frame(1),
            Some(WzssSprite {
                page: 0,
                rect: parsed.rects[0],
            })
        );
        assert_eq!(parsed.sprite_for_frame(3), None);
    }

    #[test]
    fn test_sheet_collection_does_not_depend_on_container_order() {
        let container = vec![
            objects::ContainerEntry {
                path: format!("{SHEET_PREFIX}tw/login_0.png"),
                preload_index: 0,
                preload_size: 0,
                asset: objects::PPtr {
                    file_id: 0,
                    path_id: 10,
                },
            },
            objects::ContainerEntry {
                path: format!("{SHEET_PREFIX}tw/login.wzspritesheet"),
                preload_index: 0,
                preload_size: 0,
                asset: objects::PPtr {
                    file_id: 0,
                    path_id: 20,
                },
            },
            objects::ContainerEntry {
                path: format!("{SHEET_PREFIX}tw/Character/00002000_0.png"),
                preload_index: 0,
                preload_size: 0,
                asset: objects::PPtr {
                    file_id: 0,
                    path_id: 30,
                },
            },
            objects::ContainerEntry {
                path: format!("{SHEET_PREFIX}tw/Character/00002000.asset"),
                preload_index: 0,
                preload_size: 0,
                asset: objects::PPtr {
                    file_id: 0,
                    path_id: 40,
                },
            },
        ];
        let cn_login = [
            objects::ContainerEntry {
                path: format!("{SHEET_PREFIX}CN/login.wzspritesheet"),
                preload_index: 0,
                preload_size: 0,
                asset: objects::PPtr {
                    file_id: 0,
                    path_id: 50,
                },
            },
            objects::ContainerEntry {
                path: format!("{SHEET_PREFIX}cn/login_0.png"),
                preload_index: 0,
                preload_size: 0,
                asset: objects::PPtr {
                    file_id: 0,
                    path_id: 60,
                },
            },
        ];
        let loc_of = |pid: i64| {
            Some(EntryLocation {
                bundle_index: 1,
                node_index: 2,
                abs_offset: pid as u64,
                byte_size: 3,
                class_id: objects::CLASS_TEXTURE2D,
            })
        };
        let mut index = ImageIndex::default();
        index.collect_sheet(&container, &loc_of).unwrap();
        index.collect_sheet(&cn_login, &loc_of).unwrap();
        let variants = &index.sheets_by_path["login"];
        let cn = localized_sheet(variants, "CN").unwrap();
        assert_eq!(cn.index_loc.as_ref().unwrap().abs_offset, 50);
        assert_eq!(cn.atlas_pages[0].1.abs_offset, 60);
        let sheet = localized_sheet(variants, "TW").unwrap();
        assert_eq!(sheet.atlas_pages[0].1.abs_offset, 10);
        assert_eq!(sheet.index_loc.as_ref().unwrap().abs_offset, 20);
        assert!(!sheet.index_is_asset);
        assert_eq!(sheet.atlas_pages.len(), 1);
        assert_eq!(sheet.atlas_pages[0].0, 0);
        let sheet = &index.sheets_by_path["Character/00002000"]["TW"];
        assert_eq!(sheet.index_loc.as_ref().unwrap().abs_offset, 40);
        assert!(sheet.index_is_asset);
        assert_eq!(sheet.atlas_pages.len(), 1);
    }
}
