// 裝備 tooltip 聚合（hosted 版）：語意對齊 src-tauri/src/item_tooltip.rs，
// id → 路徑映射對齊 commodity.rs 的 item_path_candidates。
// 差異：Rust 只從本機 catalog 查；hosted 版同樣以 catalog 判斷存在性，
// tree 透過 HTTP 懶載入（loadTree 內建快取與去重）。

import type { ItemTooltip, TooltipStatLine, Value } from "../api.hosted";
import {
  findNode,
  getImage,
  loadTextAsset,
  loadTree,
  requireCatalog,
  splitImgPrefix,
} from "./core";
import type { TreeNode } from "./core";

// ---------------------------------------------------------------------------
// 對齊 item_tooltip.rs 的常數表
// ---------------------------------------------------------------------------

/** 對齊 MPST 的 GEAR_STAT_LABELS（順序即顯示順序）。 */
const GEAR_STAT_LABELS: [string, string][] = [
  ["incSTR", "STR"],
  ["incDEX", "DEX"],
  ["incINT", "INT"],
  ["incLUK", "LUK"],
  ["incMHP", "MaxHP"],
  ["incMMP", "MaxMP"],
  ["incPAD", "攻擊力"],
  ["incMAD", "魔法攻擊力"],
  ["incPDD", "物理防禦力"],
  ["incMDD", "魔法防禦力"],
  ["incACC", "命中值"],
  ["incEVA", "迴避值"],
  ["incSpeed", "移動速度"],
  ["incJump", "跳躍力"],
  ["tuc", "可升級次數"],
];

const ALL_JOB_LABELS = ["初心者", "劍士", "法師", "弓箭手", "盜賊", "海盜"];
const JOB_MASK_LABELS: [number, string][] = [
  [1, "劍士"],
  [2, "法師"],
  [4, "弓箭手"],
  [8, "盜賊"],
  [16, "海盜"],
];

// ---------------------------------------------------------------------------
// 節點屬性讀取（對齊 item_tooltip.rs 的 prop / prop_i64 / value_as_i64）
// ---------------------------------------------------------------------------

/** 直接子節點查找（不分大小寫，對齊 Rust prop）。 */
function prop(node: TreeNode, name: string): TreeNode | null {
  const lower = name.toLowerCase();
  return node.children?.find((child) => child.name.toLowerCase() === lower) ?? null;
}

function valueAsI64(value: Value): number | null {
  switch (value.type) {
    case "U8":
    case "U16":
    case "Int":
    case "Long":
      return value.value;
    case "Int2":
      return value.value[0];
    case "Double":
      return Math.trunc(value.value);
    default:
      return null;
  }
}

function propI64(node: TreeNode, name: string): number | null {
  const child = prop(node, name);
  return child ? valueAsI64(child.value) : null;
}

function propBool(node: TreeNode, name: string): boolean {
  const value = propI64(node, name);
  return value != null && value !== 0;
}

// ---------------------------------------------------------------------------
// String/TW/Item 名稱表（整份快取；monster.ts 也會用）
// ---------------------------------------------------------------------------

export interface ItemNameEntry {
  name: string;
  desc: string | null;
}

let itemNameTablePromise: Promise<Map<number, ItemNameEntry>> | null = null;

/** 懶載入 `String/TW/Item` 名稱表；資料集沒有此資產時視為空表（對齊 Rust）。 */
export function loadItemNameTable(): Promise<Map<number, ItemNameEntry>> {
  if (!itemNameTablePromise) {
    itemNameTablePromise = loadTextAsset("String/TW/Item")
      .then((text) => {
        const table = new Map<number, ItemNameEntry>();
        const raw = JSON.parse(text) as Record<string, { name?: unknown; desc?: unknown }>;
        for (const [key, value] of Object.entries(raw)) {
          const id = Number(key);
          if (!Number.isInteger(id) || typeof value?.name !== "string") continue;
          table.set(id, {
            name: value.name,
            desc: typeof value.desc === "string" ? value.desc : null,
          });
        }
        return table;
      })
      .catch(() => new Map<number, ItemNameEntry>());
  }
  return itemNameTablePromise;
}

// ---------------------------------------------------------------------------
// 道具 id → 候選路徑（對齊 commodity.rs 的 item_path_candidates）
// ---------------------------------------------------------------------------

