//! BGM → 地圖反查索引：掃 `Map/Map/MapN/*` img 的 `info/bgm`，
//! 補 `String/TW/Map` 的地圖名稱，依 BGM 分組。
//! 音樂播放器頁的「使用此 BGM 的地圖」右欄與地圖名搜尋用。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::catalog::{DataService, Result};
use crate::monster::load_map_names;
use crate::wzjs::Value;

/// BGM 索引快取（只需組裝一次；跟隨 DataService 生命週期，更換資料集自動重建）。
#[derive(Default)]
pub(crate) struct MapBgmCache(pub Mutex<Option<Arc<Vec<BgmMapGroup>>>>);

/// 使用某首 BGM 的一張地圖。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BgmMapRef {
    pub map_id: u32,
    pub name: Option<String>,
    /// 地圖 img 路徑（如 `Map/Map/Map0/000010000`）；前端用來組 miniMap 縮圖路徑。
    pub img_path: String,
}

/// 一首 BGM 與使用它的地圖清單。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BgmMapGroup {
    /// BGM 路徑（`Sound/` 前綴之後的部分，如 `Bgm00/FloralLife`）。
    pub bgm: String,
    pub maps: Vec<BgmMapRef>,
}

impl DataService {
    /// BGM → 地圖索引：遍歷 `Map/Map/MapN/` 下的地圖 img，讀 `info/bgm` 分組；
    /// 名稱取自 `String/TW/Map`（缺名稱回 None）。結果快取。
    pub fn get_map_bgm_index(&self) -> Result<Arc<Vec<BgmMapGroup>>> {
        let mut guard = self
            .map_bgm_cache
            .0
            .lock()
            .map_err(|error| crate::catalog::CatalogError::CacheInvalid(error.to_string()))?;
        if let Some(cached) = guard.as_ref() {
            return Ok(cached.clone());
        }

        let names = load_map_names(self);
        let mut groups: BTreeMap<String, Vec<BgmMapRef>> = BTreeMap::new();
        for path in self.catalog.entries.keys() {
            // 地圖 img 路徑固定為 `Map/Map/Map<數字>/<9 碼數字 id>`。
            let Some(rest) = path.strip_prefix("Map/Map/Map") else {
                continue;
            };
            let Some((dir, file)) = rest.split_once('/') else {
                continue;
            };
            if dir.is_empty()
                || !dir.bytes().all(|b| b.is_ascii_digit())
                || file.len() != 9
                || !file.bytes().all(|b| b.is_ascii_digit())
            {
                continue;
            }
            let Ok(map_id) = file.parse::<u32>() else {
                continue;
            };
            let Ok(tree) = self.tree_for(path) else {
                continue;
            };
            let Some(bgm) = tree.find("info/bgm").and_then(|node| match &node.value {
                Value::Str(text) if !text.is_empty() => Some(text.clone()),
                _ => None,
            }) else {
                continue;
            };
            groups.entry(bgm).or_default().push(BgmMapRef {
                map_id,
                name: names.get(&map_id).cloned(),
                img_path: path.clone(),
            });
        }

        let index: Vec<BgmMapGroup> = groups
            .into_iter()
            .map(|(bgm, mut maps)| {
                maps.sort_by_key(|map| map.map_id);
                BgmMapGroup { bgm, maps }
            })
            .collect();
        let shared = Arc::new(index);
        *guard = Some(shared.clone());
        Ok(shared)
    }
}
