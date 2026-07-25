// 資料夾選擇與路徑解析(複刻 src-tauri/src/gui.rs 的 resolve_data_root)。
//
// 統一使用 <input type="file" webkitdirectory>:本工具的 web 版只把選到的
// File 物件複製進 OPFS,不需要 File System Access 的目錄控制代碼;
// 而 showDirectoryPicker 在 Chromium 有系統目錄封鎖清單(AppData、Downloads、
// 磁碟根目錄等會被原生對話框擋下「包含系統檔案」),webkitdirectory 無此限制,
// 且全瀏覽器行為一致。

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

/** 主入口:回傳 null 表示使用者取消。 */
export function pickFolder(): Promise<PickedFolder | null> {
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
