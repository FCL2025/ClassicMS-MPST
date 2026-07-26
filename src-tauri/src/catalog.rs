//! 資產索引與懶解碼服務（Phase 3 後端核心，不依賴 GUI）。
//!
//! 兩層設計：
//! - [`Catalog`]：bundle → 邏輯路徑 → 物件位置 的索引，可序列化快取到磁碟，
//!   二次啟動驗證 bundle size/mtime 後直接載入（秒開）。
//! - [`DataService`]：開啟的 bundle 控制代碼 + 已解碼 WZJS 樹的 LRU 快取，
//!   提供 list_children / get_node / search 給前端（tauri command 只是薄包裝）。
//!
//! 路徑協定（對前端公開的邏輯路徑）：
//! - container 路徑 `Assets/WzAssets/Json/Character/00002000.wzjson`
//!   → 索引鍵 `Character/00002000`（去前綴、去副檔名）
//! - 樹上前幾段是「目錄」（由索引鍵分組產生），命中索引鍵的那節點是「img」，
//!   img 之後的段落由 WZJS 樹導航（如 `Character/00020116/angry/0/face`）。

use std::collections::{BTreeMap, HashMap, VecDeque};
#[cfg(not(target_arch = "wasm32"))]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::image::{ImageIndex, SerializedImageContext};
use crate::unity::bundle::Bundle;
use crate::unity::objects;
use crate::unity::serialized_file::{metadata_len, SerializedFile, HEADER_SIZE};
use crate::wzjs::{self, Value, WzNode};

mod cache;

#[cfg(target_arch = "wasm32")]
use crate::wasm_shims::JsRangeRead;
use cache::{bundle_files, modified_nanos};
#[cfg(not(target_arch = "wasm32"))]
use cache::{
    bundle_manifest, cache_backup_path, quarantine_cache, read_cache_with_backup, CacheFileLock,
};

/// wasm 下 Catalog 的虛擬 root（無檔案系統目錄，僅作為快取驗證的識別用）。
#[cfg(target_arch = "wasm32")]
pub const WASM_VIRTUAL_ROOT: &str = "opfs://bundles";

