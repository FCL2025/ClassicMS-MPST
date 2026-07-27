// Web 版 api:與 src/api.ts 完全相同的 export 介面,內部改走 Worker RPC。
// 型別宣告與 api.ts 保持一致(兩邊都是 wasm/gui.rs serde 輸出的契約);
// vite.config.web.ts 用 alias 把所有 "./api" / "../api" import 換成本檔。

import { zipSync } from "fflate";
import { clearProgress, rpc, subscribeProgress } from "./web/client";
import { pickFolder, type PickedFolder } from "./web/pickFolder";

export type ChildEntry =
  | { kind: "Category"; name: string; path: string; img_count: number }
  | { kind: "Img"; name: string; path: string; class_id: number }
  | { kind: "Node"; name: string; path: string; type_tag: number; has_children: boolean };

export interface OpenInfo {
  from_cache: boolean;
  elapsed_ms: number;
  entry_count: number;
  bundle_count: number;
}

export interface OpenResult {
  info: OpenInfo;
  top: ChildEntry[];
  data_dir: string;
}

export type Value =
  | { type: "SubTree" }
  | { type: "Canvas" }
  | { type: "U8"; value: number }
  | { type: "U16"; value: number }
  | { type: "Int"; value: number }
  | { type: "Float2"; value: [number, number] }
  | { type: "Int2"; value: [number, number] }
  | { type: "Double"; value: number }
  | { type: "Vector"; value: [number, number] }
  | { type: "Str"; value: string }
  | { type: "Uol"; value: string };

export interface NodeDetail {
  path: string;
  name: string;
  type_tag: number;
  value: Value;
  node_path?: string | null;
  children: [string, number, boolean][];
  text_asset?: string | null;
}

export interface ImagePreviewResult {
  base64: string;
  mode: "exact" | "atlas";
  note?: string | null;
}

export interface AudioResult {
  base64: string;
}

export interface TooltipStatLine {
  label: string;
  value: string;
}

/** 裝備 tooltip（對齊 item_tooltip.rs 的 serde 輸出）。 */
export interface ItemTooltip {
  itemId: string;
  imgPath: string;
  title: string;
  description: string | null;
  categoryLine: string;
  iconBase64: string | null;
  requiredLevel: number;
  requiredStr: number;
  requiredDex: number;
  requiredInt: number;
  requiredLuk: number;
  requiredJobs: string[];
  attributeLines: string[];
  attackSpeedLabel: string | null;
  scrollCount: number | null;
  statLines: TooltipStatLine[];
}

export interface ExportStats {
  out_dir: string;
  json_files: number;
  png_files: number;
  nodes: number;
}

export type ExportFormat = "json" | "png" | "xml";

export interface FileExportResult {
  out_path: string;
  nodes: number;
}

export interface ImportProgress {
  phase: "copy" | "catalog";
  percent: number;
  file?: string;
  message: string;
}

export interface ExportDialogOptions {
  title: string;
  defaultName: string;
  label: string;
  extension: ExportFormat;
}

/** 商城商品（對齊 commodity.rs 的 serde 輸出）。 */
export interface CommodityItem {
  sn: number;
  itemId: number;
  count: number;
  price: number;
  name: string | null;
  iconBase64: string | null;
}

/** 紙娃娃膚色（對齊 avatar.rs 的 serde 輸出）。 */
export interface SkinInfo {
  bodyId: string;
  headId: string;
  name: string;
}

/** 紙娃娃基本資料：body 動作清單 + 膚色清單。 */
export interface AvatarMeta {
  actions: string[];
  skins: SkinInfo[];
}

/** 紙娃娃裝備項目。 */
export interface EquipItem {
  id: string;
  name: string;
  slot: string;
  iconPath: string;
  cash: boolean;
}

/** 紙娃娃部位輸入（"body"/"head" 必有，其餘為裝備 slot）。 */
export interface PartInput {
  slot: string;
  id: string;
  visible: boolean;
}

/** 紙娃娃幀內的一個圖層（path 用 getImage 取圖，x/y 為 canvas 左上角）。 */
export interface AvatarLayer {
  path: string;
  x: number;
  y: number;
  z: number;
}

/** 紙娃娃動畫幀（layers 已按繪製順序排好，z 遞減）。 */
export interface AvatarFrame {
  delay: number;
  layers: AvatarLayer[];
  /** 身體幀專用：當幀 brow 錨點的絕對位置（表情層以此即時配對）；表情幀恆 null。 */
  faceAnchor: [number, number] | null;
  /** 身體幀專用：當幀是否要畫臉（背面動作不畫）；表情幀恆 true。 */
  showFace: boolean;
}

/** 紙娃娃合成結果：身體動畫幀 + 表情動畫幀。 */
export interface AvatarFrames {
  frames: AvatarFrame[];
  faceFrames: AvatarFrame[];
}

// pickDataDirectory 與 openDataDir 之間的暫存:File 物件無法走字串路徑往返,
// 所以選完資料夾先存在這裡,openDataDir 時餵給 Worker。
let pendingPick: PickedFolder | null = null;

export const pickDataDirectory = async (): Promise<string | null> => {
  const picked = await pickFolder();
  if (!picked) return null;
  pendingPick = picked;
  return picked.label;
};

export const openDataDir = async (_path?: string): Promise<OpenResult> => {
  let picked = pendingPick;
  if (!picked) {
    picked = await pickFolder();
    if (!picked) throw new Error("已取消選擇資料夾");
    pendingPick = picked;
  }
  try {
    const result = await rpc<OpenResult>("importBundles", picked.files);
    return { ...result, data_dir: picked.label };
  } finally {
    clearProgress();
  }
};

