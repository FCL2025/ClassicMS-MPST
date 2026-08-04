// 怪物圖鑑與商城商品清單（hosted 版）：語意對齊 src-tauri/src/monster.rs
// 與 commodity.rs。清單直接讀 pipeline 預處理的 features/*.json；
// 單怪明細即時從 tree/strings/sheets 聚合。

import type {
  CommodityItem,
  MapRef,
  MobDetail,
  MobDropItem,
  MobFrame,
  MonsterSummary,
  Value,
} from "../api.hosted";
import {
  cmpNodeNames,
  cutSprite,
  DATA_ROOT,
  fetchChecked,
  findNode,
  getImage,
  loadTextAsset,
  loadTree,
  requireCatalog,
  resolveUolChain,
} from "./core";
import type { TreeNode } from "./core";
import { itemPathCandidates, loadItemNameTable } from "./tooltip";

// ---------------------------------------------------------------------------
// features 清單（fetch + 快取，就這麼簡單）
// ---------------------------------------------------------------------------

let monsterBookPromise: Promise<MonsterSummary[]> | null = null;
let commodityPromise: Promise<CommodityItem[]> | null = null;

export function getMonsterBook(): Promise<MonsterSummary[]> {
  if (!monsterBookPromise) {
    monsterBookPromise = fetchChecked(`${DATA_ROOT}/features/monster_book.json`).then(
      (response) => response.json() as Promise<MonsterSummary[]>,
    );
    monsterBookPromise.catch(() => {
      monsterBookPromise = null;
    });
  }
  return monsterBookPromise;
}

export function getCommodityItems(): Promise<CommodityItem[]> {
  if (!commodityPromise) {
    commodityPromise = fetchChecked(`${DATA_ROOT}/features/commodity.json`).then(
      (response) => response.json() as Promise<CommodityItem[]>,
    );
    commodityPromise.catch(() => {
      commodityPromise = null;
    });
  }
  return commodityPromise;
}

// ---------------------------------------------------------------------------
// 圖鑑文字表（懶載入 + 整份快取，對齊 monster.rs 的 MonsterNameTables）
// ---------------------------------------------------------------------------

interface BookText {
  level: number | null;
  category: string | null;
  description: string | null;
}

interface MonsterNameTables {
  mobs: Map<number, string>;
  bookTexts: Map<number, BookText>;
  /** 地圖 id → 顯示名稱（「地區 : 地圖名」）。 */
  maps: Map<number, string>;
  /** 道具名稱（Item + Pet，Pet 只補 Item 缺的 id）。 */
  items: Map<number, string>;
}

let nameTablesPromise: Promise<MonsterNameTables> | null = null;

function stripBom(text: string): string {
  return text.startsWith("﻿") ? text.slice(1) : text;
}

/** 解析字串表 TextAsset（`{id: {name: ...}}`）成 id → 名稱；失敗時回空表。 */
async function loadNames(path: string): Promise<Map<number, string>> {
  const table = new Map<number, string>();
  try {
    const raw = JSON.parse(stripBom(await loadTextAsset(path))) as Record<
      string,
      { name?: unknown }
    >;
    for (const [key, value] of Object.entries(raw)) {
      const id = Number(key);
      if (!Number.isInteger(id) || typeof value?.name !== "string") continue;
      table.set(id, value.name);
    }
  } catch {
    // 資料集缺字串表時視為空表（對齊 Rust load_names）。
  }
  return table;
}

/** 解析 `String/TW/MonsterBook`：換行是字面 `\n` 兩字元，格式 `等級：Lv.8\n型態：植物\n\n描述…`。 */
function parseBookText(raw: string): BookText {
  const normalized = raw.replace(/\\n/g, "\n");
  let level: number | null = null;
  let category: string | null = null;
  for (const line of normalized.split("\n")) {
    if (line.startsWith("等級：Lv.")) {
      const parsed = Number.parseInt(line.slice("等級：Lv.".length).trim(), 10);
      level = Number.isNaN(parsed) ? null : parsed;
    } else if (line.startsWith("型態：")) {
      const value = line.slice("型態：".length).trim();
      if (value !== "") category = value;
    }
  }
  const separator = normalized.indexOf("\n\n");
  const body = separator >= 0 ? normalized.slice(separator + 2).trim() : "";
  return { level, category, description: body === "" ? null : body };
}

