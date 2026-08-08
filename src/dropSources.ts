// 「也掉落此道具的怪物」共享查詢：怪物圖鑑頁與任務頁的掉落來源彈窗
// 都從這裡拿（圖鑑戰利品 ∪ 任務道具補充表），圖鑑清單整個 session 只抓一次。

import { getMonsterBook, type MonsterSummary } from "./api";
import { QUEST_ITEM_DROPS } from "./questDrops";

let bookPromise: Promise<MonsterSummary[]> | null = null;

/** 圖鑑清單只抓一次（MonsterBookPage 清單與各頁的掉落來源彈窗共用）；失敗時允許重試。 */
export function loadMonsterBookOnce(): Promise<MonsterSummary[]> {
  if (!bookPromise) {
    bookPromise = getMonsterBook();
    bookPromise.catch(() => {
      bookPromise = null;
    });
  }
  return bookPromise;
}

/** 掉落來源排序：等級升冪（未知排最後），再等級同名稱、再 id。 */
export function compareDropSourceMobs(a: MonsterSummary, b: MonsterSummary): number {
  return (
    (a.level ?? Number.MAX_SAFE_INTEGER) - (b.level ?? Number.MAX_SAFE_INTEGER) ||
    (a.name ?? "").localeCompare(b.name ?? "", "zh-Hant") ||
    a.mobId - b.mobId
  );
}

/** 會掉落指定道具的怪物（圖鑑戰利品 ∪ 任務道具補充表），已排序。 */
export async function getDropSources(itemId: number): Promise<MonsterSummary[]> {
  const book = await loadMonsterBookOnce();
  const sources = new Map<number, MonsterSummary>();
  for (const entry of book) {
    if (entry.drops.some((drop) => drop.itemId === itemId)) sources.set(entry.mobId, entry);
  }
  const questMobIds = QUEST_ITEM_DROPS[itemId];
  if (questMobIds) {
    for (const mobId of questMobIds) {
      if (sources.has(mobId)) continue;
      // 怪物不在圖鑑清單時補一筆最小資料，讓補充表的來源仍可顯示與跳轉。
      const entry = book.find((mob) => mob.mobId === mobId) ?? {
        mobId,
        name: null,
        level: null,
        drops: [],
      };
      sources.set(mobId, entry);
    }
  }
  return [...sources.values()].sort(compareDropSourceMobs);
}
