// 後端 command 的型別與包裝（對齊 src-tauri/src/gui.rs / catalog.rs 的 serde 輸出）。

import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";

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

/** 圖鑑清單中的掉落物參照（對齊 monster.rs 的 serde 輸出）。 */
export interface DropRef {
  itemId: number;
  name: string | null;
}

/** 怪物圖鑑清單項目。 */
export interface MonsterSummary {
  mobId: number;
  name: string | null;
  level: number | null;
  drops: DropRef[];
}

/** 怪物出沒地圖。 */
export interface MapRef {
  mapId: number;
  name: string | null;
}

/** 怪物掉落物（含 icon）。 */
export interface MobDropItem {
  itemId: number;
  name: string | null;
  iconBase64: string | null;
}

/** Mob 動畫的一幀（base64 PNG + 播放資訊）。 */
export interface MobFrame {
  base64: string;
  delay: number;
  origin: [number, number];
  width: number;
  height: number;
}

/** 單一怪物的完整明細。 */
export interface MobDetail {
  mobId: number;
  name: string | null;
  level: number | null;
  category: string | null;
  description: string | null;
  maxHp: number;
  maxMp: number;
  exp: number;
  pad: number;
  pdd: number;
  mad: number;
  mdd: number;
  acc: number;
  eva: number;
  speed: number;
  pushed: number;
  bodyAttack: boolean;
  undead: boolean;
  maps: MapRef[];
  drops: MobDropItem[];
  frames: MobFrame[];
}

/** 技能分類（對齊 skill.rs 的 SkillCategory serde 輸出）。 */
export type SkillCategory = "passive" | "support" | "active";

/** 技能清單項目（對齊 skill.rs 的 serde 輸出）。 */
export interface SkillSummary {
  id: string;
  job: string;
  name: string | null;
  maxLevel: number;
  skillType: number | null;
  category: SkillCategory;
}

/** 單一職業的技能群組。 */
export interface SkillJobGroup {
  job: string;
  skills: SkillSummary[];
}

/** 單一技能完整明細。levels 的葉值可能是數字、字串或 [x,y] 向量。 */
export interface SkillDetail {
  id: string;
  job: string;
  name: string | null;
  desc: string | null;
  h: string | null;
  levelDesc: Record<string, string>;
  maxLevel: number;
  skillType: number | null;
  actions: string[];
  actionDelays: Record<string, number>;
  levels: Record<string, Record<string, number | string | [number, number]>>;
}

export interface ExportStats {
  out_dir: string;
  json_files: number;
  png_files: number;
  nodes: number;
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

export type ExportFormat = "json" | "png" | "xml";

export interface FileExportResult {
  out_path: string;
  nodes: number;
}

export const openDataDir = (path?: string) => invoke<OpenResult>("open_data_dir", { path });
export const listChildren = (path: string) => invoke<ChildEntry[]>("list_children", { path });
export const getNode = (path: string) => invoke<NodeDetail>("get_node", { path });
export const getImage = (path: string) => invoke<ImagePreviewResult | null>("get_image", { path });
export const getAudio = (path: string) => invoke<AudioResult>("get_audio", { path });
export const getItemTooltip = (path: string) =>
  invoke<ItemTooltip | null>("get_item_tooltip", { path });
export const getItemTooltipById = (itemId: number) =>
  invoke<ItemTooltip | null>("get_item_tooltip_by_id", { itemId });
export const getCommodityItems = () => invoke<CommodityItem[]>("get_commodity_items");
export const getMonsterBook = () => invoke<MonsterSummary[]>("get_monster_book");
export const getMobDetail = (mobId: number) =>
  invoke<MobDetail>("get_mob_detail", { mobId });
export const getSkillBook = () => invoke<SkillJobGroup[]>("get_skill_book");
export const getSkillDetail = (job: string, skillId: string) =>
  invoke<SkillDetail>("get_skill_detail", { job, skillId });
export const getAvatarMeta = () => invoke<AvatarMeta>("get_avatar_meta");
export const getAvatarEmotions = (faceId: string) =>
  invoke<string[]>("get_avatar_emotions", { faceId });
export const getAvatarEquipment = () => invoke<EquipItem[]>("get_avatar_equipment");
export const getAvatarFrames = (
  parts: PartInput[],
  action: string,
  emotion: string,
  hairCover: boolean,
  hairShade: boolean,
) => invoke<AvatarFrames>("get_avatar_frames", { parts, action, emotion, hairCover, hairShade });
export interface SearchHit {
  path: string;
  name: string | null;
}

export const searchNodes = (query: string, scope?: string | null, limit = 200) =>
  invoke<SearchHit[]>("search_nodes", { query, scope: scope ?? null, limit });
export const exportSubtree = (path: string) => invoke<ExportStats>("export_subtree", { path });
export const exportFile = (path: string, format: ExportFormat, outputPath: string) =>
  invoke<FileExportResult>("export_file", { path, format, outputPath });
export const initialPath = () => invoke<string | null>("initial_path");
export const initialView = () => invoke<string | null>("initial_view");
export const initialDataDir = () => invoke<string | null>("initial_data_dir");

// web 版「記住資料夾 / 重新連線」專用(api.web.ts 才有實作);桌面版不需要,恆 null。
export const storedFolderLabel = (): Promise<string | null> => Promise.resolve(null);
export const reconnectDataDirectory = (): Promise<string | null> => Promise.resolve(null);

// 資料來源切換(hosted 版 api.hosted.ts 才有雙來源);桌面版固定本機資料夾。
export type DataSource = "cloud" | "local";
export const getDataSource = (): DataSource => "local";
export const setDataSource = (_source: DataSource): void => undefined;
export const supportsDataSourceSwitch = (): boolean => false;

/** 以系統預設瀏覽器開啟外部連結（桌面版走 opener plugin）。 */
export const openExternal = (url: string) => openUrl(url);

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

/**
 * 桌面版：跳出另存對話框，把前端產生的 base64 內容寫到選擇的路徑。
 * 回傳是否真的有存（使用者取消為 false）。web 版在 api.web.ts 改為瀏覽器下載。
 */
export const saveBinaryFile = async (
  defaultName: string,
  label: string,
  extension: string,
  base64: string,
): Promise<boolean> => {
  const outputPath = await save({
    title: "儲存圖片",
    defaultPath: defaultName,
    filters: [{ name: label, extensions: [extension] }],
  });
  if (!outputPath) return false;
  await invoke<void>("write_binary_file", { outputPath, base64 });
  return true;
};

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
