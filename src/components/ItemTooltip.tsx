import { useState, type CSSProperties } from "react";
import type { ItemTooltip as ItemTooltipData } from "../api";

/** 呈現方式對齊 Harepacker-MPST 的 ItemTooltipCard（gear 版本）。 */

const ALL_JOB_LABELS = ["初心者", "劍士", "法師", "弓箭手", "盜賊", "海盜"];
const ICON_SCALE = 2;
const ICON_MIN_SIZE = 34;
const ICON_MAX_SIZE = 68;

const numberFormatter = new Intl.NumberFormat();

function formatReq(value: number, zeroAsTriple = false): string {
  if (value === 0 && zeroAsTriple) return "000";
  return numberFormatter.format(value);
}

function reqTone(value: number, highlightLevel = false): string {
  if (value === 0) return "is-zero";
  return highlightLevel ? "is-level" : "is-active";
}

export function ItemTooltip({ details }: { details: ItemTooltipData }) {
  const [iconSize, setIconSize] = useState<{ width: number; height: number } | null>(null);

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
    <article className="item-tooltip" aria-label="裝備 tooltip">
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
              <dt>REQ LEV</dt>
              <dd>{formatReq(details.requiredLevel)}</dd>
            </div>
          </div>
          <div className="item-tooltip-req-row">
            <div className={`item-tooltip-req ${reqTone(details.requiredStr)}`}>
              <dt>REQ STR</dt>
              <dd>{formatReq(details.requiredStr, true)}</dd>
            </div>
            <div className={`item-tooltip-req ${reqTone(details.requiredLuk)}`}>
              <dt>REQ LUK</dt>
              <dd>{formatReq(details.requiredLuk, true)}</dd>
            </div>
          </div>
          <div className="item-tooltip-req-row">
            <div className={`item-tooltip-req ${reqTone(details.requiredDex)}`}>
              <dt>REQ DEX</dt>
              <dd>{formatReq(details.requiredDex, true)}</dd>
            </div>
            <div className={`item-tooltip-req ${reqTone(details.requiredInt)}`}>
              <dt>REQ INT</dt>
              <dd>{formatReq(details.requiredInt, true)}</dd>
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
            <span className="item-tooltip-label">攻擊速度 : </span>
            <span className="item-tooltip-value">{details.attackSpeedLabel}</span>
          </p>
        ) : null}
        {details.statLines.map((line) => (
          <p className="item-tooltip-line" key={`${line.label}:${line.value}`}>
            <span className="item-tooltip-label">{line.label} : </span>
            <span className="item-tooltip-value">{line.value}</span>
          </p>
        ))}
        {details.scrollCount !== null ? (
          <p className="item-tooltip-line is-accent">
            <span className="item-tooltip-label">可使用卷軸次數 : </span>
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
  );
}