/** 依道具 id 推出可能的（img 或 img 內節點）路徑，按優先順序。 */
export function itemPathCandidates(itemId: number, hasPath: (path: string) => boolean): string[] {
  const bases: string[] = [];
  const id8 = String(itemId).padStart(8, "0");
  const id7 = String(itemId).padStart(7, "0");
  const dir4 = String(Math.floor(itemId / 10000)).padStart(4, "0");
  switch (Math.floor(itemId / 1_000_000)) {
    // 裝備：Character/<分類目錄>/<8 碼 id>。
    case 1: {
      const prefix = Math.floor(itemId / 10000);
      const dir =
        prefix === 100
          ? "Cap"
          : prefix >= 101 && prefix <= 103
            ? "Accessory"
            : prefix === 104
              ? "Coat"
              : prefix === 105
                ? "Longcoat"
                : prefix === 106
                  ? "Pants"
                  : prefix === 107
                    ? "Shoes"
                    : prefix === 108
                      ? "Glove"
                      : prefix === 109
                        ? "Shield"
                        : prefix === 110
                          ? "Cape"
                          : prefix === 111
                            ? "Ring"
                            : prefix >= 112 && prefix <= 115
                              ? "Accessory"
                              : prefix >= 121 && prefix <= 179
                                ? "Weapon"
                                : prefix === 180
                                  ? "PetEquip"
                                  : null;
      if (dir) bases.push(`Character/${dir}/${id8}`);
      break;
    }
    case 2:
      bases.push(`Item/Consume/${dir4}/${id8}`);
      break;
    case 3:
      bases.push(`Item/Install/${dir4}/${id8}`);
      break;
    case 4:
      bases.push(`Item/Etc/${dir4}/${id8}`);
      break;
    // 5 開頭：寵物是一 id 一 img，其餘落在 Cash 分區 img。
    case 5: {
      const pet = `Item/Pet/${itemId}`;
      if (hasPath(pet)) bases.push(pet);
      bases.push(`Item/Cash/${dir4}/${id8}`);
      break;
    }
    // 9 開頭（套裝組、欄位擴充等）：Special img 內的節點是 7 碼 id。
    case 9:
      bases.push(`Item/Special/0${Math.floor(itemId / 10000)}/${id7}`);
      break;
    default:
      break;
  }
  return bases;
}

// ---------------------------------------------------------------------------
// tooltip 組裝（對齊 item_tooltip.rs 的 build_tooltip / tooltip_icon）
// ---------------------------------------------------------------------------

const isIdSegment = (segment: string) => /^\d{8}$/.test(segment);

/** 由道具根節點（Character img 根或 Item img 內的道具節點）聚合 tooltip 內容。 */
async function buildTooltip(
  rootPath: string,
  rootNode: TreeNode,
  categoryHint: string,
): Promise<ItemTooltip | null> {
  const info = rootNode.children?.find((child) => child.name === "info") ?? null;
  if (!info) return null;
  const itemId = rootPath.split("/").at(-1) ?? "";
  const numericId = Number.parseInt(itemId, 10) || 0;

  const names = await loadItemNameTable();
  const entry = names.get(numericId);
  let title = entry?.name ?? itemId;
  const reqGender = propI64(info, "reqGender");
  if (reqGender === 0) title += " (男)";
  else if (reqGender === 1) title += " (女)";

  const description = entry?.desc?.trim() ? entry.desc : null;
  const iconBase64 = await tooltipIcon(rootPath, itemId, categoryHint);

  return {
    itemId,
    imgPath: rootPath,
    title,
    description,
    categoryLine: gearCategoryLine(itemId, categoryHint),
    iconBase64,
    requiredLevel: propI64(info, "reqLevel") ?? 0,
    requiredStr: propI64(info, "reqSTR") ?? 0,
    requiredDex: propI64(info, "reqDEX") ?? 0,
    requiredInt: propI64(info, "reqINT") ?? 0,
    requiredLuk: propI64(info, "reqLUK") ?? 0,
    requiredJobs: requiredJobLabels(propI64(info, "reqJob")),
    attributeLines: gearAttributeLines(info),
    attackSpeedLabel: attackSpeedLabel(propI64(info, "attackSpeed")),
    scrollCount: propI64(info, "tuc"),
    statLines: gearStatLines(info),
  };
}