async function loadBookTexts(): Promise<Map<number, BookText>> {
  const table = new Map<number, BookText>();
  try {
    const raw = JSON.parse(stripBom(await loadTextAsset("String/TW/MonsterBook"))) as Record<
      string,
      unknown
    >;
    for (const [key, value] of Object.entries(raw)) {
      const id = Number(key);
      if (!Number.isInteger(id) || typeof value !== "string") continue;
      table.set(id, parseBookText(value));
    }
  } catch {
    // 同上：缺字串表視為空表。
  }
  return table;
}

/** 解析 `String/TW/Map`（`{id: {streetName, mapName}}`）成 id →「地區 : 地圖名」。 */
async function loadMapNames(): Promise<Map<number, string>> {
  const table = new Map<number, string>();
  try {
    const raw = JSON.parse(stripBom(await loadTextAsset("String/TW/Map"))) as Record<
      string,
      { streetName?: unknown; mapName?: unknown }
    >;
    for (const [key, value] of Object.entries(raw)) {
      const id = Number(key);
      if (!Number.isInteger(id)) continue;
      const street = typeof value?.streetName === "string" ? value.streetName : "";
      const mapName = typeof value?.mapName === "string" ? value.mapName : "";
      if (street === "" && mapName === "") continue;
      table.set(id, street === "" ? mapName : mapName === "" ? street : `${street} : ${mapName}`);
    }
  } catch {
    // 同上。
  }
  return table;
}

function loadNameTables(): Promise<MonsterNameTables> {
  if (!nameTablesPromise) {
    nameTablesPromise = (async () => {
      // 道具名稱以 String/TW/Item 為主（與 tooltip.ts 共用整份快取），
      // Pet 只補 Item 查無的 id（對齊 Rust 的 entry().or_insert()）。
      const [itemEntries, pets, mobs, bookTexts, maps] = await Promise.all([
        loadItemNameTable(),
        loadNames("String/TW/Pet"),
        loadNames("String/TW/Mob"),
        loadBookTexts(),
        loadMapNames(),
      ]);
      const items = new Map<number, string>();
      for (const [id, entry] of itemEntries) items.set(id, entry.name);
      for (const [id, name] of pets) {
        if (!items.has(id)) items.set(id, name);
      }
      return { mobs, bookTexts, maps, items };
    })();
    nameTablesPromise.catch(() => {
      nameTablesPromise = null;
    });
  }
  return nameTablesPromise;
}

// ---------------------------------------------------------------------------
// 節點屬性讀取（對齊 monster.rs 的 prop / value_as_i64）
// ---------------------------------------------------------------------------

function prop(node: TreeNode, name: string): TreeNode | null {
  const lower = name.toLowerCase();
  return node.children?.find((child) => child.name.toLowerCase() === lower) ?? null;
}

