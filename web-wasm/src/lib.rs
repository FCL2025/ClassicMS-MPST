//! msclassic 核心的 wasm 介面層（Step 2）。
//!
//! 在瀏覽器 Dedicated Worker 內執行，匯出與 Tauri command 對應的函式
//! （語意對齊 `src-tauri/src/gui.rs` 的 serde 輸出與 `src/api.ts` 的 TS 型別）：
//!
//! - 結構化資料（open / list_children / get_node / search_nodes）走
//!   `serde_json` → `JSON.parse`，保證與桌面版 Tauri 輸出形狀完全一致。
//! - 二進位（get_image / export_file）直接回 `Uint8Array`，不做 base64。
//!
//! 檔案 I/O 全部由 Worker 端 `msclassic_shims.js` 提供
//! （OPFS 同步讀、catalog 快取、匯出收集器），見 msclassic 的 `wasm_shims` 模組。

use std::cell::RefCell;
use std::path::Path;

use js_sys::{Object, Reflect, Uint8Array};
use msclassic::catalog::{DataService, WasmBundleEntry, WASM_VIRTUAL_ROOT};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

thread_local! {
    /// wasm 單執行緒，thread_local 不要求 Send/Sync；`open` 重入時丟掉舊的重建。
    static SERVICE: RefCell<Option<DataService>> = const { RefCell::new(None) };
}

fn js_err(message: impl Into<String>) -> JsValue {
    JsValue::from_str(&message.into())
}

/// 以已開啟的 DataService 執行操作；錯誤訊息字串統一在這層轉 JsValue。
fn with_service<T>(f: impl FnOnce(&DataService) -> Result<T, String>) -> Result<T, JsValue> {
    SERVICE
        .with(|cell| {
            let borrow = cell.borrow();
            let service = borrow
                .as_ref()
                .ok_or_else(|| "尚未開啟資料集，請先呼叫 open".to_string())?;
            f(service)
        })
        .map_err(js_err)
}

/// 結構化回傳：serde_json 序列化再 JSON.parse，與桌面 Tauri command 輸出同形狀。
fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    let json = serde_json::to_string(value).map_err(|e| js_err(format!("序列化失敗: {e}")))?;
    js_sys::JSON::parse(&json).map_err(|e| js_err(format!("JSON.parse 失敗: {e:?}")))
}

fn set_field(obj: &Object, key: &str, value: &JsValue) -> Result<(), JsValue> {
    Reflect::set(obj, &JsValue::from_str(key), value).map(|_| ())
}

/// JS manifest 的一筆 bundle 描述：`{ name, size, lastModified }`。
/// 數字用 f64 收（JS number），再轉 u64（bundle 遠小於 2^53，精度足夠）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestEntryJs {
    name: String,
    size: f64,
    last_modified: f64,
}

/// 去掉 `.bundle` 副檔名（不分大小寫），與桌面 BundleMeta.name（file stem）一致。
fn strip_bundle_ext(name: &str) -> String {
    const EXT: &str = ".bundle";
    if name.len() > EXT.len() && name[name.len() - EXT.len()..].eq_ignore_ascii_case(EXT) {
        name[..name.len() - EXT.len()].to_string()
    } else {
        name.to_string()
    }
}

fn parse_manifest(manifest: &JsValue) -> Result<Vec<WasmBundleEntry>, JsValue> {
    let json = js_sys::JSON::stringify(manifest)
        .ok()
        .and_then(|s| s.as_string())
        .ok_or_else(|| js_err("manifest 無法序列化"))?;
    let entries: Vec<ManifestEntryJs> =
        serde_json::from_str(&json).map_err(|e| js_err(format!("manifest 格式錯誤: {e}")))?;
    if entries.is_empty() {
        return Err(js_err("manifest 沒有任何 bundle"));
    }
    entries
        .into_iter()
        .map(|entry| {
            if !entry.size.is_finite() || entry.size < 0.0 {
                return Err(js_err(format!("manifest 的 size 無效: {}", entry.name)));
            }
            Ok(WasmBundleEntry {
                name: strip_bundle_ext(&entry.name),
                size: entry.size as u64,
                mtime_ms: entry.last_modified.max(0.0) as u64,
            })
        })
        .collect()
}

/// 對齊 gui.rs 的 `OpenResult`：`{ info, top, data_dir }`。
#[derive(Serialize)]
struct OpenResultWeb {
    info: msclassic::catalog::OpenInfo,
    top: Vec<msclassic::catalog::ChildEntry>,
    data_dir: String,
}

