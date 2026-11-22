// 前端共用的「作用中語系」狀態。開啟資料集後由 App 依後端回傳的 OpenInfo 設定；
// 共用元件（NodeDetail、classicFrame）用它組出 String/<locale>/… 與 <locale>/UI/… 路徑。
// 語系切換是整站單一狀態（對齊後端單一 DataService），因此以模組層變數保存。

let active = "TW";
const listeners = new Set<() => void>();

/** 設定作用中語系；有變動時通知訂閱者（讓與語系相關的前端快取重建）。 */
export function setActiveLocale(code: string): void {
  if (!code || code === active) return;
  active = code;
  for (const listener of listeners) listener();
}

/** 目前作用中的語系代碼。 */
export function getActiveLocale(): string {
  return active;
}

/** 訂閱語系變更；回傳取消訂閱函式。 */
export function onLocaleChange(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
