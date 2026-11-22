//! wasm（瀏覽器）的 JS shim 接線：bundle 隨機讀、catalog 快取、匯出收集。
//!
//! shim 原始檔在 `src-tauri/wasm_shims/msclassic_shims.js`（`/` 開頭的 module
//! 路徑以 msclassic 的 crate root 為基準）；wasm-bindgen 會把它當 local file
//! 複製成 `web-wasm/pkg/snippets/msclassic-*/wasm_shims/msclassic_shims.js`
//! Worker 經 wasm 匯出函式設定檔案與取回匯出結果，兩者共用此模組的狀態。
//! （wasm-bindgen 0.2 不支援 `./` 相對 module 路徑，故用此形式。）
//!
//! - `syncReadAt`：用預先開好的 `FileSystemSyncAccessHandle` 陣列做同步隨機讀。
//! - `cacheRead` / `cacheWrite`：catalog JSON 存 OPFS 單一檔（單寫者，不需鎖）。
//! - `exportFile`：匯出檔案逐個丟給 JS 收集器（由 JS 打包 zip 或觸發下載）。

use std::io;

use wasm_bindgen::prelude::*;

use crate::unity::bundle::RangeRead;

#[wasm_bindgen(module = "/wasm_shims/msclassic_shims.js")]
extern "C" {
    #[wasm_bindgen(js_name = __setSyncHandles)]
    pub fn set_sync_handles(handles: &js_sys::Array, cache: &JsValue);

    #[wasm_bindgen(js_name = __takeExported)]
    pub fn take_exported_files() -> js_sys::Array;

    /// 同步讀 bundle：[offset, offset+length)；JS 端 throw 會轉成 Err。
    #[wasm_bindgen(catch, js_name = syncReadAt)]
    fn sync_read_at(
        file_index: u32,
        offset: f64,
        length: u32,
    ) -> Result<js_sys::Uint8Array, JsValue>;

    /// 讀 catalog 快取 JSON；沒有或讀取失敗時 JS 回 null。
    #[wasm_bindgen(js_name = cacheRead)]
    fn cache_read() -> Option<js_sys::Uint8Array>;

    /// 寫 catalog 快取 JSON；成功回 true（JS 端內部已 catch，不會 throw）。
    #[wasm_bindgen(js_name = cacheWrite)]
    fn cache_write(bytes: &[u8]) -> bool;

    /// 丟一個匯出檔案給 JS 收集器；成功回 true（JS 端內部已 catch）。
    #[wasm_bindgen(js_name = exportFile)]
    fn export_file_js(path: &str, bytes: &[u8]) -> bool;
}

/// 讀 catalog 快取（無快取或失敗都回 None，由呼叫端重建索引）。
pub(crate) fn cache_read_bytes() -> Option<Vec<u8>> {
    cache_read().map(|arr| arr.to_vec())
}

/// 寫 catalog 快取；回傳是否成功（失敗不擋主流程）。
pub(crate) fn cache_write_bytes(bytes: &[u8]) -> bool {
    cache_write(bytes)
}

/// 把匯出檔案丟給 JS 收集器；回傳是否成功。
pub(crate) fn export_file_to_js(path: &str, bytes: &[u8]) -> bool {
    export_file_js(path, bytes)
}

/// 以 JS 同步讀為後端的 [`RangeRead`]：wasm 下開 bundle 的唯一入口。
///
/// `index` 對應 JS manifest 的順序（fileIndex），`len` 為 manifest 回報的檔案大小。
/// 純資料欄位，自動滿足 `Send + Sync`。
pub struct JsRangeRead {
    index: u32,
    len: u64,
}

impl JsRangeRead {
    pub fn new(index: u32, len: u64) -> Self {
        Self { index, len }
    }
}

impl RangeRead for JsRangeRead {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let length = u32::try_from(buf.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "單次讀取超過 u32 上限"))?;
        // offset 以 f64 傳給 JS：bundle 遠小於 2^53，精度足夠。
        let arr = sync_read_at(self.index, offset as f64, length).map_err(|error| {
            io::Error::other(format!(
                "JS syncReadAt 失敗（file {}）: {error:?}",
                self.index
            ))
        })?;
        if arr.length() as usize != buf.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "syncReadAt 回傳 {} bytes，預期 {}（file {} offset {offset}）",
                    arr.length(),
                    buf.len(),
                    self.index
                ),
            ));
        }
        arr.copy_to(buf);
        Ok(())
    }

    fn len(&self) -> u64 {
        self.len
    }
}
