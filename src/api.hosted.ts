// Hosted（靜態資料集）版 api：與 src/api.ts / api.web.ts 完全相同的 export 介面，
// 內部改走 HTTP fetch 預處理資料集（out/hosted/<版本>/，部署後為 R2/Workers）。
// vite.config.hosted.ts 用 alias 把所有 "./api" / "../api" import 換成本檔。
//
// 資料集結構（見 docs/HOSTED_DATA.md）：
//   index.json    版本與統計
//   catalog.json  {"entries": {"<img 邏輯路徑>": class_id}}（114=WZJS img、49=TextAsset、83=AudioClip）
//   tree/<img>.json   每 img 一棵 WZJS 節點樹，canvas 節點內嵌 rect/origin metadata
//   strings/<路徑>.json  TextAsset 明文
//   sheets/<sheet>_<page>.webp  atlas 頁（rect 的 y 是 Unity bottom-up 座標，切圖要換算）

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

export interface SearchHit {
  path: string;
  name: string | null;
}

// ---------------------------------------------------------------------------
// 資料集常數（vite define 注入，見 vite.config.hosted.ts / src/vite-env.d.ts）
// ---------------------------------------------------------------------------

const DATA_ROOT = `${__HOSTED_DATA_BASE__}/${__HOSTED_DATA_VERSION__}`;
const DATA_LABEL = `雲端資料 ${__HOSTED_DATA_VERSION__}`;

const CLASS_WZJS_IMG = 114;
const CLASS_TEXT_ASSET = 49;
const CLASS_AUDIO_CLIP = 83;

// ---------------------------------------------------------------------------
// tree JSON 的節點形狀（pipeline 產物；value 的 JSON 與前端 Value 型別一致）
// ---------------------------------------------------------------------------

interface TreeCanvas {
  sheet: string;
  page: number;
  x: number;
  y: number;
  w: number;
  h: number;
  ox: number;
  oy: number;
}

interface TreeNode {
  name: string;
  tag: number;
  value: Value;
  canvas?: TreeCanvas;
  children?: TreeNode[];
}

// ---------------------------------------------------------------------------
// 記憶體狀態：catalog、目錄 trie、tree / sheet / 切圖快取
// ---------------------------------------------------------------------------

interface CatalogState {
  entries: Record<string, number>;
  root: DirEntry;
}

interface DirEntry {
  /** 此節點本身是 catalog entry 時的 class_id；純目錄為 null。 */
  classId: number | null;
  children: Map<string, DirEntry>;
  /** 底下的 entry 總數（含 TextAsset/AudioClip，對齊 Rust list_index_children）。 */
  imgCount: number;
}

let catalogState: CatalogState | null = null;

/** img 路徑 → tree Promise（fetch 去重；失敗時清掉讓下次重試）。 */
const treeCache = new Map<string, Promise<TreeNode>>();
/** 已成功載入的 tree（搜尋節點名用）。 */
const resolvedTrees = new Map<string, TreeNode>();
/** "<sheet>_<page>" → ImageBitmap（整張 sheet 只抓一次）。 */
const sheetCache = new Map<string, Promise<ImageBitmap>>();
/** 整頁 atlas 的 base64 PNG（atlas fallback 用，避免重複 encode）。 */
const sheetPagePngCache = new Map<string, Promise<string>>();
/** path → 切圖結果（資料集依版本不可變，整個 session 快取）。 */
const imageResultCache = new Map<string, Promise<ImagePreviewResult | null>>();

async function fetchChecked(url: string): Promise<Response> {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`抓取失敗（HTTP ${response.status}）：${url}`);
  }
  return response;
}

function requireCatalog(): CatalogState {
  if (!catalogState) throw new Error("資料集尚未載入");
  return catalogState;
}

