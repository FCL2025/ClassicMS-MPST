// 資料夾選擇與路徑解析(複刻 src-tauri/src/gui.rs 的 resolve_data_root)。
//
// 兩種選擇管道:
// 1. File System Access(showDirectoryPicker,Chromium 限定):拿到可存入
//    IndexedDB 的目錄控制代碼,下次可重新連線不用重選;但 Chromium 有系統目錄
//    封鎖清單(AppData、Downloads、磁碟根目錄等會被原生對話框擋下)。
// 2. <input type="file" webkitdirectory>(全瀏覽器):無封鎖清單,但 File 物件
//    無法跨 session 保存,關閉後只能重選。
// 支援 FSA 時優先走 1,被取消或環境不允許時回退 2。

import { loadStoredFolder, saveStoredFolder } from "./handleStore";

export interface PickedFolder {
  /** 顯示用路徑(選到的根 + 候選子路徑)。 */
  label: string;
  /** 該層目錄下的 *.bundle 檔案(依檔名排序)。 */
  files: File[];
}

// 候選層級,順序與 gui.rs resolve_data_root 一致。
const CANDIDATES = [
  "",
  "w",
  "aa/w",
  "StreamingAssets/aa/w",
  "Maplestory_Classic_Data/StreamingAssets/aa/w",
];

const NO_BUNDLE_MESSAGE =
  "選取的目錄找不到 *.bundle。請選擇遊戲安裝目錄、Maplestory_Classic_Data," +
  "或 StreamingAssets\\aa\\w";

function isBundleName(name: string): boolean {
  return name.toLowerCase().endsWith(".bundle");
}

function sortByName(files: File[]): File[] {
  return files.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
}

/** 主入口:回傳 null 表示使用者取消。支援 FSA 時優先走 showDirectoryPicker。 */
export async function pickFolder(): Promise<PickedFolder | null> {
  if (window.showDirectoryPicker) {
    try {
      const handle = await window.showDirectoryPicker({ id: "msclassic-data", mode: "read" });
      const picked = await fromDirectoryHandle(handle);
      // 記住控制代碼,下次可重新連線;寫入失敗不影響本次開啟。
      void saveStoredFolder(handle, picked.label);
      return picked;
    } catch (error) {
      // 使用者取消(AbortError)就直接回 null;其他錯誤(封鎖目錄、環境不允許)
      // 回退 webkitdirectory,讓被擋的位置仍有機會選。
      if (error instanceof DOMException && error.name === "AbortError") return null;
      if (error instanceof Error && error.message.startsWith(NO_BUNDLE_MESSAGE)) throw error;
    }
  }
  return pickViaInput();
}

/** 重新連線上次用 showDirectoryPicker 選過的資料夾;沒記錄或未獲授權回 null。 */
export async function reconnectStoredFolder(): Promise<PickedFolder | null> {
  const stored = await loadStoredFolder();
  if (!stored) return null;
  const { handle } = stored;
  if ((await handle.queryPermission({ mode: "read" })) !== "granted") {
    // requestPermission 需要使用者手勢,呼叫端需由點擊觸發。
    if ((await handle.requestPermission({ mode: "read" })) !== "granted") return null;
  }
  return fromDirectoryHandle(handle);
}

/** 查詢是否有記住的資料夾,以及權限是否還在(granted 時可免手勢自動重連)。 */
export async function storedFolderState(): Promise<{ label: string; granted: boolean } | null> {
  const stored = await loadStoredFolder();
  if (!stored) return null;
  try {
    const granted = (await stored.handle.queryPermission({ mode: "read" })) === "granted";
    return { label: stored.label, granted };
  } catch {
    return { label: stored.label, granted: false };
  }
}

/** webkitdirectory 管道:全瀏覽器可用,無封鎖清單。 */
function pickViaInput(): Promise<PickedFolder | null> {
  return new Promise((resolve, reject) => {
    const input = document.createElement("input");
    input.type = "file";
    input.webkitdirectory = true;
    input.addEventListener("change", () => {
      try {
        resolve(fromFileList(Array.from(input.files ?? [])));
      } catch (error) {
        reject(error);
      }
    });
    // Chromium 110+ / Safari 16.4+ 有 cancel 事件;舊瀏覽器取消時無事件,只能放著。
    input.addEventListener("cancel", () => resolve(null));
    input.click();
  });
}

/** 依候選層級往下鑽,任一層不存在回 null。 */
async function navigateCandidate(
  root: FileSystemDirectoryHandle,
  candidate: string,
): Promise<FileSystemDirectoryHandle | null> {
  let dir = root;
  for (const segment of candidate.split("/")) {
    if (!segment) continue;
    try {
      dir = await dir.getDirectoryHandle(segment);
    } catch {
      return null;
    }
  }
  return dir;
}

/** File System Access 管道:走同一組候選層級找 *.bundle。 */
async function fromDirectoryHandle(root: FileSystemDirectoryHandle): Promise<PickedFolder> {
  for (const candidate of CANDIDATES) {
    const dir = await navigateCandidate(root, candidate);
    if (!dir) continue;
    const files: File[] = [];
    for await (const entry of dir.values()) {
      if (entry.kind === "file" && isBundleName(entry.name)) {
        files.push(await (entry as FileSystemFileHandle).getFile());
      }
    }
    if (files.length > 0) {
      return {
        label: candidate ? `${root.name}/${candidate}` : root.name,
        files: sortByName(files),
      };
    }
  }
  throw new Error(`${NO_BUNDLE_MESSAGE}:${root.name}`);
}

/** 從 file.webkitRelativePath 判斷層級。 */
function fromFileList(files: File[]): PickedFolder | null {
  if (files.length === 0) return null;
  // webkitRelativePath = "<選到的根>/<...子路徑>/<檔名>"
  const rootName = files[0].webkitRelativePath.split("/")[0] ?? "";
  for (const candidate of CANDIDATES) {
    const matches = files.filter((file) => {
      if (!isBundleName(file.name)) return false;
      const segments = file.webkitRelativePath.split("/");
      const dirPart = segments.slice(1, -1).join("/");
      return dirPart === candidate;
    });
    if (matches.length > 0) {
      return {
        label: candidate ? `${rootName}/${candidate}` : rootName,
        files: sortByName(matches),
      };
    }
  }
  throw new Error(`${NO_BUNDLE_MESSAGE}:${rootName}`);
}