/// 開啟資料集。`manifest` 是 JS 物件陣列 `[{ name, size, lastModified }]`，
/// 順序固定即 fileIndex（需與 `__setSyncHandles` 的 handle 陣列同序）。
/// 重複呼叫會丟掉舊的 DataService 重建。
#[wasm_bindgen]
pub fn open(manifest: JsValue) -> Result<JsValue, JsValue> {
    let entries = parse_manifest(&manifest)?;
    let (service, info) = DataService::open_wasm(&entries).map_err(|e| js_err(e.to_string()))?;
    let top = service
        .list_children("")
        .map_err(|e| js_err(e.to_string()))?;
    let result = OpenResultWeb {
        info,
        top,
        data_dir: WASM_VIRTUAL_ROOT.to_string(),
    };
    SERVICE.with(|cell| *cell.borrow_mut() = Some(service));
    to_js(&result)
}

#[wasm_bindgen]
pub fn list_children(path: &str) -> Result<JsValue, JsValue> {
    with_service(|s| s.list_children(path).map_err(|e| e.to_string())).and_then(|v| to_js(&v))
}

#[wasm_bindgen]
pub fn get_node(path: &str) -> Result<JsValue, JsValue> {
    with_service(|s| s.get_node(path).map_err(|e| e.to_string())).and_then(|v| to_js(&v))
}

#[wasm_bindgen]
pub fn search_nodes(query: &str, limit: Option<u32>) -> Result<JsValue, JsValue> {
    let limit = limit.map_or(200, |l| l as usize).min(2_000);
    with_service(|s| Ok(s.search(query, limit))).and_then(|v| to_js(&v))
}

/// 裝備 tooltip：回 `ItemTooltip`（camelCase）或 `null`（非裝備路徑）。
#[wasm_bindgen]
pub fn get_item_tooltip(path: &str) -> Result<JsValue, JsValue> {
    with_service(|s| s.get_item_tooltip(path).map_err(|e| e.to_string())).and_then(|v| to_js(&v))
}

/// 取圖：回 `{ png: Uint8Array, mode: "exact" | "atlas", note: string | null }`，
/// 查無圖片回 `null`；保留 gui.rs 的 atlas fallback 串接邏輯（不轉 base64）。
#[wasm_bindgen]
pub fn get_image(path: &str) -> Result<JsValue, JsValue> {
    with_service(|s| match s.get_image(path) {
        Ok(Some(png)) => image_result(&png, "exact", None).map_err(|e| format!("{e:?}")),
        Ok(None) => Ok(JsValue::NULL),
        Err(error) => {
            let note = error.to_string();
            match s.get_canvas_atlas_fallback(path) {
                Ok(Some(png)) => {
                    image_result(&png, "atlas", Some(&note)).map_err(|e| format!("{e:?}"))
                }
                Ok(None) => Err(note),
                Err(fallback) => Err(format!("{note}；atlas fallback 也失敗：{fallback}")),
            }
        }
    })
}

fn image_result(png: &[u8], mode: &str, note: Option<&str>) -> Result<JsValue, JsValue> {
    let obj = Object::new();
    set_field(&obj, "png", &Uint8Array::from(png))?;
    set_field(&obj, "mode", &JsValue::from_str(mode))?;
    set_field(&obj, "note", &note.map_or(JsValue::NULL, JsValue::from_str))?;
    Ok(obj.into())
}

/// 取聲音：回 Ogg Vorbis bytes（Uint8Array），由前端轉 base64 / blob 播放。
#[wasm_bindgen]
pub fn get_audio(path: &str) -> Result<Uint8Array, JsValue> {
    let bytes = with_service(|s| s.get_audio(path).map_err(|e| e.to_string()))?;
    Ok(Uint8Array::from(&bytes[..]))
}

/// 單檔匯出：回 `{ filename: string, bytes: Uint8Array }` 讓前端觸發下載。
/// 內容由 export.rs 產生；wasm 下經 `export_file_to_memory` 攔截到記憶體，
/// 不走 JS 收集器（收集器留給 export_subtree 的多檔情境）。
#[wasm_bindgen]
pub fn export_file(path: &str, format: &str) -> Result<JsValue, JsValue> {
    if !matches!(format.to_ascii_lowercase().as_str(), "json" | "png" | "xml") {
        return Err(js_err(format!("不支援的匯出格式: {format}")));
    }
    let (filename, bytes) = with_service(|s| {
        s.export_file_to_memory(path, format)
            .map_err(|e| e.to_string())
    })?;
    let obj = Object::new();
    set_field(&obj, "filename", &JsValue::from_str(&filename))?;
    set_field(&obj, "bytes", &Uint8Array::from(&bytes[..]))?;
    Ok(obj.into())
}

/// 子樹匯出：檔案逐個經 JS 的 exportFile callback 進收集器
/// （JS 端用 `__takeExported()` 取走打包 zip），回傳匯出檔案數。
#[wasm_bindgen]
pub fn export_subtree(path: &str) -> Result<u32, JsValue> {
    let stats = with_service(|s| {
        s.export_subtree(path, Path::new(""))
            .map_err(|e| e.to_string())
    })?;
    u32::try_from(stats.json_files + stats.png_files).map_err(|_| js_err("匯出檔案數超過 u32 上限"))
}