/** 由 catalog entries 建目錄 trie；imgCount 計算對齊 Rust list_index_children。 */
function buildDirIndex(entries: Record<string, number>): DirEntry {
  const root: DirEntry = { classId: null, children: new Map(), imgCount: 0 };
  for (const [path, classId] of Object.entries(entries)) {
    const segments = path.split("/");
    let node = root;
    node.imgCount += 1;
    for (const segment of segments) {
      let child = node.children.get(segment);
      if (!child) {
        child = { classId: null, children: new Map(), imgCount: 0 };
        node.children.set(segment, child);
      }
      child.imgCount += 1;
      node = child;
    }
    node.classId = classId;
  }
  return root;
}

function walkDir(root: DirEntry, path: string): DirEntry | null {
  let node = root;
  if (path === "") return node;
  for (const segment of path.split("/")) {
    const child = node.children.get(segment);
    if (!child) return null;
    node = child;
  }
  return node;
}

/** 目錄層的 ChildEntry 清單（對齊 Rust list_index_children：BTreeMap 依路徑排序）。 */
function listDirChildren(state: CatalogState, path: string): ChildEntry[] {
  const dir = walkDir(state.root, path);
  if (!dir) throw new Error(`找不到目錄：${path || "(根目錄)"}`);
  const prefix = path === "" ? "" : `${path}/`;
  return [...dir.children.entries()]
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([name, child]) =>
      child.classId != null
        ? { kind: "Img", name, path: `${prefix}${name}`, class_id: child.classId }
        : { kind: "Category", name, path: `${prefix}${name}`, img_count: child.imgCount },
    );
}

/** 對齊 Rust split_img_prefix：從完整路徑逐層截尾，找最長的 catalog entry 前綴。 */
function splitImgPrefix(
  entries: Record<string, number>,
  path: string,
): [string, string] | null {
  let p = path;
  for (;;) {
    if (Object.prototype.hasOwnProperty.call(entries, p)) {
      const inner = path.slice(p.length).replace(/^\//, "");
      return [p, inner];
    }
    const index = p.lastIndexOf("/");
    if (index === -1) return null;
    p = p.slice(0, index);
  }
}

/** 懶載入並快取一個 img 的 tree JSON（以 img 路徑為鍵，fetch 去重）。 */
function loadTree(imgPath: string): Promise<TreeNode> {
  let cached = treeCache.get(imgPath);
  if (!cached) {
    cached = fetchChecked(`${DATA_ROOT}/tree/${imgPath}.json`)
      .then((response) => response.json() as Promise<TreeNode>)
      .then((tree) => {
        resolvedTrees.set(imgPath, tree);
        return tree;
      });
    cached.catch(() => {
      treeCache.delete(imgPath);
    });
    treeCache.set(imgPath, cached);
  }
  return cached;
}

/** 在 tree 內走訪內部路徑（inner 不含 img 根名稱，從根的 children 開始）。 */
function findNode(root: TreeNode, inner: string): TreeNode | null {
  if (inner === "") return root;
  let node = root;
  for (const segment of inner.split("/")) {
    const child = node.children?.find((c) => c.name === segment);
    if (!child) return null;
    node = child;
  }
  return node;
}

/** 節點名排序：對齊 Rust cmp_node_names（純數字依數值排最前，其餘不分大小寫字母序）。 */
function cmpNodeNames(a: string, b: string): number {
  const numA = /^\d+$/.test(a) ? Number(a) : null;
  const numB = /^\d+$/.test(b) ? Number(b) : null;
  if (numA != null && numB != null) return numA - numB || (a < b ? -1 : a > b ? 1 : 0);
  if (numA != null) return -1;
  if (numB != null) return 1;
  const lowerA = a.toLowerCase();
  const lowerB = b.toLowerCase();
  if (lowerA !== lowerB) return lowerA < lowerB ? -1 : 1;
  return a < b ? -1 : a > b ? 1 : 0;
}

/**
 * UOL 解析：對齊 image/render.rs 的 resolve_uol_target。
 * UOL 相對於該 UOL property 的父節點；source 是 UOL 節點在 img 內的相對路徑
 * （不含 img 根名稱）。解析失敗（跳出根）回 null。
 */
function resolveUolTarget(source: string, uol: string): string | null {
  const segments = source.split("/").filter((part) => part !== "");
  if (segments.length === 0) return null;
  segments.pop();
  for (const part of uol.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      if (segments.length === 0) return null;
      segments.pop();
    } else {
      segments.push(part);
    }
  }
  return segments.join("/");
}

