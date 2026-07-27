import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { getImage, type EquipItem } from "../api";
import type { EquipSlot, PartsState } from "./AvatarPage";
import {
  dedupeColorVariants,
  resolveColorVariant,
  splitColorId,
  stripColorPrefix,
} from "./avatarColor";

// 部位 tab（顯示順序固定）；AvatarEquipPanel 的「目前裝備」清單也用這份順序。
export const SLOT_TABS: readonly { slot: EquipSlot; label: string }[] = [
  { slot: "cap", label: "帽子" },
  { slot: "coat", label: "上衣" },
  { slot: "longcoat", label: "套服" },
  { slot: "pants", label: "褲裙" },
  { slot: "shoes", label: "鞋子" },
  { slot: "glove", label: "手套" },
  { slot: "cape", label: "披風" },
  { slot: "shield", label: "盾牌" },
  { slot: "weapon", label: "武器" },
  { slot: "earrings", label: "耳環" },
  { slot: "faceAcc", label: "臉部飾品" },
  { slot: "eyeAcc", label: "眼部飾品" },
  { slot: "hair", label: "髮型" },
  { slot: "face", label: "臉型" },
];

// 卡片固定高 64px、grid 間距 7px，供列數估算（對齊 .avatar-shop-card / .avatar-shop-grid）。
const CARD_ROW_PX = 64 + 7;

// icon dataURL 快取（module-level）：裝備近七千件，只抓看過的，看過就不再抓。
const iconCache = new Map<string, Promise<string | null>>();

// 依 iconPath 載入 icon；取不到時依路徑型態試替代節點：
// 裝備 info/icon → info/iconRaw；髮型 default/hairOverHead → default/hair
// （有些髮型只有 hair 一張貼圖，如 00030160）。
export function loadAvatarIcon(path: string): Promise<string | null> {
  let cached = iconCache.get(path);
  if (!cached) {
    cached = (async () => {
      const result = await getImage(path).catch(() => null);
      if (result) return `data:image/png;base64,${result.base64}`;
      for (const fallback of [
        path.replace(/info\/icon$/, "info/iconRaw"),
        path.replace(/default\/hairOverHead$/, "default/hair"),
      ]) {
        if (fallback === path) continue;
        const retry = await getImage(fallback).catch(() => null);
        if (retry) return `data:image/png;base64,${retry.base64}`;
      }
      return null;
    })();
    iconCache.set(path, cached);
  }
  return cached;
}

// 頁碼列：頁數多時只留首尾與現頁附近，其餘用省略號折起（同 CommodityShop）。
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

function AvatarShopCard({
  item,
  worn,
  onToggle,
}: {
  item: EquipItem;
  worn: boolean;
  onToggle: () => void;
}) {
  const [icon, setIcon] = useState<string | null>(null);

  // icon 按頁 lazy：卡片只在當前頁渲染，掛載時才抓圖。
  useEffect(() => {
    let active = true;
    loadAvatarIcon(item.iconPath).then((url) => {
      if (active && url) setIcon(url);
    });
    return () => {
      active = false;
    };
  }, [item.iconPath]);

  return (
    <button
      type="button"
      className={`avatar-shop-card${worn ? " is-worn" : ""}`}
      title={`${item.name}（${item.id}）`}
      onClick={onToggle}
    >
      <span className="avatar-shop-card-icon">{icon ? <img src={icon} alt="" /> : null}</span>
      <span className="avatar-shop-card-name">
        {item.slot === "hair" || item.slot === "face" ? stripColorPrefix(item.name) : item.name}
      </span>
    </button>
  );
}

// slot + id → img 節點路徑（對齊後端 avatar.rs 的 img_key_of；耳環/臉飾/眼飾都在 Accessory）。
function slotImgPath(slot: EquipSlot, id: string): string {
  const dirs: Partial<Record<EquipSlot, string>> = {
    hair: "Hair",
    face: "Face",
    cap: "Cap",
    coat: "Coat",
    longcoat: "Longcoat",
    pants: "Pants",
    shoes: "Shoes",
    glove: "Glove",
    shield: "Shield",
    cape: "Cape",
    weapon: "Weapon",
  };
  return `Character/${dirs[slot] ?? "Accessory"}/${id}`;
}

interface AvatarShopProps {
  equipment: EquipItem[];
  parts: PartsState;
  /** 只列點裝（cash === true）；tab 不過濾、卡片過濾。 */
  cashOnly: boolean;
  /** 目前選取的髮色/臉色（0–7），穿上髮型/臉型時套用。 */
  hairColor: number;
  faceColor: number;
  onEquip: (slot: EquipSlot, id: string) => void;
  onRemove: (slot: EquipSlot) => void;
  /** 點卡片時回報該裝備的 img 路徑（給 footer 麵包屑）。 */
  onPickItem?: (path: string) => void;
}

