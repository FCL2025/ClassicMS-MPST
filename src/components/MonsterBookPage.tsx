import { useEffect, useMemo, useRef, useState } from "react";
import {
  getMobDetail,
  getMonsterBook,
  type MobDetail,
  type MobFrame,
  type MonsterSummary,
} from "../api";

/** Mob 動畫：逐幀播放，各幀以 origin 對齊（取全幀 bounding box）避免跳動。 */
function MobSprite({ frames, playing }: { frames: MobFrame[]; playing: boolean }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [images, setImages] = useState<HTMLImageElement[] | null>(null);
  const [box, setBox] = useState<{ left: number; top: number; width: number; height: number } | null>(
    null,
  );

  // 載入所有幀圖，並計算 origin 對齊的聯集 bounding box。
  useEffect(() => {
    let cancelled = false;
    if (frames.length === 0) {
      setImages(null);
      setBox(null);
      return;
    }
    Promise.all(
      frames.map(
        (frame) =>
          new Promise<HTMLImageElement>((resolve, reject) => {
            const img = new Image();
            img.onload = () => resolve(img);
            img.onerror = () => reject(new Error("幀圖載入失敗"));
            img.src = `data:image/png;base64,${frame.base64}`;
          }),
      ),
    )
      .then((loaded) => {
        if (cancelled) return;
        let left = 0;
        let top = 0;
        let right = 0;
        let bottom = 0;
        frames.forEach((frame) => {
          left = Math.max(left, frame.origin[0]);
          top = Math.max(top, frame.origin[1]);
          right = Math.max(right, frame.width - frame.origin[0]);
          bottom = Math.max(bottom, frame.height - frame.origin[1]);
        });
        setImages(loaded);
        setBox({ left, top, width: left + right, height: top + bottom });
      })
      .catch(() => {
        if (!cancelled) {
          setImages(null);
          setBox(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [frames]);

  // 播放迴圈：依每幀 delay 推進；暫停或只剩一幀時停在第 0 幀。
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !images || !box) return;
    canvas.width = box.width;
    canvas.height = box.height;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    let index = 0;
    let timer = 0;
    const draw = () => {
      const frame = frames[index];
      ctx.clearRect(0, 0, box.width, box.height);
      ctx.drawImage(images[index], box.left - frame.origin[0], box.top - frame.origin[1]);
    };
    draw();
    if (playing && frames.length > 1) {
      const step = () => {
        index = (index + 1) % frames.length;
        draw();
        timer = window.setTimeout(step, Math.max(30, frames[index].delay));
      };
      timer = window.setTimeout(step, Math.max(30, frames[0].delay));
    }
    return () => window.clearTimeout(timer);
  }, [images, box, frames, playing]);

  if (frames.length === 0 || !box) {
    return <div className="monster-sprite-empty">無動畫資料</div>;
  }
  // 像素圖整數倍放大（對齊紙娃娃 2x 預覽）；太大才縮小放進框裡。
  const scale = Math.min(4, Math.max(1, Math.floor(160 / Math.max(box.width, box.height))));
  return (
    <canvas
      ref={canvasRef}
      className="monster-sprite-canvas"
      style={{
        aspectRatio: `${box.width} / ${box.height}`,
        width: box.width * scale,
      }}
    />
  );
}

/** 戰利品分組：依道具 id 首位分 裝備(1) / 消耗(2) / 其他（3~5、9 等）。 */
const DROP_GROUP_ORDER = ["裝備", "消耗", "其他"] as const;

type DropGroup = (typeof DROP_GROUP_ORDER)[number];

function dropGroup(itemId: number): DropGroup {
  const prefix = Math.floor(itemId / 1_000_000);
  if (prefix === 1) return "裝備";
  if (prefix === 2) return "消耗";
  return "其他";
}

function statRows(detail: MobDetail): [string, string][] {
  const rows: [string, string][] = [];
  const push = (label: string, value: number) => {
    if (value !== 0) rows.push([label, value.toLocaleString()]);
  };
  push("HP", detail.maxHp);
  push("MP", detail.maxMp);
  push("EXP", detail.exp);
  push("物理攻擊", detail.pad);
  push("物理防禦", detail.pdd);
  push("魔法攻擊", detail.mad);
  push("魔法防禦", detail.mdd);
  push("命中", detail.acc);
  push("迴避", detail.eva);
  push("速度", detail.speed);
  push("擊退", detail.pushed);
  return rows;
}

// 怪物圖鑑頁：左欄清單 + 搜尋，右欄明細；第一次切到此頁才載入資料。
export function MonsterBookPage({
  visible,
  onPickMob,
}: {
  visible: boolean;
  /** 選取怪物時回報圖鑑路徑（給 App footer 麵包屑）。 */
  onPickMob?: (path: string) => void;
}) {
  const [book, setBook] = useState<MonsterSummary[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [detail, setDetail] = useState<MobDetail | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const detailGenerationRef = useRef(0);

  // 清單只載入一次（此元件掛載後保持不卸載）。
  useEffect(() => {
    let active = true;
    getMonsterBook()
      .then((list) => {
        if (!active) return;
        setBook(list);
        if (list.length > 0) setSelectedId((id) => id ?? list[0].mobId);
      })
      .catch((error: unknown) => {
        if (active) setLoadError(String(error));
      });
    return () => {
      active = false;
    };
  }, []);

  // 搜尋：符合怪物名稱、怪物 id、或任一掉落物名稱。
  const filtered = useMemo(() => {
    if (!book) return [];
    const keyword = query.trim().toLowerCase();
    if (!keyword) return book;
    return book.filter(
      (entry) =>
        entry.name?.toLowerCase().includes(keyword) ||
        String(entry.mobId).includes(keyword) ||
        entry.drops.some((drop) => drop.name?.toLowerCase().includes(keyword)),
    );
  }, [book, query]);

  // 選取怪物 → 載入明細（generation guard 防止舊回應蓋掉新選擇）。
  useEffect(() => {
    if (selectedId === null) return;
    const generation = ++detailGenerationRef.current;
    setDetailLoading(true);
    setDetailError(null);
    getMobDetail(selectedId)
      .then((next) => {
        if (detailGenerationRef.current !== generation) return;
        setDetail(next);
        onPickMob?.(`MonsterBook/${next.mobId}`);
      })
      .catch((error: unknown) => {
        if (detailGenerationRef.current !== generation) return;
        setDetail(null);
        setDetailError(String(error));
      })
      .finally(() => {
        if (detailGenerationRef.current === generation) setDetailLoading(false);
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedId]);

  let content;
  if (loadError) {
    content = <p className="monster-page-status">怪物圖鑑資料載入失敗：{loadError}</p>;
  } else if (!book) {
    content = <p className="monster-page-status">正在載入怪物圖鑑…</p>;
  } else {
    content = (
      <>
        <aside className="monster-left">
          <div className="monster-search">
            <input
              type="search"
              value={query}
              placeholder="搜尋怪物、ID 或掉落物…"
              aria-label="搜尋怪物"
              onChange={(event) => setQuery(event.target.value)}
            />
            <span className="monster-search-count">
              {filtered.length} / {book.length}
            </span>
          </div>
          <div className="monster-list" role="listbox" aria-label="怪物清單">
            {filtered.map((entry) => (
              <button
                key={entry.mobId}
                type="button"
                role="option"
                aria-selected={entry.mobId === selectedId}
                className={`monster-list-item${entry.mobId === selectedId ? " is-active" : ""}`}
                onClick={() => setSelectedId(entry.mobId)}
              >
                <span className="monster-list-name">{entry.name ?? "（未命名）"}</span>
                <span className="monster-list-meta">
                  {entry.level !== null ? `Lv.${entry.level}` : "Lv.?"}
                </span>
                <span className="monster-list-id">{entry.mobId}</span>
              </button>
            ))}
            {filtered.length === 0 ? (
              <p className="monster-list-empty">沒有符合「{query}」的怪物</p>
            ) : null}
          </div>
        </aside>
        <section className="monster-detail">
          {detailError ? (
            <p className="monster-page-status">怪物資料載入失敗：{detailError}</p>
          ) : !detail ? (
            <p className="monster-page-status">
              {detailLoading ? "正在載入怪物資料…" : "從左邊選擇一隻怪物"}
            </p>
          ) : (
            <div className="monster-detail-scroll">
              <header className="monster-hero">
                <div className="monster-sprite">
                  <MobSprite frames={detail.frames} playing={visible} />
                </div>
                <div className="monster-hero-text">
                  <h2>{detail.name ?? "（未命名）"}</h2>
                  <p className="monster-hero-id">{detail.mobId}</p>
                  <div className="monster-hero-chips">
                    {detail.level !== null ? (
                      <span className="monster-chip">Lv.{detail.level}</span>
                    ) : null}
                    {detail.category ? (
                      <span className="monster-chip">型態：{detail.category}</span>
                    ) : null}
                    {detail.bodyAttack ? <span className="monster-chip">身體攻擊</span> : null}
                    {detail.undead ? <span className="monster-chip">不死族</span> : null}
                  </div>
                  {detail.description ? (
                    <p className="monster-description">{detail.description}</p>
                  ) : null}
                </div>
              </header>

              {statRows(detail).length > 0 ? (
                <section className="monster-section">
                  <h3>能力值</h3>
                  <dl className="monster-stats">
                    {statRows(detail).map(([label, value]) => (
                      <div key={label} className="monster-stat">
                        <dt>{label}</dt>
                        <dd>{value}</dd>
                      </div>
                    ))}
                  </dl>
                </section>
              ) : null}

              <section className="monster-section">
                <h3>戰利品（{detail.drops.length}）</h3>
                {detail.drops.length > 0 ? (
                  DROP_GROUP_ORDER.map((group) => {
                    const items = detail.drops.filter(
                      (drop) => dropGroup(drop.itemId) === group,
                    );
                    if (items.length === 0) return null;
                    return (
                      <div key={group} className="monster-drop-group">
                        <h4>
                          {group}（{items.length}）
                        </h4>
                        <div className="monster-drops">
                          {items.map((drop) => (
                            <div
                              key={drop.itemId}
                              className="monster-drop"
                              title={drop.name ?? String(drop.itemId)}
                            >
                              <div className="monster-drop-icon">
                                {drop.iconBase64 ? (
                                  <img
                                    alt={drop.name ?? String(drop.itemId)}
                                    draggable={false}
                                    src={`data:image/png;base64,${drop.iconBase64}`}
                                  />
                                ) : (
                                  <span className="monster-drop-fallback">{drop.itemId}</span>
                                )}
                              </div>
                              <span className="monster-drop-name">{drop.name ?? drop.itemId}</span>
                            </div>
                          ))}
                        </div>
                      </div>
                    );
                  })
                ) : (
                  <p className="monster-section-empty">圖鑑沒有此怪物的掉落紀錄</p>
                )}
              </section>

              {detail.maps.length > 0 ? (
                <section className="monster-section">
                  <h3>出沒地圖（{detail.maps.length}）</h3>
                  <div className="monster-maps">
                    {detail.maps.map((map) => (
                      <span key={map.mapId} className="monster-chip" title={String(map.mapId)}>
                        {map.name ?? map.mapId}
                      </span>
                    ))}
                  </div>
                </section>
              ) : null}
            </div>
          )}
        </section>
      </>
    );
  }

  return (
    <div className="monster-page" hidden={!visible}>
      {content}
    </div>
  );
}