const UOL_RESOLVE_LIMIT = 8;

/** 解析 UOL 鏈（目標本身可能又是 UOL），回最終非 UOL 節點；解析失敗回 null。 */
function resolveUolChain(root: TreeNode, inner: string): TreeNode | null {
  let path = inner;
  for (let i = 0; i < UOL_RESOLVE_LIMIT; i += 1) {
    const node = findNode(root, path);
    if (!node) return null;
    if (node.value.type !== "Uol") return node;
    const next = resolveUolTarget(path, node.value.value);
    if (next == null) return null;
    path = next;
  }
  return null;
}

// ---------------------------------------------------------------------------
// 圖片：sheet 載入 + canvas 切圖
// ---------------------------------------------------------------------------

function loadSheet(sheet: string, page: number): Promise<ImageBitmap> {
  const key = `${sheet}_${page}`;
  let cached = sheetCache.get(key);
  if (!cached) {
    cached = fetchChecked(`${DATA_ROOT}/sheets/${key}.webp`)
      .then((response) => response.blob())
      .then((blob) => createImageBitmap(blob));
    cached.catch(() => {
      sheetCache.delete(key);
    });
    sheetCache.set(key, cached);
  }
  return cached;
}

function canvasToBase64(canvas: HTMLCanvasElement): string {
  const dataUrl = canvas.toDataURL("image/png");
  return dataUrl.slice(dataUrl.indexOf(",") + 1);
}

/** 依 canvas metadata 從 sheet 切出單幀 PNG（rect 的 y 是 bottom-up，要換算）。 */
async function cutSprite(meta: TreeCanvas): Promise<string> {
  const bitmap = await loadSheet(meta.sheet, meta.page);
  // Unity bottom-up → canvas top-down。
  const yTop = bitmap.height - meta.y - meta.h;
  const canvas = document.createElement("canvas");
  canvas.width = meta.w;
  canvas.height = meta.h;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("無法建立 2D canvas context");
  ctx.drawImage(bitmap, meta.x, yTop, meta.w, meta.h, 0, 0, meta.w, meta.h);
  return canvasToBase64(canvas);
}

/** 整頁 atlas 轉 base64 PNG（canvas 節點缺切圖索引時的 fallback）。 */
function sheetPagePng(sheet: string, page: number): Promise<string> {
  const key = `${sheet}_${page}`;
  let cached = sheetPagePngCache.get(key);
  if (!cached) {
    cached = loadSheet(sheet, page).then((bitmap) => {
      const canvas = document.createElement("canvas");
      canvas.width = bitmap.width;
      canvas.height = bitmap.height;
      const ctx = canvas.getContext("2d");
      if (!ctx) throw new Error("無法建立 2D canvas context");
      ctx.drawImage(bitmap, 0, 0);
      return canvasToBase64(canvas);
    });
    cached.catch(() => {
      sheetPagePngCache.delete(key);
    });
    sheetPagePngCache.set(key, cached);
  }
  return cached;
}

