// 每次開啟資料集或切換語系，都從新的資料來源載入名稱與圖片。
let active = "TW";
const listeners = new Set<() => void>();

export function activateDatasetLocale(code: string): void {
  active = code;
  for (const listener of listeners) listener();
}

export function getActiveLocale(): string {
  return active;
}

/** 供模組層快取在資料來源更新時清除舊資料。 */
export function onDatasetChange(listener: () => void): void {
  listeners.add(listener);
}

export function createDatasetCache<K, V>(): Map<K, V> {
  const cache = new Map<K, V>();
  onDatasetChange(() => cache.clear());
  return cache;
}