/// container 路徑要剝掉的前綴（不分大小寫）。
const JSON_PREFIX: &str = "assets/wzassets/json/";
/// 聲音資產（AudioClip）的 container 前綴與副檔名。
/// （副檔名只是名目——內容一律是 FSB5 封裝的 Vorbis。）
const SOUND_PREFIX: &str = "assets/wzdata/sound/";
const SOUND_SUFFIXES: [&str; 2] = [".mp3", ".wav"];
/// 目前索引格式版本（Phase 4 加入圖片索引 → 2；加入聲音索引 → 4；
/// spriteset 無 count 前綴的 PPtr 連排佈局 → 5）。
const CATALOG_VERSION: u32 = 5;
/// 索引只收這兩種副檔名的資產。
const WZJSON_SUFFIX: &str = ".wzjson";
const JSON_SUFFIX: &str = ".json";
/// 已解碼 WZJS 樹的快取上限（img 數）。
const TREE_CACHE_CAP: usize = 32;
const IMAGE_CACHE_CAP: usize = 64;
const IMAGE_CACHE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Error, Debug)]
pub enum CatalogError {
    #[error("Unity 容器錯誤: {0}")]
    Unity(#[from] crate::unity::UnityError),

    #[error("WZJS 解碼錯誤: {0}")]
    Wzjs(#[from] wzjs::WzjsError),

    #[error("I/O 錯誤: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON 序列化錯誤: {0}")]
    Json(#[from] serde_json::Error),

    #[error("路徑不存在或無法解析: {0}")]
    PathNotFound(String),

    #[error("索引快取無效: {0}")]
    CacheInvalid(String),

    #[error("音訊轉換錯誤: {0}")]
    Audio(#[from] crate::audio::AudioError),
}

pub type Result<T> = std::result::Result<T, CatalogError>;

// ── Catalog（可序列化的索引）────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryLocation {
    pub bundle_index: u32,
    pub node_index: u32,
    /// 物件資料在 node 內的絕對 offset（已含 dataOffset）。
    pub abs_offset: u64,
    pub byte_size: u32,
    /// 資產種類：114 = MonoBehaviour(WZJS)、49 = TextAsset(JSON 明文)、83 = AudioClip(FSB5)。
    pub class_id: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleMeta {
    pub name: String,
    pub size: u64,
    pub mtime_nanos: u128,
}

/// wasm：JS manifest 中一個 bundle 的描述（順序固定即 fileIndex）。
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
pub struct WasmBundleEntry {
    /// bundle 檔名（去副檔名，與桌面 [`BundleMeta::name`] 一致）。
    pub name: String,
    pub size: u64,
    /// `File.lastModified`（ms 精度）。
    pub mtime_ms: u64,
}

/// manifest → 快取驗證用的 [`BundleMeta`]；ms 轉 nanos（*1_000_000）存入既有欄位。
#[cfg(target_arch = "wasm32")]
fn manifest_metas(manifest: &[WasmBundleEntry]) -> Vec<BundleMeta> {
    manifest
        .iter()
        .map(|entry| BundleMeta {
            name: entry.name.clone(),
            size: entry.size,
            mtime_nanos: u128::from(entry.mtime_ms).saturating_mul(1_000_000),
        })
        .collect()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Catalog {
    /// 索引格式版本：結構變更時 +1，舊快取直接作廢重建。
    #[serde(default)]
    pub version: u32,
    pub root: PathBuf,
    pub bundles: Vec<BundleMeta>,
    /// 邏輯路徑（如 `Character/00002000`）→ 物件位置。
    pub entries: BTreeMap<String, EntryLocation>,
    /// 圖片索引（spriteset / spritesheet，Phase 4）。
    #[serde(default)]
    pub image: ImageIndex,
}

impl Catalog {
    /// 掃描目錄下所有 .bundle，用每個 AssetBundle 物件的 m_Container 建索引。
    pub fn build(root: &Path) -> Result<Catalog> {
        let mut files = bundle_files(root)?;
        files.sort();

        let mut metas = Vec::with_capacity(files.len());
        for path in &files {
            let meta = std::fs::metadata(path)?;
            let mtime_nanos = modified_nanos(&meta);
            let name = path
                .file_stem()
                .ok_or_else(|| {
                    CatalogError::CacheInvalid(format!("bundle 無檔名: {}", path.display()))
                })?
                .to_string_lossy()
                .into_owned();
            metas.push(BundleMeta {
                name,
                size: meta.len(),
                mtime_nanos,
            });
        }
        Self::build_inner(root.to_path_buf(), metas, |i| {
            Bundle::open(&files[i]).map_err(CatalogError::from)
        })
    }

    /// wasm 入口：以 JS 傳入的 manifest（順序固定即 fileIndex）建索引，
    /// 每個 bundle 用 [`JsRangeRead`] 經 JS shim 的 OPFS 同步讀開啟。
    #[cfg(target_arch = "wasm32")]
    pub fn build_manifest(manifest: &[WasmBundleEntry]) -> Result<Catalog> {
        let metas = manifest_metas(manifest);
        let sizes: Vec<u64> = metas.iter().map(|meta| meta.size).collect();
        Self::build_inner(PathBuf::from(WASM_VIRTUAL_ROOT), metas, move |i| {
            Bundle::open_reader(Box::new(JsRangeRead::new(i as u32, sizes[i])))
                .map_err(CatalogError::from)
        })
    }

    /// 建索引的共用流程；cfg 差異只在「列出 bundle」與「開單一 bundle 的 reader」，
    /// 由呼叫端準備好 `metas`（順序 = bundle_index）與 `open_bundle`。
    fn build_inner(
        root: PathBuf,
        metas: Vec<BundleMeta>,
        open_bundle: impl Fn(usize) -> Result<Bundle>,
    ) -> Result<Catalog> {
        let mut entries = BTreeMap::new();
        let mut image = ImageIndex::default();
        for (i, meta) in metas.iter().enumerate() {
            let name = meta.name.clone();
            let bundle_index = i as u32;

            let bundle = open_bundle(i)?;
            for ni in 0..bundle.nodes.len() {
                if !bundle.nodes[ni].is_serialized_file() {
                    continue;
                }
                let head48 = bundle.read_node_range(ni, 0, HEADER_SIZE)?;
                let meta_len = match metadata_len(&head48) {
                    Ok(l) => l,
                    Err(_) => continue,
                };
                let head = bundle.read_node_range(ni, 0, meta_len)?;
                let sf = match SerializedFile::parse(&head) {
                    Ok(sf) => sf,
                    Err(_) => continue,
                };
                // path_id → object 位置
                let by_path_id: HashMap<i64, &crate::unity::serialized_file::ObjectInfo> =
                    sf.objects.iter().map(|o| (o.path_id, o)).collect();
                for o in sf.objects_by_class(objects::CLASS_ASSET_BUNDLE) {
                    let relative_offset = u64::try_from(o.byte_start).map_err(|_| {
                        CatalogError::CacheInvalid(format!(
                            "AssetBundle 物件 offset 為負數: bundle={name}, node={ni}"
                        ))
                    })?;
                    let object_offset =
                        sf.data_offset.checked_add(relative_offset).ok_or_else(|| {
                            CatalogError::CacheInvalid(format!(
                                "AssetBundle 物件 offset 溢位: bundle={name}, node={ni}"
                            ))
                        })?;
                    let data = bundle.read_node_range(ni, object_offset, o.byte_size as usize)?;
                    let ab = objects::parse_asset_bundle(&data, &sf)?;
                    image.collect_from_container(SerializedImageContext {
                        bundle: &bundle,
                        bundle_index,
                        stem: &name,
                        node_index: ni,
                        serialized_file: &sf,
                        container: &ab.container,
                        by_path_id: &by_path_id,
                    })?;
                    for entry in &ab.container {
                        let Some(key) = normalize_container_path(&entry.path) else {
                            continue;
                        };
                        if entry.asset.file_id != 0 {
                            continue; // 外部參照不在此 bundle，略過
                        }
                        if let Some(obj) = by_path_id.get(&entry.asset.path_id) {
                            let Ok(relative_offset) = u64::try_from(obj.byte_start) else {
                                continue;
                            };
                            let Some(abs_offset) = sf.data_offset.checked_add(relative_offset)
                            else {
                                continue;
                            };
                            entries.insert(
                                key,
                                EntryLocation {
                                    bundle_index,
                                    node_index: ni as u32,
                                    abs_offset,
                                    byte_size: obj.byte_size,
                                    class_id: obj.class_id,
                                },
                            );
                        }
                    }
                }
            }
        }
        Ok(Catalog {
            version: CATALOG_VERSION,
            root,
            bundles: metas,
            entries,
            image,
        })
    }

    /// 序列化存檔。
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)?;
        let _cache_lock = CacheFileLock::acquire(path)?;
        let json = serde_json::to_string(self)?;
        let file_name = path.file_name().and_then(|s| s.to_str()).ok_or_else(|| {
            CatalogError::CacheInvalid(format!("快取路徑無效: {}", path.display()))
        })?;
        let temp = parent.join(format!(".{file_name}.{}.tmp", std::process::id()));
        let backup = parent.join(format!(".{file_name}.bak"));
        let _ = std::fs::remove_file(&temp);

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        if let Err(error) = (|| -> std::io::Result<()> {
            file.write_all(json.as_bytes())?;
            file.sync_all()?;
            drop(file);

            if path.exists() {
                let _ = std::fs::remove_file(&backup);
                std::fs::rename(path, &backup)?;
            }
            if let Err(error) = std::fs::rename(&temp, path) {
                if backup.exists() {
                    let _ = std::fs::rename(&backup, path);
                }
                return Err(error);
            }
            let _ = std::fs::remove_file(&backup);
            Ok(())
        })() {
            let _ = std::fs::remove_file(&temp);
            return Err(CatalogError::Io(error));
        }
        Ok(())
    }

    /// wasm：catalog JSON 經 JS shim 寫進 OPFS 單一檔（單寫者，不需鎖/備份/隔離）。
    /// 寫入失敗（例如 OPFS 配額不足）不擋開啟流程，下次重建索引即可。
    #[cfg(target_arch = "wasm32")]
    pub fn save(&self, _path: &Path) -> Result<()> {
        let json = serde_json::to_string(self)?;
        let _ = crate::wasm_shims::cache_write_bytes(json.as_bytes());
        Ok(())
    }

    /// 載入快取；bundle 檔案 size/mtime 對不上就回 None（呼叫端重建）。
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(path: &Path, root: &Path) -> Result<Option<Catalog>> {
        let json = match read_cache_with_backup(path) {
            Ok(j) => j,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let catalog: Catalog = match serde_json::from_str(&json) {
            Ok(catalog) => catalog,
            Err(_) => {
                quarantine_cache(path);
                let backup = cache_backup_path(path);
                let recovered = std::fs::read_to_string(&backup)
                    .ok()
                    .and_then(|backup_json| serde_json::from_str(&backup_json).ok());
                if let Some(catalog) = recovered {
                    let _ = std::fs::rename(backup, path);
                    catalog
                } else {
                    return Ok(None);
                }
            }
        };
        if catalog.root != root || catalog.version != CATALOG_VERSION {
            return Ok(None);
        }
        if catalog.bundles != bundle_manifest(root)? {
            return Ok(None);
        }
        Ok(Some(catalog))
    }

    /// wasm 的桌面簽名版 `load`：無目錄可比對，一律視為 miss。
    /// 瀏覽器端請用 [`Catalog::load_wasm`]（以 manifest 驗證）。
    #[cfg(target_arch = "wasm32")]
    pub fn load(_path: &Path, _root: &Path) -> Result<Option<Catalog>> {
        Ok(None)
    }

    /// wasm：從 JS shim 讀 OPFS 上的 catalog 快取，
    /// 以 manifest 的 size + mtime（ms 精度）驗證；對不上或損毀就回 None（重建）。
    #[cfg(target_arch = "wasm32")]
    pub fn load_wasm(manifest: &[WasmBundleEntry]) -> Result<Option<Catalog>> {
        let Some(bytes) = crate::wasm_shims::cache_read_bytes() else {
            return Ok(None);
        };
        let catalog: Catalog = match serde_json::from_slice(&bytes) {
            Ok(catalog) => catalog,
            Err(_) => return Ok(None), // 損毀直接重建，不需隔離那套
        };
        if catalog.root != Path::new(WASM_VIRTUAL_ROOT) || catalog.version != CATALOG_VERSION {
            return Ok(None);
        }
        if catalog.bundles != manifest_metas(manifest) {
            return Ok(None);
        }
        Ok(Some(catalog))
    }

    /// 頂層（或某目錄節點下）的下一層：(名稱, 直接 img 數, 是否 img 本身)。
    pub fn list_index_children(&self, prefix: &str) -> Vec<IndexChild> {
        let mut out: BTreeMap<String, IndexChild> = BTreeMap::new();
        let pfx = if prefix.is_empty() {
            String::new()
        } else {
            format!("{prefix}/")
        };
        for key in self.entries.keys() {
            let Some(rest) = key.strip_prefix(&pfx[..]) else {
                continue;
            };
            let seg = rest.split('/').next().unwrap();
            let child_path = format!("{pfx}{seg}");
            let e = out.entry(child_path.clone()).or_insert_with(|| IndexChild {
                name: seg.to_string(),
                path: child_path,
                is_img: false,
                img_count: 0,
            });
            if !rest.contains('/') {
                e.is_img = true;
            }
            e.img_count += 1;
        }
        out.into_values().collect()
    }

    /// 子字串搜尋（不分大小寫），最多 limit 筆。
    pub fn search(&self, query: &str, limit: usize) -> Vec<String> {
        let q = query.to_lowercase();
        self.entries
            .keys()
            .filter(|k| k.to_lowercase().contains(&q))
            .take(limit)
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexChild {
    pub name: String,
    pub path: String,
    pub is_img: bool,
    pub img_count: usize,
}

/// 節點名比較：純數字名依數值排最前（0~9），其餘不分大小寫依字母（A~Z）。
fn cmp_node_names(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (numeric_node_name(a), numeric_node_name(b)) {
        (Some(x), Some(y)) => x.cmp(&y).then_with(|| a.cmp(b)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a
            .to_lowercase()
            .cmp(&b.to_lowercase())
            .then_with(|| a.cmp(b)),
    }
}

/// 純數字節點名轉成數值；非純數字回 None。
fn numeric_node_name(name: &str) -> Option<u64> {
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    name.parse().ok()
}

/// 把 container 路徑正規化成索引鍵；不屬於 json / 聲音資產的回 None。
fn normalize_container_path(path: &str) -> Option<String> {
    let lower = path.to_lowercase();
    if let Some(stripped) = lower
        .strip_prefix(JSON_PREFIX)
        .map(|_| &path[JSON_PREFIX.len()..])
    {
        for suffix in [WZJSON_SUFFIX, JSON_SUFFIX] {
            if lower.ends_with(suffix) {
                return Some(stripped[..stripped.len() - suffix.len()].to_string());
            }
        }
        return None;
    }
    // 聲音：`Assets/WzData/Sound/Bgm00/DragonDream.mp3` → `Sound/Bgm00/DragonDream`
    if lower.starts_with(SOUND_PREFIX) {
        for suffix in SOUND_SUFFIXES {
            if lower.ends_with(suffix) {
                let stripped = &path[SOUND_PREFIX.len()..];
                return Some(format!(
                    "Sound/{}",
                    &stripped[..stripped.len() - suffix.len()]
                ));
            }
        }
    }
    None
}

// ── DataService（開啟狀態 + 懶解碼）─────────────────────────────

pub struct DataService {
    pub catalog: Catalog,
    pub(crate) bundles: Vec<Bundle>,
    tree_cache: Mutex<TreeCache>,
    image_cache: Mutex<ImageCache>,
    pub(crate) tooltip_name_cache: crate::item_tooltip::TooltipNameCache,
    pub(crate) commodity_name_cache: crate::commodity::CommodityNameCache,
}

#[derive(Default)]
struct TreeCache {
    trees: HashMap<String, Arc<WzNode>>,
    lru: VecDeque<String>,
}

#[derive(Default)]
struct ImageCache {
    images: HashMap<String, Arc<Vec<u8>>>,
    lru: VecDeque<String>,
    bytes: usize,
}

/// 開啟結果的回報資訊（給前端顯示與驗收計時）。
#[derive(Debug, Serialize)]
pub struct OpenInfo {
    pub from_cache: bool,
    pub elapsed_ms: u128,
    pub entry_count: usize,
    pub bundle_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum ChildEntry {
    /// 索引目錄節點。
    Category {
        name: String,
        path: String,
        img_count: usize,
    },
    /// img 節點（一個 WZJS/TextAsset 文件）。
    Img {
        name: String,
        path: String,
        class_id: i32,
    },
    /// WZJS 樹內節點。
    Node {
        name: String,
        path: String,
        type_tag: i32,
        has_children: bool,
    },
}

#[derive(Debug, Serialize)]
pub struct NodeDetail {
    pub path: String,
    pub name: String,
    pub type_tag: i32,
    pub value: Value,
    pub node_path: Option<String>,
    /// （名稱, type tag, 是否有子節點）
    pub children: Vec<(String, i32, bool)>,
    /// TextAsset（class 49）的 JSON 明文；WZJS 節點為 None。
    pub text_asset: Option<String>,
}

impl DataService {
    /// 開啟資料目錄：先試快取，失敗或失效就重建索引並寫入快取。
    /// `cache_path` 為索引快取檔位置（GUI 用 app data dir，測試自訂）。
    pub fn open(root: &Path, cache_path: &Path) -> Result<(DataService, OpenInfo)> {
        let t0 = web_time::Instant::now();
        let (catalog, from_cache) = match Catalog::load(cache_path, root)? {
            Some(c) => (c, true),
            None => {
                let c = Catalog::build(root)?;
                c.save(cache_path)?;
                (c, false)
            }
        };
        let mut bundles = Vec::with_capacity(catalog.bundles.len());
        for meta in &catalog.bundles {
            bundles.push(Bundle::open(
                &catalog.root.join(format!("{}.bundle", meta.name)),
            )?);
        }
        let info = OpenInfo {
            from_cache,
            elapsed_ms: t0.elapsed().as_millis(),
            entry_count: catalog.entries.len(),
            bundle_count: catalog.bundles.len(),
        };
        let service = DataService {
            catalog,
            bundles,
            tree_cache: Mutex::new(TreeCache::default()),
            image_cache: Mutex::new(ImageCache::default()),
            tooltip_name_cache: Default::default(),
            commodity_name_cache: Default::default(),
        };
        service.validate_locations()?;
        Ok((service, info))
    }

    /// wasm 入口：以 JS manifest 開啟資料集。先試 OPFS 上的快取，
    /// 失效就以 [`JsRangeRead`] 重建索引並回寫快取；bundle 全部經 JS 同步讀開啟。
    #[cfg(target_arch = "wasm32")]
    pub fn open_wasm(manifest: &[WasmBundleEntry]) -> Result<(DataService, OpenInfo)> {
        let t0 = web_time::Instant::now();
        let (catalog, from_cache) = match Catalog::load_wasm(manifest)? {
            Some(c) => (c, true),
            None => {
                let c = Catalog::build_manifest(manifest)?;
                c.save(Path::new(WASM_VIRTUAL_ROOT))?;
                (c, false)
            }
        };
        let mut bundles = Vec::with_capacity(catalog.bundles.len());
        for (i, meta) in catalog.bundles.iter().enumerate() {
            bundles.push(Bundle::open_reader(Box::new(JsRangeRead::new(
                i as u32, meta.size,
            )))?);
        }
        let info = OpenInfo {
            from_cache,
            elapsed_ms: t0.elapsed().as_millis(),
            entry_count: catalog.entries.len(),
            bundle_count: catalog.bundles.len(),
        };
        let service = DataService {
            catalog,
            bundles,
            tree_cache: Mutex::new(TreeCache::default()),
            image_cache: Mutex::new(ImageCache::default()),
            tooltip_name_cache: Default::default(),
            commodity_name_cache: Default::default(),
        };
        service.validate_locations()?;
        Ok((service, info))
    }

    /// 列出 path 的直接子節點（"" = 頂層目錄）。
    pub fn list_children(&self, path: &str) -> Result<Vec<ChildEntry>> {
        // 先當索引目錄查：path 不是 img 也沒命中任何 img 前綴 → 目錄層
        if !self.catalog.entries.contains_key(path) {
            let split = split_img_prefix(&self.catalog, path);
            if split.is_none() {
                return Ok(self
                    .catalog
                    .list_index_children(path)
                    .into_iter()
                    .map(|c| {
                        if c.is_img {
                            let class_id = self
                                .catalog
                                .entries
                                .get(&c.path)
                                .map_or(0, |location| location.class_id);
                            ChildEntry::Img {
                                name: c.name,
                                path: c.path,
                                class_id,
                            }
                        } else {
                            ChildEntry::Category {
                                name: c.name,
                                path: c.path,
                                img_count: c.img_count,
                            }
                        }
                    })
                    .collect());
            }
        }
        // img 或 img 內部
        let (img_key, inner) = split_img_prefix(&self.catalog, path)
            .ok_or_else(|| CatalogError::PathNotFound(path.to_string()))?;
        if self.catalog.entries.get(&img_key).is_some_and(|l| {
            l.class_id == objects::CLASS_TEXT_ASSET || l.class_id == objects::CLASS_AUDIO_CLIP
        }) {
            return Ok(vec![]); // TextAsset / AudioClip 沒有子節點
        }
        let root = self.tree_for(&img_key)?;
        let node = if inner.is_empty() {
            root.as_ref()
        } else {
            root.find(&inner)
                .ok_or_else(|| CatalogError::PathNotFound(path.to_string()))?
        };
        let base = if inner.is_empty() {
            img_key
        } else {
            format!("{img_key}/{inner}")
        };
        // 節點名依 0~9 → A~Z 排序（純數字名依數值排最前，其餘不分大小寫依字母）。
        let mut children: Vec<&WzNode> = node.children.iter().collect();
        children.sort_by(|a, b| cmp_node_names(&a.name, &b.name));
        Ok(children
            .into_iter()
            .map(|c| ChildEntry::Node {
                name: c.name.clone(),
                path: format!("{base}/{}", c.name),
                type_tag: c.type_tag,
                has_children: !c.children.is_empty(),
            })
            .collect())
    }

    /// 取單一節點的詳細內容（右欄顯示用）。
    pub fn get_node(&self, path: &str) -> Result<NodeDetail> {
        let (img_key, inner) = split_img_prefix(&self.catalog, path)
            .ok_or_else(|| CatalogError::PathNotFound(path.to_string()))?;
        let loc = self
            .catalog
            .entries
            .get(&img_key)
            .ok_or_else(|| CatalogError::PathNotFound(img_key.clone()))?
            .clone();
        // TextAsset（小 json bundle）：直接回傳 JSON 明文
        if loc.class_id == objects::CLASS_TEXT_ASSET {
            if !inner.is_empty() {
                return Err(CatalogError::PathNotFound(path.to_string()));
            }
            self.validate_location(&loc)?;
            let bundle = self
                .bundles
                .get(loc.bundle_index as usize)
                .ok_or_else(|| CatalogError::CacheInvalid("bundle index 越界".into()))?;
            let data = bundle.read_node_range(
                loc.node_index as usize,
                loc.abs_offset,
                loc.byte_size as usize,
            )?;
            let text_asset = objects::parse_text_asset(&data, true)?;
            return Ok(NodeDetail {
                path: path.to_string(),
                name: text_asset.name,
                type_tag: objects::CLASS_TEXT_ASSET,
                value: Value::SubTree,
                node_path: None,
                children: vec![],
                text_asset: Some(String::from_utf8_lossy(&text_asset.script).into_owned()),
            });
        }
        // AudioClip（聲音）：只回基本資訊，聲音資料由 get_audio 懶載
        if loc.class_id == objects::CLASS_AUDIO_CLIP {
            if !inner.is_empty() {
                return Err(CatalogError::PathNotFound(path.to_string()));
            }
            let clip = self.read_audio_clip(&loc)?;
            return Ok(NodeDetail {
                path: path.to_string(),
                name: clip.name,
                type_tag: objects::CLASS_AUDIO_CLIP,
                value: Value::SubTree,
                node_path: None,
                children: vec![],
                text_asset: None,
            });
        }
        let root = self.tree_for(&img_key)?;
        let node = if inner.is_empty() {
            root.as_ref()
        } else {
            root.find(&inner)
                .ok_or_else(|| CatalogError::PathNotFound(path.to_string()))?
        };
        Ok(NodeDetail {
            path: path.to_string(),
            name: node.name.clone(),
            type_tag: node.type_tag,
            value: node.value.clone(),
            node_path: node.path.clone(),
            children: node
                .children
                .iter()
                .map(|c| (c.name.clone(), c.type_tag, !c.children.is_empty()))
                .collect(),
            text_asset: None,
        })
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<String> {
        self.catalog.search(query, limit)
    }

    /// 讀 AudioClip 物件本身（不含聲音資料）。
    fn read_audio_clip(&self, loc: &EntryLocation) -> Result<objects::AudioClipInfo> {
        self.validate_location(loc)?;
        let bundle = self
            .bundles
            .get(loc.bundle_index as usize)
            .ok_or_else(|| CatalogError::CacheInvalid("bundle index 越界".into()))?;
        let data = bundle.read_node_range(
            loc.node_index as usize,
            loc.abs_offset,
            loc.byte_size as usize,
        )?;
        Ok(objects::parse_audio_clip(&data, true)?)
    }

    /// 取聲音：AudioClip → FSB5 → 轉封裝成 Ogg Vorbis bytes。
    pub fn get_audio(&self, path: &str) -> Result<Vec<u8>> {
        let (img_key, inner) = split_img_prefix(&self.catalog, path)
            .ok_or_else(|| CatalogError::PathNotFound(path.to_string()))?;
        if !inner.is_empty() {
            return Err(CatalogError::PathNotFound(path.to_string()));
        }
        let loc = self
            .catalog
            .entries
            .get(&img_key)
            .ok_or_else(|| CatalogError::PathNotFound(img_key.clone()))?
            .clone();
        if loc.class_id != objects::CLASS_AUDIO_CLIP {
            return Err(CatalogError::PathNotFound(format!(
                "{img_key} 不是 AudioClip（class {}）",
                loc.class_id
            )));
        }
        let clip = self.read_audio_clip(&loc)?;
        let bundle = self
            .bundles
            .get(loc.bundle_index as usize)
            .ok_or_else(|| CatalogError::CacheInvalid("bundle index 越界".into()))?;
        let node = bundle.node(&clip.resource_node).ok_or_else(|| {
            CatalogError::CacheInvalid(format!("找不到 resource node: {}", clip.resource_node))
        })?;
        let fsb =
            bundle.read_node_range(node, clip.resource_offset, clip.resource_size as usize)?;
        Ok(crate::audio::fsb_to_ogg(&fsb)?)
    }

    pub(crate) fn cached_image(&self, path: &str) -> Result<Option<Arc<Vec<u8>>>> {
        let mut cache = self
            .image_cache
            .lock()
            .map_err(|error| CatalogError::CacheInvalid(error.to_string()))?;
        let Some(image) = cache.images.get(path).cloned() else {
            return Ok(None);
        };
        if let Some(position) = cache.lru.iter().position(|key| key == path) {
            cache.lru.remove(position);
        }
        cache.lru.push_back(path.to_string());
        Ok(Some(image))
    }

    pub(crate) fn cache_image(&self, path: &str, image: Vec<u8>) -> Result<Arc<Vec<u8>>> {
        let image = Arc::new(image);
        if image.len() > IMAGE_CACHE_BYTES {
            return Ok(image);
        }
        let mut cache = self
            .image_cache
            .lock()
            .map_err(|error| CatalogError::CacheInvalid(error.to_string()))?;
        while !cache.lru.is_empty()
            && (cache.lru.len() >= IMAGE_CACHE_CAP
                || cache.bytes.saturating_add(image.len()) > IMAGE_CACHE_BYTES)
        {
            if let Some(old) = cache.lru.pop_front() {
                if let Some(removed) = cache.images.remove(&old) {
                    cache.bytes = cache.bytes.saturating_sub(removed.len());
                }
            }
        }
        if let Some(previous) = cache.images.insert(path.to_string(), image.clone()) {
            cache.bytes = cache.bytes.saturating_sub(previous.len());
            if let Some(position) = cache.lru.iter().position(|key| key == path) {
                cache.lru.remove(position);
            }
        }
        cache.bytes += image.len();
        cache.lru.push_back(path.to_string());
        Ok(image)
    }

    fn validate_locations(&self) -> Result<()> {
        for (path, location) in &self.catalog.entries {
            self.validate_location(location).map_err(|error| {
                CatalogError::CacheInvalid(format!("資產 {path} 的快取位置無效: {error}"))
            })?;
        }
        for (path, sheet) in &self.catalog.image.sheets {
            if let Some(location) = &sheet.index_loc {
                self.validate_location(location).map_err(|error| {
                    CatalogError::CacheInvalid(format!(
                        "spritesheet {path} 的索引位置無效: {error}"
                    ))
                })?;
            }
            for (page, location) in &sheet.atlas_pages {
                self.validate_location(location).map_err(|error| {
                    CatalogError::CacheInvalid(format!(
                        "spritesheet {path} page {page} 的位置無效: {error}"
                    ))
                })?;
            }
        }
        for (asset_path, sprites) in &self.catalog.image.set_assets {
            for (sprite_name, sprite) in sprites {
                self.validate_location(&sprite.sprite_loc)
                    .and_then(|_| self.validate_location(&sprite.texture_loc))
                    .map_err(|error| {
                        CatalogError::CacheInvalid(format!(
                            "spriteset {asset_path}/{sprite_name} 的位置無效: {error}"
                        ))
                    })?;
            }
        }
        Ok(())
    }

    pub(crate) fn validate_location(&self, location: &EntryLocation) -> Result<()> {
        let bundle = self
            .bundles
            .get(location.bundle_index as usize)
            .ok_or_else(|| {
                CatalogError::CacheInvalid(format!(
                    "bundle index {} 越界（共 {}）",
                    location.bundle_index,
                    self.bundles.len()
                ))
            })?;
        let node = bundle
            .nodes
            .get(location.node_index as usize)
            .ok_or_else(|| {
                CatalogError::CacheInvalid(format!(
                    "node index {} 越界（共 {}）",
                    location.node_index,
                    bundle.nodes.len()
                ))
            })?;
        let node_size = u64::try_from(node.size).map_err(|_| {
            CatalogError::CacheInvalid(format!("node {} size 為負: {}", node.name, node.size))
        })?;
        let end = location
            .abs_offset
            .checked_add(u64::from(location.byte_size))
            .ok_or_else(|| CatalogError::CacheInvalid("物件位置加總溢位".into()))?;
        if end > node_size {
            return Err(CatalogError::CacheInvalid(format!(
                "物件範圍 {}..{} 超出 node {} bytes",
                location.abs_offset, end, node_size
            )));
        }
        Ok(())
    }

    /// 解碼（或從快取取）一個 img 的 WZJS 樹。
    pub fn tree_for(&self, img_key: &str) -> Result<Arc<WzNode>> {
        {
            let mut cache = self
                .tree_cache
                .lock()
                .map_err(|error| CatalogError::CacheInvalid(error.to_string()))?;
            if let Some(tree) = cache.trees.get(img_key).cloned() {
                if let Some(position) = cache.lru.iter().position(|key| key == img_key) {
                    cache.lru.remove(position);
                }
                cache.lru.push_back(img_key.to_string());
                return Ok(tree);
            }
        }
        let loc = self
            .catalog
            .entries
            .get(img_key)
            .ok_or_else(|| CatalogError::PathNotFound(img_key.to_string()))?
            .clone();
        if loc.class_id != objects::CLASS_MONO_BEHAVIOUR {
            return Err(CatalogError::PathNotFound(format!(
                "{img_key} 不是 WZJS（class {}），無法展開",
                loc.class_id
            )));
        }
        self.validate_location(&loc)?;
        let bundle = self
            .bundles
            .get(loc.bundle_index as usize)
            .ok_or_else(|| CatalogError::CacheInvalid("bundle index 越界".into()))?;
        let payload = bundle.read_node_range(
            loc.node_index as usize,
            loc.abs_offset,
            loc.byte_size as usize,
        )?;
        let mono = objects::parse_mono_behaviour(&payload, true)?;
        let tree = wzjs::parse(&payload[mono.header_len..])?;
        let tree = Arc::new(tree);
        let mut cache = self
            .tree_cache
            .lock()
            .map_err(|error| CatalogError::CacheInvalid(error.to_string()))?;
        if let Some(existing) = cache.trees.get(img_key).cloned() {
            return Ok(existing);
        }
        if cache.lru.len() >= TREE_CACHE_CAP {
            if let Some(old) = cache.lru.pop_front() {
                cache.trees.remove(&old);
            }
        }
        cache.lru.push_back(img_key.to_string());
        cache.trees.insert(img_key.to_string(), tree.clone());
        Ok(tree)
    }
}

/// 把 path 拆成 (img_key, img 內部路徑)；找不到任何 img 前綴回 None。
pub(crate) fn split_img_prefix(catalog: &Catalog, path: &str) -> Option<(String, String)> {
    let mut p = path;
    loop {
        if catalog.entries.contains_key(p) {
            let inner = &path[p.len()..];
            return Some((
                p.to_string(),
                inner.strip_prefix('/').unwrap_or(inner).to_string(),
            ));
        }
        match p.rfind('/') {
            Some(i) => p = &p[..i],
            None => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "msclassic-catalog-{name}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn empty_catalog(root: &Path) -> Catalog {
        Catalog {
            version: CATALOG_VERSION,
            root: root.to_path_buf(),
            bundles: Vec::new(),
            entries: BTreeMap::new(),
            image: ImageIndex::default(),
        }
    }

    #[test]
    fn cache_roundtrip_and_new_bundle_invalidation() {
        let root = temp_dir("manifest");
        let cache = root.join("catalog.json");
        empty_catalog(&root).save(&cache).unwrap();
        assert!(Catalog::load(&cache, &root).unwrap().is_some());

        std::fs::write(root.join("new.bundle"), b"not parsed during manifest check").unwrap();
        assert!(Catalog::load(&cache, &root).unwrap().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupt_cache_is_quarantined_and_rebuilt_as_miss() {
        let root = temp_dir("corrupt");
        let cache = root.join("catalog.json");
        std::fs::write(&cache, "{broken json").unwrap();
        assert!(Catalog::load(&cache, &root).unwrap().is_none());
        assert!(!cache.exists());
        assert!(std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().contains(".corrupt.")));
        std::fs::remove_dir_all(root).unwrap();
    }
}