async function getImageUncached(path: string): Promise<ImagePreviewResult | null> {
  const state = requireCatalog();
  const split = splitImgPrefix(state.entries, path);
  if (!split) return null;
  const [imgKey, inner] = split;
  if (state.entries[imgKey] !== CLASS_WZJS_IMG) return null;
  const tree = await loadTree(imgKey);

  // img 根：桌面版會輸出整張 atlas 第一頁；hosted 比照（sheet 不存在就回 null）。
  if (inner === "") {
    try {
      const base64 = await sheetPagePng(imgKey, 0);
      return { base64, mode: "atlas", note: "img 根節點，顯示完整 Atlas 頁" };
    } catch {
      return null;
    }
  }

  let node = findNode(tree, inner);
  // UOL 節點：前端自行解析連結（同一 img tree 內），目標是 canvas 就切。
  if (node?.value.type === "Uol") {
    node = resolveUolChain(tree, inner);
  }
  if (!node) return null;
  // 桌面版圖片只屬於 Canvas(18/19) 與 UOL(12)；UOL 已解析，這裡只看 canvas 家族。
  if (node.tag !== 18 && node.tag !== 19) return null;

  if (node.canvas) {
    const base64 = await cutSprite(node.canvas);
    return { base64, mode: "exact", note: null };
  }
  // canvas 節點但 pipeline 沒解出 rect（約 0.06%）：退回整張 atlas 頁。
  try {
    const base64 = await sheetPagePng(imgKey, 0);
    return { base64, mode: "atlas", note: "此節點沒有切圖索引，顯示完整 Atlas 頁" };
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// export 契約：開啟 / 瀏覽
// ---------------------------------------------------------------------------

export const pickDataDirectory = (): Promise<string | null> => Promise.resolve(null);

export const openDataDir = async (_path?: string): Promise<OpenResult> => {
  const started = performance.now();
  const [indexResponse, catalogResponse] = await Promise.all([
    fetchChecked(`${DATA_ROOT}/index.json`),
    fetchChecked(`${DATA_ROOT}/catalog.json`),
  ]);
  const index = (await indexResponse.json()) as { version?: string };
  const catalog = (await catalogResponse.json()) as { entries: Record<string, number> };
  catalogState = {
    entries: catalog.entries,
    root: buildDirIndex(catalog.entries),
  };
  const label = `雲端資料 ${index.version ?? __HOSTED_DATA_VERSION__}`;
  return {
    info: {
      from_cache: true,
      elapsed_ms: Math.round(performance.now() - started),
      entry_count: Object.keys(catalog.entries).length,
      bundle_count: 0,
    },
    top: listDirChildren(catalogState, ""),
    data_dir: label,
  };
};

/** 歡迎頁「重新連線」按鈕用；hosted 模式沒有本機資料夾可記住，恆 null。 */
export const storedFolderLabel = (): Promise<string | null> => Promise.resolve(null);

export const reconnectDataDirectory = (): Promise<string | null> => Promise.resolve(null);

export const listChildren = async (path: string): Promise<ChildEntry[]> => {
  const state = requireCatalog();
  const classId = state.entries[path];
  // TextAsset / AudioClip 是葉子，沒有子節點。
  if (classId === CLASS_TEXT_ASSET || classId === CLASS_AUDIO_CLIP) return [];
  if (classId == null) {
    const split = splitImgPrefix(state.entries, path);
    if (!split) return listDirChildren(state, path);
    // 落在某個 TextAsset/AudioClip 內部：同樣沒有子節點。
    const innerClass = state.entries[split[0]];
    if (innerClass === CLASS_TEXT_ASSET || innerClass === CLASS_AUDIO_CLIP) return [];
    const [imgKey, inner] = split;
    const tree = await loadTree(imgKey);
    const node = findNode(tree, inner);
    if (!node) throw new Error(`找不到節點：${path}`);
    return nodeChildren(state, imgKey, inner, node);
  }
  // WZJS img 根：列出 tree 根節點的 children。
  const tree = await loadTree(path);
  return nodeChildren(state, path, "", tree);
};

function nodeChildren(
  _state: CatalogState,
  imgKey: string,
  inner: string,
  node: TreeNode,
): ChildEntry[] {
  const base = inner === "" ? imgKey : `${imgKey}/${inner}`;
  return [...(node.children ?? [])]
    .sort((a, b) => cmpNodeNames(a.name, b.name))
    .map((child) => ({
      kind: "Node",
      name: child.name,
      path: `${base}/${child.name}`,
      type_tag: child.tag,
      has_children: (child.children?.length ?? 0) > 0,
    }));
}

export const getNode = async (path: string): Promise<NodeDetail> => {
  const state = requireCatalog();
  const split = splitImgPrefix(state.entries, path);
  if (!split) throw new Error(`找不到路徑：${path}`);
  const [imgKey, inner] = split;
  const classId = state.entries[imgKey];
  const name = imgKey.split("/").at(-1) ?? imgKey;

  // TextAsset：fetch strings/<path>.json 明文放 text_asset。
  if (classId === CLASS_TEXT_ASSET) {
    if (inner !== "") throw new Error(`找不到路徑：${path}`);
    const response = await fetchChecked(`${DATA_ROOT}/strings/${imgKey}.json`);
    return {
      path,
      name,
      type_tag: CLASS_TEXT_ASSET,
      value: { type: "SubTree" },
      node_path: null,
      children: [],
      text_asset: await response.text(),
    };
  }
  // AudioClip：只回基本資訊，聲音資料由 getAudio 懶載（M5 才支援）。
  if (classId === CLASS_AUDIO_CLIP) {
    if (inner !== "") throw new Error(`找不到路徑：${path}`);
    return {
      path,
      name,
      type_tag: CLASS_AUDIO_CLIP,
      value: { type: "SubTree" },
      node_path: null,
      children: [],
      text_asset: null,
    };
  }

  const tree = await loadTree(imgKey);
  const node = findNode(tree, inner);
  if (!node) throw new Error(`找不到節點：${path}`);
  return {
    path,
    name: node.name,
    type_tag: node.tag,
    value: node.value,
    node_path: null,
    children: (node.children ?? []).map((child) => [
      child.name,
      child.tag,
      (child.children?.length ?? 0) > 0,
    ]),
    text_asset: null,
  };
};

export const getImage = (path: string): Promise<ImagePreviewResult | null> => {
  let cached = imageResultCache.get(path);
  if (!cached) {
    cached = getImageUncached(path);
    cached.catch(() => {
      imageResultCache.delete(path);
    });
    imageResultCache.set(path, cached);
  }
  return cached;
};

export const getAudio = (): Promise<AudioResult> =>
  Promise.reject(new Error("hosted 模式尚未支援音效（M5）"));

export const searchNodes = async (
  query: string,
  scope?: string | null,
  limit = 200,
): Promise<SearchHit[]> => {
  const state = requireCatalog();
  const needle = query.trim().toLowerCase();
  if (!needle) return [];
  const inScope = (path: string) =>
    !scope || path === scope || path.startsWith(`${scope}/`);

  const hits: SearchHit[] = [];
  // 1. catalog entry（img / TextAsset / AudioClip）的路徑子串比對。
  for (const path of Object.keys(state.entries)) {
    if (hits.length >= limit) return hits;
    if (!inScope(path)) continue;
    if (path.toLowerCase().includes(needle)) hits.push({ path, name: null });
  }
  // 2. 已載入 tree 的節點名比對（不主動抓新 tree，避免搜一次爆量下載）。
  //    inner 路徑不含 img 根名稱（與 findNode / UOL 解析同基準），從根的 children 起算。
  for (const [imgPath, tree] of resolvedTrees) {
    if (hits.length >= limit) return hits;
    if (!inScope(imgPath)) continue;
    const stack: [TreeNode, string][] = (tree.children ?? []).map((child) => [child, ""]);
    while (stack.length > 0 && hits.length < limit) {
      const [node, parentInner] = stack.pop() as [TreeNode, string];
      const inner = parentInner === "" ? node.name : `${parentInner}/${node.name}`;
      if (node.name.toLowerCase().includes(needle)) {
        const path = `${imgPath}/${inner}`;
        if (inScope(path)) hits.push({ path, name: null });
      }
      for (const child of node.children ?? []) stack.push([child, inner]);
    }
  }
  return hits;
};

// ---------------------------------------------------------------------------
// export 契約：功能頁（M3 才接 hosted 資料；先回 null / reject 讓 UI 優雅降級）
// ---------------------------------------------------------------------------

/** NodeDetail 對 tooltip 本來就容忍 null；回 null 比 reject 安靜。 */
export const getItemTooltip = (_path: string): Promise<ItemTooltip | null> =>
  Promise.resolve(null);

export const getItemTooltipById = (_itemId: number): Promise<ItemTooltip | null> =>
  Promise.resolve(null);

export const getCommodityItems = (): Promise<CommodityItem[]> =>
  Promise.reject(new Error("hosted 模式尚未支援商城資料（M3）"));

export const getMonsterBook = (): Promise<MonsterSummary[]> =>
  Promise.reject(new Error("hosted 模式尚未支援怪物圖鑑（M3）"));

export const getMobDetail = (_mobId: number): Promise<MobDetail> =>
  Promise.reject(new Error("hosted 模式尚未支援怪物圖鑑（M3）"));

export const getSkillBook = (): Promise<SkillJobGroup[]> =>
  Promise.reject(new Error("hosted 模式尚未支援技能瀏覽器（M3）"));

export const getSkillDetail = (_job: string, _skillId: string): Promise<SkillDetail> =>
  Promise.reject(new Error("hosted 模式尚未支援技能瀏覽器（M3）"));

export const getAvatarMeta = (): Promise<AvatarMeta> =>
  Promise.reject(new Error("hosted 模式尚未支援紙娃娃（M3）"));

export const getAvatarEmotions = (_faceId: string): Promise<string[]> =>
  Promise.reject(new Error("hosted 模式尚未支援紙娃娃（M3）"));

export const getAvatarEquipment = (): Promise<EquipItem[]> =>
  Promise.reject(new Error("hosted 模式尚未支援紙娃娃（M3）"));

export const getAvatarFrames = (
  _parts: PartInput[],
  _action: string,
  _emotion: string,
  _hairCover: boolean,
  _hairShade: boolean,
): Promise<AvatarFrames> =>
  Promise.reject(new Error("hosted 模式尚未支援紙娃娃（M3）"));

// ---------------------------------------------------------------------------
// export 契約：匯出（hosted 模式不支援；ExportMenu 會用 toast 顯示錯誤）
// ---------------------------------------------------------------------------

const EXPORT_UNSUPPORTED = "hosted 模式尚未支援匯出";

export const exportFile = (
  _path: string,
  _format: ExportFormat,
  _outputPath: string,
): Promise<FileExportResult> => Promise.reject(new Error(EXPORT_UNSUPPORTED));

export const exportSubtree = (_path: string): Promise<ExportStats> =>
  Promise.reject(new Error(EXPORT_UNSUPPORTED));

export const pickExportPath = (
  _options: ExportDialogOptions,
): Promise<string | null> => Promise.reject(new Error(EXPORT_UNSUPPORTED));

export const saveBinaryFile = (
  _defaultName: string,
  _label: string,
  _extension: string,
  _base64: string,
): Promise<boolean> => Promise.reject(new Error(EXPORT_UNSUPPORTED));

// ---------------------------------------------------------------------------
// export 契約：啟動 / 雜項
// ---------------------------------------------------------------------------

export const initialPath = (): Promise<string | null> => Promise.resolve(null);
export const initialView = (): Promise<string | null> => Promise.resolve(null);

/** hosted 模式不用選資料夾：直接回標籤，App 啟動即自動載入。 */
export const initialDataDir = (): Promise<string | null> => Promise.resolve(DATA_LABEL);

/** 與 api.web.ts 相同：直接開新分頁。 */
export const openExternal = (url: string): Promise<void> => {
  window.open(url, "_blank", "noopener,noreferrer");
  return Promise.resolve();
};

/** hosted 模式沒有匯入進度；回傳 no-op 取消函式。 */
export const onProgress = (_listener: (progress: ImportProgress | null) => void): (() => void) =>
  () => undefined;

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
