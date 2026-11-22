// msclassic_shims.js — wasm 介面層的 JS 後端，跑在 Dedicated Worker 內。
//
// 本檔是「原始檔」：msclassic（src-tauri/src/wasm_shims.rs）以
// `#[wasm_bindgen(module = "/wasm_shims/msclassic_shims.js")]` import，
// wasm-bindgen 會把它複製成
//   web-wasm/pkg/snippets/msclassic-*/wasm_shims/msclassic_shims.js
// Worker 透過 wasm 設定 OPFS handle 與取回匯出結果，使用當次建置所引用的模組。
//
// 防禦性說明：FileSystemSyncAccessHandle 的同步 read/write/truncate 是
// Baseline（Chrome/Edge 102+、Firefox 111+、Safari 15.2+）；更舊的瀏覽器
// 只有 async 變體，這裡不做 feature-detect，不支援就讓它在呼叫時失敗。

// bundle 的 FileSystemSyncAccessHandle 陣列，index = manifest 順序（fileIndex）。
let syncHandles = [];
// catalog 快取 JSON 的 FileSystemSyncAccessHandle（OPFS 單一檔，單寫者）。
let cacheHandle = null;
// export_subtree / export_file 期間收集的匯出檔案（{ path, bytes }）。
let exported = [];

/**
 * 初始化：注入 bundle 與 catalog 快取的同步 handle。
 * @param {FileSystemSyncAccessHandle[]} handles 與 manifest 同序的 bundle handle
 * @param {FileSystemSyncAccessHandle|null} [cache] catalog 快取檔的 handle
 */
export function __setSyncHandles(handles, cache = null) {
  syncHandles = handles;
  cacheHandle = cache;
}

// bundle 隨機讀：同步讀 [offset, offset+length)。
// 注意 sync handle 的 read 簽名是 read(buffer, { at: offset })，
// 回傳實際讀到的 byte 數；讀不足由 Rust 端判 UnexpectedEof。
export function syncReadAt(fileIndex, offset, length) {
  const handle = syncHandles[fileIndex];
  if (!handle) {
    throw new Error(`syncReadAt: fileIndex ${fileIndex} 尚未初始化（請先 __setSyncHandles）`);
  }
  const buf = new Uint8Array(length);
  // offset 由 wasm 以 f64 傳入；bundle 遠小於 2^53，精度足夠。
  const read = handle.read(buf, { at: offset });
  return read < length ? buf.subarray(0, read) : buf;
}

// 讀 catalog 快取 JSON；沒有 handle、空檔或讀取失敗都回 null（由 wasm 端重建索引）。
export function cacheRead() {
  if (!cacheHandle) return null;
  try {
    const size = cacheHandle.getSize();
    if (size === 0) return null;
    const buf = new Uint8Array(size);
    const read = cacheHandle.read(buf, { at: 0 });
    return read < size ? buf.subarray(0, read) : buf;
  } catch {
    return null;
  }
}

// 寫 catalog 快取 JSON（truncate 後整段覆寫）；失敗回 false，wasm 端不擋主流程。
export function cacheWrite(bytes) {
  if (!cacheHandle) return false;
  try {
    cacheHandle.truncate(0);
    // bytes 是 wasm 線性記憶體的 view；write 同步完成，可直接使用。
    cacheHandle.write(bytes, { at: 0 });
    cacheHandle.flush();
    return true;
  } catch {
    return false;
  }
}

// 收集一個匯出檔案；失敗（理論上不會）回 false 讓 wasm 端報錯。
export function exportFile(path, bytes) {
  try {
    // bytes 是 wasm 線性記憶體的 view，保存前必須 slice() 複製。
    exported.push({ path, bytes: bytes.slice() });
    return true;
  } catch {
    return false;
  }
}

// 取走目前收集到的匯出檔案並清空收集器。
export function __takeExported() {
  const out = exported;
  exported = [];
  return out;
}
