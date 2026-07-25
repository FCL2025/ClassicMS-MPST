// 後端 command 的型別與包裝（對齊 src-tauri/src/gui.rs / catalog.rs 的 serde 輸出）。

import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

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

export const openDataDir = (path?: string) => invoke<OpenResult>("open_data_dir", { path });
export const listChildren = (path: string) => invoke<ChildEntry[]>("list_children", { path });
export const getNode = (path: string) => invoke<NodeDetail>("get_node", { path });
export const getImage = (path: string) => invoke<ImagePreviewResult | null>("get_image", { path });
export const getItemTooltip = (path: string) =>
  invoke<ItemTooltip | null>("get_item_tooltip", { path });
export const searchNodes = (query: string, limit = 200) =>
  invoke<string[]>("search_nodes", { query, limit });
export const exportSubtree = (path: string) => invoke<ExportStats>("export_subtree", { path });
export const exportFile = (path: string, format: ExportFormat, outputPath: string) =>
  invoke<FileExportResult>("export_file", { path, format, outputPath });
export const initialPath = () => invoke<string | null>("initial_path");
export const initialDataDir = () => invoke<string | null>("initial_data_dir");

/** web 版的匯入進度（OPFS 複製 / 索引建立）;桌面版沒有,onProgress 為 no-op。 */
export interface ImportProgress {
  phase: "copy" | "catalog";
  percent: number;
  file?: string;
  message: string;
}

/** 桌面版不會有匯入進度,訂閱直接回 no-op 取消函式。 */
export const onProgress = (_listener: (progress: ImportProgress | null) => void): (() => void) =>
  () => {};

/** 集中 Tauri 專屬的資料夾選擇器,讓 web 版可以在 api 層換實作。 */
export const pickDataDirectory = (): Promise<string | null> =>
  open({
    directory: true,
    multiple: false,
    title: "選擇楓之谷經典版遊戲資料夾",
  });

export interface ExportDialogOptions {
  title: string;
  defaultName: string;
  label: string;
  extension: ExportFormat;
}

/** 集中 Tauri 專屬的儲存對話框;web 版改為直接下載。 */
export const pickExportPath = (options: ExportDialogOptions): Promise<string | null> =>
  save({
    title: options.title,
    defaultPath: options.defaultName,
    filters: [
      {
        name: options.label,
        extensions: [options.extension],
      },
    ],
  });

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