async function cutImageBase64(path: string): Promise<string | null> {
  try {
    const result = await getImage(path);
    return result?.base64 ?? null;
  } catch {
    return null;
  }
}

/** 取 tooltip 圖示：一般裝備用 `info/icon`；髮型/臉型改用預設外觀圖。 */
async function tooltipIcon(
  rootPath: string,
  itemId: string,
  categoryHint: string,
): Promise<string | null> {
  const icon = await cutImageBase64(`${rootPath}/info/icon`);
  if (icon) return icon;
  const hint = categoryHint.toLowerCase();
  let inner: string | null = null;
  if (hint === "hair") inner = "default/hairOverHead";
  else if (hint === "face") inner = "default/face";
  else {
    // Character 直屬的 0002xxxx（臉型）/0003~0005（髮型）。
    const prefix = Math.floor((Number.parseInt(itemId, 10) || 0) / 10000);
    if (prefix === 2) inner = "default/face";
    else if (prefix >= 3 && prefix <= 5) inner = "default/hairOverHead";
  }
  return inner ? cutImageBase64(`${rootPath}/${inner}`) : null;
}

function gearAttributeLines(info: TreeNode): string[] {
  const lines: string[] = [];
  if (propBool(info, "only")) lines.push("專屬道具");
  if (propBool(info, "cash")) lines.push("點數道具");
  if (propBool(info, "timeLimited")) lines.push("限時道具");
  if (propBool(info, "tradeBlock")) lines.push("不可交易");
  else if (propBool(info, "accountSharable")) lines.push("帳號內可移動");
  if (propBool(info, "onlyEquip")) lines.push("只能單獨使用");
  return lines;
}

function gearStatLines(info: TreeNode): TooltipStatLine[] {
  const lines: TooltipStatLine[] = [];
  for (const [name, label] of GEAR_STAT_LABELS) {
    const value = propI64(info, name);
    if (value == null || value === 0) continue;
    lines.push({ label, value: value > 0 ? `+${value}` : String(value) });
  }
  return lines;
}

function requiredJobLabels(reqJob: number | null): string[] {
  if (reqJob == null || reqJob === 0) return [...ALL_JOB_LABELS];
  const labels = JOB_MASK_LABELS.filter(([mask]) => reqJob & mask).map(([, label]) => label);
  return labels.length > 0 ? labels : [...ALL_JOB_LABELS];
}

function attackSpeedLabel(value: number | null): string | null {
  if (value == null) return null;
  if (value < 2) return "非常快";
  switch (value) {
    case 2:
    case 3:
      return "比較快";
    case 4:
    case 5:
      return "快";
    case 6:
      return "普通";
    case 7:
    case 8:
      return "慢";
    case 9:
      return "比較慢";
    default:
      return "非常慢";
  }
}

/** 8 碼裝備 id 的前 3 位分類碼（id 開頭必為 0，例如 01302000 → 130）。 */
function idPrefix3(itemId: string): number {
  return Math.floor((Number.parseInt(itemId, 10) || 0) / 10000);
}

/** 裝備分類行：武器給「武器分類 : 單手劍」，其餘給「裝備分類 : 帽子」。 */
function gearCategoryLine(itemId: string, categoryHint: string): string {
  const weaponLabel = weaponTypeLabel(itemId);
  if (weaponLabel) return `武器分類 : ${weaponLabel}`;
  return `裝備分類 : ${gearCategoryLabel(itemId, categoryHint)}`;
}