// 右側試穿商店：部位 tab + 卡片 grid + 動態分頁（欄×列由 ResizeObserver 量出）。
export function AvatarShop({
  equipment,
  parts,
  cashOnly,
  hairColor,
  faceColor,
  onEquip,
  onRemove,
  onPickItem,
}: AvatarShopProps) {
  const [activeSlot, setActiveSlot] = useState<EquipSlot>("cap");
  const [page, setPage] = useState(0);
  // 初次渲染、尚未量測前的暫用每頁數；之後由可用高度 × 欄數動態決定。
  const [pageSize, setPageSize] = useState(24);
  const gridWrapRef = useRef<HTMLDivElement | null>(null);
  const gridRef = useRef<HTMLDivElement | null>(null);

  // 依 grid 容器可用空間調整每頁筆數：欄數讀 computed style、列數用高度 ÷ 卡片高。
  useLayoutEffect(() => {
    const wrap = gridWrapRef.current;
    if (!wrap) return;
    const measure = () => {
      const grid = gridRef.current;
      if (!grid) return;
      const cols = Math.max(
        1,
        getComputedStyle(grid).gridTemplateColumns.split(" ").filter(Boolean).length,
      );
      // 16 = grid wrap 的上下 padding 各 8px。
      const rows = Math.max(1, Math.floor((wrap.clientHeight - 16) / CARD_ROW_PX));
      const next = cols * rows;
      setPageSize((previous) => (previous === next ? previous : next));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(wrap);
    return () => observer.disconnect();
  }, []);

  // 裝備依部位分桶（順序保持後端回傳序）。
  const itemsBySlot = useMemo(() => {
    const map = new Map<string, EquipItem[]>();
    for (const item of equipment) {
      const list = map.get(item.slot);
      if (list) list.push(item);
      else map.set(item.slot, [item]);
    }
    return map;
  }, [equipment]);

  const allItems = itemsBySlot.get(activeSlot) ?? [];
  // 髮型/臉型同一造型有多個顏色 id，列表只留預設色那張卡。
  const items = useMemo(() => {
    const filtered = cashOnly ? allItems.filter((item) => item.cash) : allItems;
    return dedupeColorVariants(activeSlot, filtered);
  }, [allItems, cashOnly, activeSlot]);
  // 換色時查 variant 是否存在用（全裝備 id 集合）。
  const equipmentIds = useMemo(() => new Set(equipment.map((item) => item.id)), [equipment]);
  const totalPages = Math.max(1, Math.ceil(items.length / pageSize));
  const currentPage = Math.min(page, totalPages - 1);
  const pageItems = items.slice(currentPage * pageSize, currentPage * pageSize + pageSize);

  return (
    <section className="avatar-shop" aria-label="裝備列表">
      <div className="avatar-shop-tabs" role="tablist">
        {SLOT_TABS.map(({ slot, label }) => (
          <button
            key={slot}
            type="button"
            role="tab"
            aria-selected={slot === activeSlot}
            className={slot === activeSlot ? "is-active" : ""}
            onClick={() => {
              setActiveSlot(slot);
              setPage(0);
            }}
          >
            {label}
          </button>
        ))}
      </div>

      <div className="avatar-shop-grid-wrap" ref={gridWrapRef}>
        {pageItems.length === 0 ? (
          <p className="avatar-shop-status">
            {cashOnly ? "此部位沒有點裝" : "此部位沒有裝備"}
          </p>
        ) : (
          <div className="avatar-shop-grid" ref={gridRef}>
            {pageItems.map((item) => {
              const slot = item.slot as EquipSlot;
              // 髮型/臉型：已穿的是同造型任一顏色就算穿著。
              const wornId = parts[slot]?.id;
              const wornSplit = wornId ? splitColorId(slot, wornId) : null;
              const itemSplit = splitColorId(slot, item.id);
              const worn =
                wornId != null &&
                (wornId === item.id ||
                  (wornSplit != null && itemSplit != null && wornSplit.base === itemSplit.base));
              return (
                <AvatarShopCard
                  key={item.id}
                  item={item}
                  worn={worn}
                  onToggle={() => {
                    // 點已穿的卡片 → 脫下；否則穿上（同 slot 取代），
                    // 髮型/臉型套用目前選取的顏色。麵包屑跟著顯示該裝備路徑。
                    if (worn) {
                      onRemove(slot);
                      onPickItem?.(slotImgPath(slot, wornId!));
                    } else {
                      const color = slot === "hair" ? hairColor : faceColor;
                      const id = resolveColorVariant(slot, item.id, color, (candidate) =>
                        equipmentIds.has(candidate),
                      );
                      onEquip(slot, id);
                      onPickItem?.(slotImgPath(slot, id));
                    }
                  }}
                />
              );
            })}
          </div>
        )}
      </div>

      {totalPages > 1 ? (
        <nav className="avatar-shop-pager" aria-label="裝備分頁">
          <button
            type="button"
            disabled={currentPage === 0}
            onClick={() => setPage(currentPage - 1)}
          >
            &lt;
          </button>
          {pageNumbers(currentPage, totalPages).map((p, i) =>
            p === "…" ? (
              <span key={`ellipsis-${i}`} className="avatar-shop-pager-ellipsis">
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
    </section>
  );
}
