import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { getCommodityItems, type CommodityItem } from "../api";
import { loadClassicFrame } from "./classicFrame";
import { TextAssetCard } from "./NodeDetail";

// 初次渲染、尚未量測前的暫用每頁道具數；之後由可用高度 × 欄數動態決定。
const PAGE_SIZE = 12;
// 卡片固定高 72px（見 .commodity-item）、grid 間距 7px，供列數估算。
const CARD_ROW_PX = 72 + 7;

// SN 前段是分類代碼：萬位以上是主分類、之後兩碼是子分類
// （例如 110000000 → 主 11 子 0；92000000 → 主 9 子 20）。
const mainOf = (sn: number) => Math.floor(sn / 10_000_000);
const subOf = (sn: number) => Math.floor(sn / 100_000) % 100;

// 主分類標籤（依商品實際內容判斷；未列出的代碼顯示「分類 NN」）。
const MAIN_LABELS: Record<number, string> = {
  9: "人氣商品",
  11: "特殊",
  12: "轉蛋",
  13: "強化",
  14: "裝備",
  15: "美容",
  16: "寵物",
  17: "便利",
  18: "其他",
};

// 子分類標籤（同樣依內容判斷；未列出的顯示「子分類 NN」）。
const SUB_LABELS: Record<number, Record<number, string>> = {
  14: {
    0: "其他",
    1: "套裝",
    2: "武器",
    3: "帽子",
    4: "套服",
    5: "上衣",
    6: "褲裙",
    7: "披風",
    8: "鞋子",
    9: "手套",
    10: "戒指",
    11: "眼飾",
    12: "特效",
  },
  15: { 0: "美髮", 1: "整形", 2: "表情", 3: "染髮" },
  16: { 2: "寵物", 3: "寵物服飾", 4: "寵物食品", 5: "其他" },
  17: { 0: "雜貨", 1: "通訊", 2: "天氣", 3: "欄位擴充", 4: "商店" },
};

const mainLabel = (main: number) => MAIN_LABELS[main] ?? `分類 ${main}`;
const subLabel = (main: number, sub: number) =>
  SUB_LABELS[main]?.[sub] ?? `子分類 ${sub}`;

// 頁碼列：頁數多時只留首尾與現頁附近，其餘用省略號折起。
function pageNumbers(current: number, total: number): (number | "…")[] {
  if (total <= 7) return Array.from({ length: total }, (_, i) => i);
  const keep = new Set(
    [0, total - 1, current - 1, current, current + 1].filter((p) => p >= 0 && p < total),
  );
  const out: (number | "…")[] = [];
  let prev = -1;
  for (const p of [...keep].sort((a, b) => a - b)) {
    if (prev !== -1 && p - prev > 1) out.push("…");
    out.push(p);
    prev = p;
  }
  return out;
}

