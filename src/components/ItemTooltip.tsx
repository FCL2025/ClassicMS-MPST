import { useEffect, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import type { ItemTooltip as ItemTooltipData } from "../api";
import { exportTooltipPng } from "./tooltipExport";

/** 呈現方式對齊 Harepacker-MPST 的 ItemTooltipCard（gear 版本）。 */

export const ALL_JOB_LABELS = ["初心者", "劍士", "法師", "弓箭手", "盜賊", "海盜"];
/** 需求屬性標籤（畫面與 PNG 匯出共用）。 */
export const REQ_LABELS = {
  level: "等級限制",
  str: "力量限制",
  dex: "敏捷限制",
  int: "智力限制",
  luk: "幸運限制",
} as const;
const ICON_SCALE = 2;
const ICON_MIN_SIZE = 34;
const ICON_MAX_SIZE = 68;

const numberFormatter = new Intl.NumberFormat();

function formatReq(value: number): string {
  return numberFormatter.format(value);
}

function reqTone(value: number, highlightLevel = false): string {
  if (value === 0) return "is-zero";
  return highlightLevel ? "is-level" : "is-active";
}

export function ItemTooltip({ details }: { details: ItemTooltipData }) {
  const [iconSize, setIconSize] = useState<{ width: number; height: number } | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  // 右鍵選單開著時：點外面、Esc、視窗失焦或縮放就關掉。
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const onPointerDown = (e: PointerEvent) => {
      if (menuRef.current && e.target instanceof Node && menuRef.current.contains(e.target)) {
        return;
      }
      close();
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
    };
  }, [menu]);

  const openMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    // 預留選單大概尺寸，避免超出視窗右下緣。
    setMenu({
      x: Math.min(e.clientX, window.innerWidth - 150),
      y: Math.min(e.clientY, window.innerHeight - 48),
    });
  };

  const saveAsPng = () => {
    setMenu(null);
    exportTooltipPng(details).catch((err: unknown) => {
      window.alert(`儲存失敗：${String(err)}`);
    });
  };

  const iconStyle: CSSProperties | undefined = iconSize
    ? {
        width: Math.min(
          Math.max(iconSize.width * ICON_SCALE, ICON_MIN_SIZE),
          ICON_MAX_SIZE,
        ),
        height: Math.min(
          Math.max(iconSize.height * ICON_SCALE, ICON_MIN_SIZE),
          ICON_MAX_SIZE,
        ),
      }
    : undefined;

  return (
    <>
      <article
        className="item-tooltip"
        aria-label="裝備 tooltip"
        title="右鍵可儲存成圖片"
        onContextMenu={openMenu}
      >
      <header className="item-tooltip-header">
        <span className="item-tooltip-id">{details.itemId}</span>
        <h3>{details.title}</h3>
        {details.attributeLines.length > 0 ? (
          <p className="item-tooltip-attributes">{details.attributeLines.join(", ")}</p>
        ) : null}
      </header>

      <section className="item-tooltip-summary">
        <div className="item-tooltip-icon-frame">
          {details.iconBase64 ? (
            <img
              alt={details.title}
              draggable={false}
              src={`data:image/png;base64,${details.iconBase64}`}
              style={iconStyle}
              onLoad={(event) =>
                setIconSize({
                  width: event.currentTarget.naturalWidth,
                  height: event.currentTarget.naturalHeight,
                })
              }
            />
          ) : (
            <span className="item-tooltip-icon-fallback">{details.itemId}</span>
          )}
        </div>
        <dl className="item-tooltip-reqs">
          <div className="item-tooltip-req-row">
            <div className={`item-tooltip-req is-wide ${reqTone(details.requiredLevel, true)}`}>
              <dt>{REQ_LABELS.level}</dt>
              <dd>{formatReq(details.requiredLevel)}</dd>
            </div>
          </div>
          <div className="item-tooltip-req-row">
            <div className={`item-tooltip-req ${reqTone(details.requiredStr)}`}>
              <dt>{REQ_LABELS.str}</dt>
              <dd>{formatReq(details.requiredStr)}</dd>
            </div>
            <div className={`item-tooltip-req ${reqTone(details.requiredLuk)}`}>
              <dt>{REQ_LABELS.luk}</dt>
              <dd>{formatReq(details.requiredLuk)}</dd>
            </div>
          </div>
          <div className="item-tooltip-req-row">
            <div className={`item-tooltip-req ${reqTone(details.requiredDex)}`}>
              <dt>{REQ_LABELS.dex}</dt>
              <dd>{formatReq(details.requiredDex)}</dd>
            </div>
            <div className={`item-tooltip-req ${reqTone(details.requiredInt)}`}>
              <dt>{REQ_LABELS.int}</dt>
              <dd>{formatReq(details.requiredInt)}</dd>
            </div>
          </div>
        </dl>
      </section>

      <div className="item-tooltip-jobs">
        {ALL_JOB_LABELS.map((job) => (
          <span
            key={job}
            className={details.requiredJobs.includes(job) ? "is-enabled" : "is-disabled"}
          >
            {job}
          </span>
        ))}
      </div>

      <div className="item-tooltip-divider" aria-hidden="true" />

      <section className="item-tooltip-details">
        <p className="item-tooltip-line is-full">{details.categoryLine}</p>
        {details.attackSpeedLabel ? (
          <p className="item-tooltip-line">
            <span className="item-tooltip-label">{"攻擊速度 : "}</span>
            <span className="item-tooltip-value">{details.attackSpeedLabel}</span>
          </p>
        ) : null}
        {details.statLines.map((line) => (
          <p className="item-tooltip-line" key={`${line.label}:${line.value}`}>
            <span className="item-tooltip-label">{`${line.label} : `}</span>
            <span className="item-tooltip-value">{line.value}</span>
          </p>
        ))}
        {details.scrollCount !== null ? (
          <p className="item-tooltip-line is-accent">
            <span className="item-tooltip-label">{"可使用卷軸次數 : "}</span>
            <span className="item-tooltip-value">{details.scrollCount}</span>
          </p>
        ) : null}
      </section>

      {details.description ? (
        <>
          <div className="item-tooltip-divider" aria-hidden="true" />
          <section className="item-tooltip-flavor">
            <p>{details.description}</p>
          </section>
        </>
      ) : null}
      </article>
      {menu
        ? createPortal(
            <div
              ref={menuRef}
              className="tooltip-context-menu"
              style={{ left: menu.x, top: menu.y }}
              role="menu"
            >
              <button type="button" role="menuitem" onClick={saveAsPng}>
                儲存成 PNG
              </button>
            </div>,
            document.body,
          )
        : null}
    </>
  );
}