export const listChildren = (path: string) => rpc<ChildEntry[]>("listChildren", path);
export const getNode = (path: string) => rpc<NodeDetail>("getNode", path);

interface WasmImageResult {
  png: Uint8Array;
  mode: "exact" | "atlas";
  note: string | null;
}

export const getImage = async (path: string): Promise<ImagePreviewResult | null> => {
  const result = await rpc<WasmImageResult | null>("getImage", path);
  if (!result) return null;
  return { base64: uint8ToBase64(result.png), mode: result.mode, note: result.note };
};

export const getAudio = async (path: string): Promise<AudioResult> => {
  const bytes = await rpc<Uint8Array>("getAudio", path);
  return { base64: uint8ToBase64(bytes) };
};

export const searchNodes = (query: string, limit = 200) =>
  rpc<string[]>("searchNodes", query, limit);

export const getItemTooltip = (path: string) =>
  rpc<ItemTooltip | null>("getItemTooltip", path);

export const getCommodityItems = () => rpc<CommodityItem[]>("getCommodityItems");

export const getAvatarMeta = () => rpc<AvatarMeta>("getAvatarMeta");

export const getAvatarEmotions = (faceId: string) =>
  rpc<string[]>("getAvatarEmotions", faceId);

export const getAvatarEquipment = () => rpc<EquipItem[]>("getAvatarEquipment");

export const getAvatarFrames = (
  parts: PartInput[],
  action: string,
  emotion: string,
  hairCover: boolean,
) => rpc<AvatarFrames>("getAvatarFrames", parts, action, emotion, hairCover);

interface WasmFileExport {
  filename: string;
  bytes: Uint8Array;
}

export const exportFile = async (
  path: string,
  format: ExportFormat,
  _outputPath: string,
): Promise<FileExportResult> => {
  const result = await rpc<WasmFileExport>("exportFile", path, format);
  downloadBytes(result.filename, result.bytes);
  // wasm 單檔匯出沒有回節點數,桌面版才有;web 版以 0 帶過。
  return { out_path: result.filename, nodes: 0 };
};

interface WasmSubtreeExport {
  count: number;
  files: { path: string; bytes: Uint8Array }[];
}

export const exportSubtree = async (path: string): Promise<ExportStats> => {
  const result = await rpc<WasmSubtreeExport>("exportSubtree", path);
  const entries: Record<string, Uint8Array> = {};
  let jsonFiles = 0;
  let pngFiles = 0;
  for (const file of result.files) {
    entries[file.path] = file.bytes;
    if (file.path.toLowerCase().endsWith(".json")) jsonFiles += 1;
    else if (file.path.toLowerCase().endsWith(".png")) pngFiles += 1;
  }
  const zipped = zipSync(entries);
  const rootName = sanitizeFileName(path.split("/").at(-1) ?? "export");
  const zipName = `export-${rootName}.zip`;
  downloadBytes(zipName, zipped, "application/zip");
  return { out_dir: zipName, json_files: jsonFiles, png_files: pngFiles, nodes: result.count };
};

export const initialPath = (): Promise<string | null> => Promise.resolve(null);
export const initialDataDir = (): Promise<string | null> => Promise.resolve(null);

/** web 版直接開新分頁。 */
export const openExternal = (url: string): Promise<void> => {
  window.open(url, "_blank", "noopener,noreferrer");
  return Promise.resolve();
};

/** web 沒有原生儲存對話框,直接回預設檔名(實際下載由 exportFile 觸發)。 */
export const pickExportPath = (options: ExportDialogOptions): Promise<string | null> =>
  Promise.resolve(options.defaultName);

/** 訂閱 OPFS 複製 / 索引建立進度;回傳取消訂閱函式。 */
export const onProgress = (listener: (progress: ImportProgress | null) => void): (() => void) =>
  subscribeProgress(listener);

function sanitizeFileName(value: string): string {
  const sanitized = value.replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_").trim();
  return sanitized || "export";
}

function downloadBytes(filename: string, bytes: Uint8Array, type = "application/octet-stream"): void {
  const blob = new Blob([bytes as BlobPart], { type });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

/** 分塊 btoa,避免大圖 String.fromCharCode(...bytes) 爆 call stack。 */
function uint8ToBase64(bytes: Uint8Array): string {
  const CHUNK = 0x8000;
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + CHUNK));
  }
  return btoa(binary);
}

/** type tag → 顯示標籤（對齊 wzjs/node.rs）。 */
export function typeLabel(tag: number): string {
  switch (tag) {
    case 2: return "SubTree";
    case 3: return "u8";
    case 5: return "u16";
    case 6: return "int";
    case 8: return "float2";
    case 9: return "double";
    case 11: return "string";
    case 12: return "uol";
    case 14: return "vector";
    case 18: case 19: return "canvas";
    case 49: return "TextAsset";
    case 83: return "AudioClip";
    default: return `type${tag}`;
  }
}

export function formatValue(v: Value): string {
  switch (v.type) {
    case "SubTree": return "(子樹)";
    case "Canvas": return "(圖片)";
    case "U8": case "U16": case "Int": return String(v.value);
    case "Double": return String(v.value);
    case "Float2": case "Vector": case "Int2": return `(${v.value[0]}, ${v.value[1]})`;
    case "Str": return v.value;
    case "Uol": return `→ ${v.value}`;
  }
}