function gearCategoryLabel(itemId: string, categoryHint: string): string {
  const prefix = idPrefix3(itemId);
  if (prefix === 100) return "帽子";
  if (prefix === 101) return "臉飾";
  if (prefix === 102) return "眼飾";
  if (prefix === 103) return "耳環";
  if (prefix === 104) return "上衣";
  if (prefix === 105) return "套服";
  if (prefix === 106) return "褲子";
  if (prefix === 107) return "鞋子";
  if (prefix === 108) return "手套";
  if (prefix === 109) return "盾牌";
  if (prefix === 110) return "披風";
  if (prefix === 111) return "戒指";
  if (prefix === 112) return "吊墜";
  if (prefix === 113) return "腰帶";
  if (prefix === 114) return "勳章";
  if (prefix === 115) return "肩飾";
  if (prefix >= 121 && prefix <= 159) return "武器";
  switch (categoryHint.toLowerCase()) {
    case "accessory":
      return "飾品";
    case "cap":
      return "帽子";
    case "coat":
      return "上衣";
    case "longcoat":
      return "套服";
    case "pants":
      return "褲子";
    case "shoes":
      return "鞋子";
    case "glove":
      return "手套";
    case "shield":
      return "盾牌";
    case "cape":
      return "披風";
    case "weapon":
      return "武器";
    case "ring":
      return "戒指";
    case "petequip":
      return "寵物裝備";
    case "tamingmob":
      return "騎寵";
    case "face":
      return "臉型";
    case "hair":
      return "髮型";
    default: {
      // Character 直屬的 0002xxxx（臉型）/0003~0005（髮型）。
      if (prefix === 2) return "臉型";
      if (prefix >= 3 && prefix <= 5) return "髮型";
      return "裝備";
    }
  }
}

function weaponTypeLabel(itemId: string): string | null {
  switch (idPrefix3(itemId)) {
    case 130:
      return "單手劍";
    case 131:
      return "單手斧";
    case 132:
      return "單手棍";
    case 133:
      return "短劍";
    case 134:
      return "刀";
    case 136:
      return "手杖";
    case 137:
      return "短杖";
    case 138:
      return "長杖";
    case 140:
      return "雙手劍";
    case 141:
      return "雙手斧";
    case 142:
      return "雙手棍";
    case 143:
      return "槍";
    case 144:
      return "矛";
    case 145:
      return "弓";
    case 146:
      return "弩";
    case 147:
      return "拳套";
    case 148:
      return "指虎";
    case 149:
      return "火槍";
    default:
      return null;
  }
}

// ---------------------------------------------------------------------------
// export 契約
// ---------------------------------------------------------------------------

/**
 * 路徑指向裝備（Character 下 8 碼數字 img 或其內部節點）時回傳 tooltip；
 * 非裝備路徑回 null（對齊 item_tooltip.rs 的 get_item_tooltip）。
 */
export async function getItemTooltip(path: string): Promise<ItemTooltip | null> {
  const segments = path.split("/");
  if (segments[0] !== "Character") return null;
  let imgPath: string;
  let hint: string;
  if (segments.length >= 2 && isIdSegment(segments[1])) {
    imgPath = `Character/${segments[1]}`;
    hint = "";
  } else if (segments.length >= 3 && isIdSegment(segments[2])) {
    imgPath = `Character/${segments[1]}/${segments[2]}`;
    hint = segments[1];
  } else {
    return null;
  }
  const state = requireCatalog();
  if (!Object.prototype.hasOwnProperty.call(state.entries, imgPath)) return null;
  const tree = await loadTree(imgPath);
  return buildTooltip(imgPath, tree, hint);
}

/**
 * 以道具 id 取 tooltip（怪物圖鑑戰利品 hover 用）；支援所有類別
 * （Character / Item 各分區），候選路徑逐一嘗試，找不到回 null。
 */
export async function getItemTooltipById(itemId: number): Promise<ItemTooltip | null> {
  const state = requireCatalog();
  const hasPath = (path: string) =>
    Object.prototype.hasOwnProperty.call(state.entries, path);
  // Character 直屬 img（0002xxxx 臉型、0003~0005 髮型等）優先。
  const candidates = [
    `Character/${String(itemId).padStart(8, "0")}`,
    ...itemPathCandidates(itemId, hasPath),
  ];
  for (const candidate of candidates) {
    const split = splitImgPrefix(state.entries, candidate);
    if (!split) continue;
    const [imgKey, inner] = split;
    let tree: TreeNode;
    try {
      tree = await loadTree(imgKey);
    } catch {
      continue;
    }
    const node = inner === "" ? tree : findNode(tree, inner);
    if (!node) continue;
    const categoryHint =
      segments2Hint(candidate);
    const tooltip = await buildTooltip(candidate, node, categoryHint);
    if (tooltip) return tooltip;
  }
  return null;
}

/** Character/<分類>/<id> 的分類提示；其餘路徑沒有提示。 */
function segments2Hint(path: string): string {
  const segments = path.split("/");
  return segments[0] === "Character" && segments.length === 3 ? segments[1] : "";
}
