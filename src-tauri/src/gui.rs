//! Tauri GUI 入口與 command 註冊。
//!
//! `AppState` 只在切換資料集時取得寫鎖；成功開啟後，各 command 共用
//! `Arc<DataService>`。耗時的索引、解碼與匯出工作都在 blocking worker 執行，
//! 不會長時間持有全域 service lock。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use serde::Serialize;
use tauri::{Manager, State};

use crate::avatar::{self, AvatarFrames, AvatarMeta, EquipItem};
use crate::catalog::{ChildEntry, DataService, NodeDetail, OpenInfo};
use crate::commodity::CommodityItem;
use crate::export::{ExportStats, FileExportResult};
use crate::item_tooltip::ItemTooltip;

#[derive(Clone)]
pub struct AppState {
    service: Arc<RwLock<Option<Arc<DataService>>>>,
    open_gate: Arc<Mutex<()>>,
    cache_dir: PathBuf,
}

#[derive(Serialize)]
struct ImagePreviewResult {
    base64: String,
    mode: &'static str,
    note: Option<String>,
}

#[derive(Serialize)]
struct AudioResult {
    base64: String,
}

impl AppState {
    fn current_or_open(
        &self,
        requested: Option<&Path>,
    ) -> Result<(Arc<DataService>, OpenInfo), String> {
        if requested.is_none() {
            if let Some(service) = self
                .service
                .read()
                .map_err(|error| error.to_string())?
                .as_ref()
                .cloned()
            {
                return Ok((service.clone(), open_info_for(&service)));
            }
            return Err("尚未選擇遊戲資料目錄".into());
        }
        let Some(requested) = requested else {
            return Err("尚未選擇遊戲資料目錄".into());
        };
        let root = resolve_data_root(requested)?;

        // 開檔不持有 service RwLock；gate 只防止多個 command 同時重建相同索引。
        let _gate = self.open_gate.lock().map_err(|error| error.to_string())?;

        if let Some(service) = self.current_matching(&root)? {
            return Ok((service.clone(), open_info_for(&service)));
        }

        let cache_path = self.cache_path_for(&root);
        let (service, info) =
            DataService::open(&root, &cache_path).map_err(|error| error.to_string())?;
        let service = Arc::new(service);
        *self.service.write().map_err(|error| error.to_string())? = Some(service.clone());
        Ok((service, info))
    }

    fn current_matching(&self, requested: &Path) -> Result<Option<Arc<DataService>>, String> {
        let current = self.service.read().map_err(|error| error.to_string())?;
        let Some(service) = current.as_ref() else {
            return Ok(None);
        };

        let normalized =
            std::fs::canonicalize(requested).unwrap_or_else(|_| requested.to_path_buf());
        Ok((service.catalog.root == normalized).then(|| service.clone()))
    }

    fn cache_path_for(&self, root: &Path) -> PathBuf {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for byte in root.to_string_lossy().as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.cache_dir.join(format!("catalog-{hash:016x}.json"))
    }
}

fn resolve_data_root(selected: &Path) -> Result<PathBuf, String> {
    let candidates = [
        selected.to_path_buf(),
        selected.join("w"),
        selected.join("aa").join("w"),
        selected.join("StreamingAssets").join("aa").join("w"),
        selected
            .join("Maplestory_Classic_Data")
            .join("StreamingAssets")
            .join("aa")
            .join("w"),
    ];

    for candidate in candidates {
        if contains_bundle(&candidate) {
            return std::fs::canonicalize(&candidate)
                .map_err(|error| format!("資料目錄 {} 無法開啟: {error}", candidate.display()));
        }
    }

    Err(format!(
        "選取的目錄找不到 *.bundle。請選擇遊戲安裝目錄、Maplestory_Classic_Data，或 StreamingAssets\\aa\\w：{}",
        selected.display()
    ))
}

fn contains_bundle(path: &Path) -> bool {
    std::fs::read_dir(path).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("bundle"))
        })
    })
}

