//! 樹狀區搜尋：img 層級路徑子字串 ＋ 道具/寵物名稱索引。
//!
//! 名稱索引懶載入 `String/TW/Item`、`String/TW/Pet`，以
//! `commodity::item_path_candidates` 的 id → 路徑規則對到可導航的節點路徑
//! （Item 群組 img 的目標是 img 內的 8 碼 id 節點），建一次跟隨
//! DataService 生命週期。比對時名稱命中排前面，路徑命中在後並去重；
//! `scope` 非空時只回該節點子樹內（或包住該節點的 img）的結果。

use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::catalog::{CatalogError, DataService, Result};

/// 搜尋結果：路徑；名稱命中時帶顯示名稱。
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub path: String,
    pub name: Option<String>,
}

/// 名稱索引一筆：小寫名稱 / id 字串供比對，原始名稱供顯示。
struct NameEntry {
    lower_name: String,
    id: String,
    display: String,
    path: String,
}

/// 名稱索引懶載入快取（跟隨 DataService，更換資料集自動重建）。
#[derive(Default)]
pub(crate) struct SearchIndexCache(Mutex<Option<Arc<Vec<NameEntry>>>>);

impl SearchIndexCache {
    /// 清掉快取（切換語系時呼叫）。
    pub(crate) fn reset(&self) {
        *self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

impl DataService {
    /// 子字串搜尋（不分大小寫）：名稱命中在前、img 路徑命中在後，最多 limit 筆。
    /// scope 非空時只回該節點子樹內（或包住該節點的 img）的結果。
    pub fn search_hits(&self, query: &str, scope: Option<&str>, limit: usize) -> Vec<SearchHit> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let in_scope = |path: &str| match scope {
            None => true,
            // 選到 img 內深層節點時，收斂到包住它的 img。
            Some(s) => {
                path == s
                    || path.starts_with(&format!("{s}/"))
                    || s.starts_with(&format!("{path}/"))
            }
        };

        let mut hits: Vec<SearchHit> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        if let Ok(index) = self.name_index() {
            for entry in index.iter() {
                if hits.len() >= limit {
                    break;
                }
                if !entry.lower_name.contains(&q) && !entry.id.contains(&q) {
                    continue;
                }
                if !in_scope(&entry.path) || !seen.insert(entry.path.clone()) {
                    continue;
                }
                hits.push(SearchHit {
                    path: entry.path.clone(),
                    name: Some(entry.display.clone()),
                });
            }
        }
        for key in self.catalog.entries.keys() {
            if hits.len() >= limit {
                break;
            }
            if !key.to_lowercase().contains(&q) {
                continue;
            }
            if !in_scope(key) || !seen.insert(key.clone()) {
                continue;
            }
            hits.push(SearchHit {
                path: key.clone(),
                name: None,
            });
        }
        hits
    }

    fn name_index(&self) -> Result<Arc<Vec<NameEntry>>> {
        let mut guard = self
            .search_index_cache
            .0
            .lock()
            .map_err(|error| CatalogError::CacheInvalid(error.to_string()))?;
        if let Some(index) = guard.as_ref() {
            return Ok(index.clone());
        }
        let index = Arc::new(self.build_name_index());
        *guard = Some(index.clone());
        Ok(index)
    }

    fn build_name_index(&self) -> Vec<NameEntry> {
        let mut names = crate::commodity::load_names(self, &self.string_path("Item"));
        for (id, name) in crate::commodity::load_names(self, &self.string_path("Pet")) {
            names.entry(id).or_insert(name);
        }
        let mut out: Vec<NameEntry> = names
            .into_iter()
            .filter_map(|(id, display)| {
                let path = crate::commodity::item_path_candidates(id, |p| {
                    self.catalog.entries.contains_key(p)
                })
                .into_iter()
                .find(|p| self.target_exists(p))?;
                Some(NameEntry {
                    lower_name: display.to_lowercase(),
                    id: id.to_string(),
                    display,
                    path,
                })
            })
            .collect();
        out.sort_by(|a, b| {
            a.lower_name
                .cmp(&b.lower_name)
                .then_with(|| a.path.cmp(&b.path))
        });
        out
    }

    /// Character/Pet 的 img 本身即目標；Item 群組的目標是 img 內節點，驗 img 層存在即可。
    fn target_exists(&self, path: &str) -> bool {
        if self.catalog.entries.contains_key(path) {
            return true;
        }
        match path.rsplit_once('/') {
            Some((parent, _)) => self.catalog.entries.contains_key(parent),
            None => false,
        }
    }
}