// Etc/TW/Commodity 的商城檢視：分類 tab + 道具卡片（icon / 名稱 / 價格與數量）+ 分頁。
// 名稱與 icon 由後端 get_commodity_items 一次聚合回傳；右上角可切回原始 JSON。
export function CommodityShop({ text }: { text: string }) {
  const [showRaw, setShowRaw] = useState(false);
  const [items, setItems] = useState<CommodityItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [main, setMain] = useState<number | null>(null);
  const [sub, setSub] = useState<number | null>(null);
  const [page, setPage] = useState(0);
  const [frame, setFrame] = useState<string | null>(null);
  const sectionRef = useRef<HTMLElement | null>(null);
  const gridRef = useRef<HTMLDivElement | null>(null);
  // 視窗拉滿右半窗格的高度（px）；未量測前為 null（自然高度）。
  const [fillHeight, setFillHeight] = useState<number | null>(null);
  // 動態每頁數：欄數（量 grid 實際欄數）× 列數（依可用高度估算）。
  const [pageSize, setPageSize] = useState(PAGE_SIZE);

  // 依窗格可用空間調整視窗高度與每頁筆數。每次渲染後量一次（setState 有守門，
  // 值相同不會觸發重繪），另以 ResizeObserver 跟隨窗格尺寸變化。
  useLayoutEffect(() => {
    const section = sectionRef.current;
    if (!section || !items) return;
    const pane = section.closest(".app-right");
    if (!pane) return;

    const measure = () => {
      const paneRect = pane.getBoundingClientRect();
      const secTop = section.getBoundingClientRect().top;
      // 12 = .app-right 的 padding-bottom。
      const height = Math.max(240, Math.floor(paneRect.bottom - 12 - secTop));
      setFillHeight((h) => (h === height ? h : height));

      const grid = gridRef.current;
      if (!grid) return;
      const cols = Math.max(
        1,
        getComputedStyle(grid).gridTemplateColumns.split(" ").filter(Boolean).length,
      );
      const gridTop = grid.getBoundingClientRect().top;
      // 分頁列約 30px + 視窗下框 27px + grid 下 padding 8px。
      const availRows = paneRect.bottom - 12 - gridTop - 30 - 27 - 8;
      const rows = Math.max(1, Math.floor(availRows / CARD_ROW_PX));
      const next = cols * rows;
      setPageSize((s) => (s === next ? s : next));
    };

    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(pane);
    return () => observer.disconnect();
  });

  useEffect(() => {
    let active = true;
    loadClassicFrame().then((url) => {
      if (active) setFrame(url);
    });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    let active = true;
    getCommodityItems()
      .then((list) => {
        if (active) setItems(list);
      })
      .catch((nextError) => {
        if (active) setError(String(nextError));
      });
    return () => {
      active = false;
    };
  }, []);

  // 主分類 → 子分類 → 商品（SN 已由後端排序，這裡只分桶）。
  const groups = useMemo(() => {
    const map = new Map<number, Map<number, CommodityItem[]>>();
    for (const item of items ?? []) {
      const m = mainOf(item.sn);
      const s = subOf(item.sn);
      let subs = map.get(m);
      if (!subs) map.set(m, (subs = new Map()));
      const bucket = subs.get(s);
      if (bucket) bucket.push(item);
      else subs.set(s, [item]);
    }
    return map;
  }, [items]);

  if (showRaw) {
    return (
      <div className="commodity-raw-stack">
        <div className="commodity-raw-bar">
          <button type="button" className="find-hint" onClick={() => setShowRaw(false)}>
            ← 返回商城預覽
          </button>
        </div>
        <TextAssetCard text={text} />
      </div>
    );
  }

  const mains = [...groups.keys()].sort((a, b) => a - b);
  const activeMain = main ?? mains[0];
  const subs = [...(groups.get(activeMain)?.keys() ?? [])].sort((a, b) => a - b);
  const activeSub = sub ?? subs[0];
  const visible = groups.get(activeMain)?.get(activeSub) ?? [];
  const totalPages = Math.max(1, Math.ceil(visible.length / pageSize));
  const currentPage = Math.min(page, totalPages - 1);
  const pageItems = visible.slice(
    currentPage * pageSize,
    currentPage * pageSize + pageSize,
  );

  return (
    <section
      ref={sectionRef}
      className={frame ? "commodity-window has-frame" : "commodity-window"}
      style={{
        ...(frame ? { borderImageSource: `url("${frame}")` } : undefined),
        ...(fillHeight != null ? { height: fillHeight } : undefined),
      }}
    >
      <div className="commodity-window-title">
        <span>商城{items ? `（${items.length}）` : ""}</span>
        <button type="button" className="commodity-title-btn" onClick={() => setShowRaw(true)}>
          原始 JSON
        </button>
      </div>

      {error ? <p className="error commodity-status">{error}</p> : null}
      {!error && !items ? <p className="commodity-status">正在載入商城資料…</p> : null}

      {items ? (
        <>
          <div className="commodity-tabs" role="tablist">
            {mains.map((m) => (
              <button
                key={m}
                type="button"
                role="tab"
                aria-selected={m === activeMain}
                className={m === activeMain ? "is-active" : ""}
                onClick={() => {
                  setMain(m);
                  setSub(null);
                  setPage(0);
                }}
              >
                {mainLabel(m)}
              </button>
            ))}
          </div>

          {subs.length > 1 ? (
            <div className="commodity-subtabs" role="tablist">
              {subs.map((s) => (
                <button
                  key={s}
                  type="button"
                  role="tab"
                  aria-selected={s === activeSub}
                  className={s === activeSub ? "is-active" : ""}
                  onClick={() => {
                    setSub(s);
                    setPage(0);
                  }}
                >
                  {subLabel(activeMain, s)}
                </button>
              ))}
            </div>
          ) : null}

          {pageItems.length === 0 ? (
            <p className="commodity-status">此分類沒有商品</p>
          ) : (
            <div className="commodity-grid" ref={gridRef}>
              {pageItems.map((item) => (
                <article className="commodity-item" key={item.sn} title={`道具代碼 ${item.itemId}`}>
                  <div className="commodity-item-icon">
                    {item.iconBase64 ? (
                      <img src={`data:image/png;base64,${item.iconBase64}`} alt="" />
                    ) : (
                      <span className="commodity-item-noicon">?</span>
                    )}
                  </div>
                  <div className="commodity-item-body">
                    <strong className="commodity-item-name">
                      {item.name ?? `#${item.itemId}`}
                    </strong>
                    <span className="commodity-item-price">
                      {item.price} 點 / {item.count} 個
                    </span>
                  </div>
                </article>
              ))}
            </div>
          )}

          {totalPages > 1 ? (
            <nav className="commodity-pager" aria-label="商城分頁">
              <button
                type="button"
                disabled={currentPage === 0}
                onClick={() => setPage(currentPage - 1)}
              >
                &lt;
              </button>
              {pageNumbers(currentPage, totalPages).map((p, i) =>
                p === "…" ? (
                  <span key={`ellipsis-${i}`} className="commodity-pager-ellipsis">
                    …
                  </span>
                ) : (
                  <button
                    key={p}
                    type="button"
                    className={p === currentPage ? "is-active" : ""}
                    onClick={() => setPage(p)}
                  >
                    {p + 1}
                  </button>
                ),
              )}
              <button
                type="button"
                disabled={currentPage >= totalPages - 1}
                onClick={() => setPage(currentPage + 1)}
              >
                &gt;
              </button>
            </nav>
          ) : null}
        </>
      ) : null}
    </section>
  );
}