fn open_info_for(service: &DataService) -> OpenInfo {
    OpenInfo {
        from_cache: true,
        elapsed_ms: 0,
        entry_count: service.catalog.entries.len(),
        bundle_count: service.catalog.bundles.len(),
    }
}

async fn blocking<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| format!("背景工作失敗: {error}"))?
}

#[derive(Debug, Serialize)]
pub struct OpenResult {
    pub info: OpenInfo,
    pub top: Vec<ChildEntry>,
    pub data_dir: String,
}

#[tauri::command]
async fn open_data_dir(
    path: Option<String>,
    state: State<'_, AppState>,
) -> Result<OpenResult, String> {
    let state = state.inner().clone();
    blocking(move || {
        let requested = path.as_deref().map(Path::new);
        let (service, info) = state.current_or_open(requested)?;
        let top = service
            .list_children("")
            .map_err(|error| error.to_string())?;
        Ok(OpenResult {
            info,
            top,
            data_dir: service.catalog.root.display().to_string(),
        })
    })
    .await
}

#[tauri::command]
async fn list_children(
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<ChildEntry>, String> {
    let state = state.inner().clone();
    blocking(move || {
        let (service, _) = state.current_or_open(None)?;
        service
            .list_children(&path)
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
async fn get_node(path: String, state: State<'_, AppState>) -> Result<NodeDetail, String> {
    let state = state.inner().clone();
    blocking(move || {
        let (service, _) = state.current_or_open(None)?;
        service.get_node(&path).map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
async fn get_image(
    path: String,
    state: State<'_, AppState>,
) -> Result<Option<ImagePreviewResult>, String> {
    use base64::Engine;

    let state = state.inner().clone();
    blocking(move || {
        let (service, _) = state.current_or_open(None)?;
        match service.get_image(&path) {
            Ok(Some(bytes)) => Ok(Some(ImagePreviewResult {
                base64: base64::engine::general_purpose::STANDARD.encode(bytes),
                mode: "exact",
                note: None,
            })),
            Ok(None) => Ok(None),
            Err(error) => {
                let note = error.to_string();
                match service.get_canvas_atlas_fallback(&path) {
                    Ok(Some(bytes)) => Ok(Some(ImagePreviewResult {
                        base64: base64::engine::general_purpose::STANDARD.encode(bytes),
                        mode: "atlas",
                        note: Some(note),
                    })),
                    Ok(None) => Err(note),
                    Err(fallback_error) => {
                        Err(format!("{note}；atlas fallback 也失敗：{fallback_error}"))
                    }
                }
            }
        }
    })
    .await
}

/// 取聲音：回 Ogg Vorbis 的 base64（與 get_image 同模式）。
#[tauri::command]
async fn get_audio(path: String, state: State<'_, AppState>) -> Result<AudioResult, String> {
    use base64::Engine;

    let state = state.inner().clone();
    blocking(move || {
        let (service, _) = state.current_or_open(None)?;
        let bytes = service
            .get_audio(&path)
            .map_err(|error| error.to_string())?;
        Ok(AudioResult {
            base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        })
    })
    .await
}

#[tauri::command]
async fn get_item_tooltip(
    path: String,
    state: State<'_, AppState>,
) -> Result<Option<ItemTooltip>, String> {
    let state = state.inner().clone();
    blocking(move || {
        let (service, _) = state.current_or_open(None)?;
        service
            .get_item_tooltip(&path)
            .map_err(|error| error.to_string())
    })
    .await
}

/// 商城商品清單（Etc/TW/Commodity）：名稱與 icon 由後端聚合，一次拿完。
#[tauri::command]
async fn get_commodity_items(state: State<'_, AppState>) -> Result<Vec<CommodityItem>, String> {
    let state = state.inner().clone();
    blocking(move || {
        let (service, _) = state.current_or_open(None)?;
        service
            .get_commodity_items()
            .map_err(|error| error.to_string())
    })
    .await
}

/// 紙娃娃基本資料：body 動作清單 + 膚色清單。
#[tauri::command]
async fn get_avatar_meta(state: State<'_, AppState>) -> Result<AvatarMeta, String> {
    let state = state.inner().clone();
    blocking(move || {
        let (service, _) = state.current_or_open(None)?;
        avatar::avatar_meta(&service).map_err(|error| error.to_string())
    })
    .await
}

/// 指定臉型的表情清單。
#[tauri::command]
async fn get_avatar_emotions(
    face_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let state = state.inner().clone();
    blocking(move || {
        let (service, _) = state.current_or_open(None)?;
        avatar::avatar_emotions(&service, &face_id).map_err(|error| error.to_string())
    })
    .await
}

/// 全裝備清單（id / 名稱 / slot / icon 路徑）。
#[tauri::command]
async fn get_avatar_equipment(state: State<'_, AppState>) -> Result<Vec<EquipItem>, String> {
    let state = state.inner().clone();
    blocking(move || {
        let (service, _) = state.current_or_open(None)?;
        avatar::avatar_equipment(&service).map_err(|error| error.to_string())
    })
    .await
}

/// 合成紙娃娃一個動作的幀圖層（身體幀 + 表情幀）。
/// `hair_cover` = 「帽子遮髮」核取方塊（有戴帽才生效）。
#[tauri::command]
async fn get_avatar_frames(
    parts: Vec<avatar::PartInput>,
    action: String,
    emotion: String,
    hair_cover: bool,
    state: State<'_, AppState>,
) -> Result<AvatarFrames, String> {
    let state = state.inner().clone();
    blocking(move || {
        let (service, _) = state.current_or_open(None)?;
        avatar::avatar_frames(&service, parts, &action, &emotion, hair_cover)
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
async fn search_nodes(
    query: String,
    scope: Option<String>,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<crate::search::SearchHit>, String> {
    let state = state.inner().clone();
    blocking(move || {
        let (service, _) = state.current_or_open(None)?;
        Ok(service.search_hits(&query, scope.as_deref(), limit.unwrap_or(200).min(2_000)))
    })
    .await
}

#[tauri::command]
async fn export_subtree(path: String, state: State<'_, AppState>) -> Result<ExportStats, String> {
    let state = state.inner().clone();
    blocking(move || {
        let (service, _) = state.current_or_open(None)?;
        let out_root = state.cache_dir.join("exports");
        service
            .export_subtree(&path, &out_root)
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
async fn export_file(
    path: String,
    format: String,
    output_path: String,
    state: State<'_, AppState>,
) -> Result<FileExportResult, String> {
    let state = state.inner().clone();
    blocking(move || {
        let (service, _) = state.current_or_open(None)?;
        service
            .export_file(&path, Path::new(&output_path), &format)
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
fn initial_path() -> Option<String> {
    std::env::var("MSCLASSIC_INITIAL_PATH")
        .ok()
        .filter(|path| !path.is_empty())
}

/// 自動化測試用：啟動時直接開啟的資料目錄（略過資料夾選擇器）。
#[tauri::command]
fn initial_data_dir() -> Option<String> {
    std::env::var("MSCLASSIC_DATA_DIR")
        .ok()
        .filter(|path| !path.is_empty())
}

/// 前端 canvas 產生的圖檔（base64）寫到使用者從另存對話框選的路徑。
#[tauri::command]
async fn write_binary_file(output_path: String, base64: String) -> Result<(), String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64)
        .map_err(|error| error.to_string())?;
    blocking(move || std::fs::write(&output_path, &bytes).map_err(|error| error.to_string())).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let cache_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("msclassic"));
            app.manage(AppState {
                service: Arc::new(RwLock::new(None)),
                open_gate: Arc::new(Mutex::new(())),
                cache_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_data_dir,
            list_children,
            get_node,
            get_image,
            get_audio,
            get_item_tooltip,
            get_commodity_items,
            get_avatar_meta,
            get_avatar_emotions,
            get_avatar_equipment,
            get_avatar_frames,
            search_nodes,
            export_subtree,
            export_file,
            initial_path,
            initial_data_dir,
            write_binary_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