function valueAsI64(value: Value): number | null {
  switch (value.type) {
    case "U8":
    case "U16":
    case "Int":
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

/** 子節點的數值清單（reward / map 用；只留 > 0 的 id）。 */
function childIdList(node: TreeNode | null): number[] {
  if (!node) return [];
  return (node.children ?? [])
    .map((child) => valueAsI64(child.value))
    .filter((id): id is number => id != null && id > 0);
}

// ---------------------------------------------------------------------------
// 掉落物 icon（對齊 commodity.rs 的 icon_candidates）
// ---------------------------------------------------------------------------

/** 依道具 id 推出 icon 的候選節點路徑（按優先順序，逐一嘗試）。 */
function iconCandidates(itemId: number): string[] {
  // 9 開頭：Special img 內的節點是 7 碼 id，且 icon 直接掛在節點下（沒有 info 層）。
  if (Math.floor(itemId / 1_000_000) === 9) {
    const base = `Item/Special/0${Math.floor(itemId / 10000)}/${String(itemId).padStart(7, "0")}`;
    return ["/icon", "/iconRaw", "/info/icon", "/info/iconRaw"].map((suffix) => base + suffix);
  }
  const state = requireCatalog();
  const hasPath = (path: string) => Object.prototype.hasOwnProperty.call(state.entries, path);
  return itemPathCandidates(itemId, hasPath).flatMap((base) => [
    `${base}/info/icon`,
    `${base}/info/iconRaw`,
  ]);
}

async function dropIconBase64(itemId: number): Promise<string | null> {
  for (const path of iconCandidates(itemId)) {
    try {
      const result = await getImage(path);
      if (result?.base64) return result.base64;
    } catch {
      // 候選切圖失敗就試下一個。
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// Mob 動畫幀（對齊 monster.rs 的 mob_frames）
// ---------------------------------------------------------------------------

function hasCanvasChild(node: TreeNode): boolean {
  return (node.children ?? []).some((child) => child.value.type === "Canvas");
}

async function mobFrames(tree: TreeNode): Promise<MobFrame[]> {
  const children = tree.children ?? [];
  const byName = (name: string) => children.find((child) => child.name === name) ?? null;
  const action =
    [byName("stand"), byName("move")].find((node) => node && hasCanvasChild(node)) ??
    children.find((node) => node.name !== "info" && hasCanvasChild(node)) ??
    null;
  if (!action) return [];

  const frames: MobFrame[] = [];
  const frameNodes = [...(action.children ?? [])].sort((a, b) => cmpNodeNames(a.name, b.name));
  for (const frame of frameNodes) {
    // UOL 幀節點解析到實際 canvas；非 canvas/UOL 的節點略過。
    let target: TreeNode | null = null;
    if (frame.value.type === "Canvas") {
      target = frame;
    } else if (frame.value.type === "Uol") {
      target = resolveUolChain(tree, `${action.name}/${frame.name}`);
      if (target?.value.type !== "Canvas") target = null;
    }
    const canvas = target?.canvas;
    if (!canvas) continue;
    try {
      const base64 = await cutSprite(canvas);
      frames.push({
        base64,
        delay: propI64(frame, "delay") ?? 120,
        origin: [canvas.ox, canvas.oy],
        width: canvas.w,
        height: canvas.h,
      });
    } catch {
      // 單幀切圖失敗略過（對齊 Rust 的 filter_map）。
    }
  }
  return frames;
}

// ---------------------------------------------------------------------------
// export 契約：單一怪物明細
// ---------------------------------------------------------------------------

/**
 * 單一怪物明細：能力值（Mob img info）、動畫幀、掉落物（名稱+icon）、
 * 出沒地圖與圖鑑文字。缺 Mob img 時能力值/動畫留空，不報錯。
 */
export async function getMobDetail(mobId: number): Promise<MobDetail> {
  const names = await loadNameTables();
  const bookText = names.bookTexts.get(mobId) ?? null;

  const imgPath = `Mob/${String(mobId).padStart(7, "0")}`;
  const state = requireCatalog();
  let mobTree: TreeNode | null = null;
  if (Object.prototype.hasOwnProperty.call(state.entries, imgPath)) {
    try {
      mobTree = await loadTree(imgPath);
    } catch {
      mobTree = null;
    }
  }
  const info = mobTree ? findNode(mobTree, "info") : null;
  const stat = (name: string) => (info ? (propI64(info, name) ?? 0) : 0);

  // 掉落物與地圖從 MonsterBook 樹取（key 為無補零的 mob id）。
  let dropIds: number[] = [];
  let mapIds: number[] = [];
  if (Object.prototype.hasOwnProperty.call(state.entries, "MonsterBook")) {
    const bookTree = await loadTree("MonsterBook");
    const entry = (bookTree.children ?? []).find(
      (child) => Number.parseInt(child.name, 10) === mobId,
    );
    if (entry) {
      dropIds = childIdList(prop(entry, "reward"));
      mapIds = childIdList(prop(entry, "map"));
    }
  }

  const drops: MobDropItem[] = [];
  for (const itemId of dropIds) {
    const name = names.items.get(itemId) ?? null;
    const iconBase64 = await dropIconBase64(itemId);
    // 幽靈道具過濾：名稱與 icon 都查不到 = 未實裝道具，直接隱藏。
    if (name == null && iconBase64 == null) continue;
    drops.push({ itemId, name, iconBase64 });
  }
  const maps: MapRef[] = mapIds.map((mapId) => ({
    mapId,
    name: names.maps.get(mapId) ?? null,
  }));

  return {
    mobId,
    name: names.mobs.get(mobId) ?? null,
    level: (info ? propI64(info, "level") : null) ?? bookText?.level ?? null,
    category: bookText?.category ?? null,
    description: bookText?.description ?? null,
    maxHp: stat("maxHP"),
    maxMp: stat("maxMP"),
    exp: stat("exp"),
    pad: stat("PADamage"),
    pdd: stat("PDDamage"),
    mad: stat("MADamage"),
    mdd: stat("MDDamage"),
    acc: stat("acc"),
    eva: stat("eva"),
    speed: stat("speed"),
    pushed: stat("pushed"),
    bodyAttack: stat("bodyAttack") !== 0,
    undead: stat("undead") !== 0,
    maps,
    drops,
    frames: mobTree ? await mobFrames(mobTree) : [],
  };
}
