import { useEffect, useState } from "react";
import {
  formatValue,
  getImage,
  getItemTooltip,
  getNode,
  typeLabel,
  type ItemTooltip as ItemTooltipData,
  type NodeDetail as Detail,
} from "../api";
import { ImagePreview } from "./ImagePreview";
import { ItemTooltip } from "./ItemTooltip";

interface NodeDetailProps {
  path: string | null;
  emptyMode?: "selection" | "category";
}

type ImageState = "idle" | "loading" | "none" | "error";

export function NodeDetail({
  path,
  emptyMode = "selection",
}: NodeDetailProps) {
  const [detail, setDetail] = useState<Detail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [imgUrl, setImgUrl] = useState<string | null>(null);
  const [imgState, setImgState] = useState<ImageState>("idle");
  const [imgMode, setImgMode] = useState<"exact" | "atlas">("exact");
  const [imgNote, setImgNote] = useState<string | null>(null);
  const [tooltip, setTooltip] = useState<ItemTooltipData | null>(null);

  // 裝備 tooltip：路徑落在 Character 裝備 img 內時由後端聚合回傳，否則為 null。
  useEffect(() => {
    if (!path) {
      setTooltip(null);
      return;
    }
    let active = true;
    getItemTooltip(path)
      .then((details) => {
        if (active) setTooltip(details);
      })
      .catch(() => {
        if (active) setTooltip(null);
      });
    return () => {
      active = false;
    };
  }, [path]);

  useEffect(() => {
    if (!path) {
      setDetail(null);
      setError(null);
      setLoading(false);
      return;
    }

    let active = true;
    setLoading(true);
    setError(null);
    setDetail(null);
    getNode(path)
      .then((nextDetail) => {
        if (active) setDetail(nextDetail);
      })
      .catch((nextError) => {
        if (!active) return;
        setDetail(null);
        setError(String(nextError));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [path]);

  useEffect(() => {
    if (
      !path ||
      !detail ||
      detail.path !== path ||
      (detail.value.type !== "Canvas" && detail.value.type !== "Uol")
    ) {
      setImgUrl(null);
      setImgState("idle");
      setImgMode("exact");
      setImgNote(null);
      return;
    }

    let active = true;
    setImgUrl(null);
    setImgState("loading");
    setImgMode("exact");
    setImgNote(null);
    getImage(path)
      .then((preview) => {
        if (!active) return;
        if (preview) {
          setImgUrl(`data:image/png;base64,${preview.base64}`);
          setImgMode(preview.mode);
          setImgNote(preview.note ?? null);
          setImgState("idle");
        } else {
          setImgState("none");
        }
      })
      .catch((nextError) => {
        if (!active) return;
        setImgUrl(null);
        setImgNote(String(nextError));
        setImgState("error");
      });
    return () => {
      active = false;
    };
  }, [detail, path]);

  if (!path) {
    if (emptyMode === "category") {
      return (
        <div className="detail-empty is-category">
          <span className="detail-empty-icon" aria-hidden="true">▣</span>
          <strong>這是分類節點</strong>
          <p>分類只用來整理資產；請展開後選擇 img 或其子節點進行預覽。</p>
        </div>
      );
    }
    return (
      <div className="detail-empty">
        <span className="detail-empty-icon" aria-hidden="true">◇</span>
        <strong>選取一個節點開始檢視</strong>
        <p>使用左側資產樹，或在搜尋列輸入資產名稱與路徑。</p>
      </div>
    );
  }
  if (loading && !detail) return <div className="detail-empty">正在載入節點內容…</div>;
  if (error) return <div className="detail-empty error">{error}</div>;
  if (!detail) return null;

  const isImageCapable = detail.value.type === "Canvas" || detail.value.type === "Uol";
  const valueText = formatValue(detail.value);
  const showPreview = Boolean(imgUrl) || isImageCapable;
  const useTextarea = valueText.includes("\n") || valueText.length > 120;

  return (
    <article className="node-detail-panel">
      <header className="detail-header">
        <div className="detail-heading">
          <p>節點檢視</p>
          <h1 title={detail.name}>{detail.name}</h1>
        </div>
        <div className="detail-actions">
          <span className="readonly-badge">唯讀</span>
        </div>
      </header>

      <div className={`detail-workspace${showPreview ? " has-preview" : ""}`}>
        {showPreview ? (
          <ImagePreview
            alt={detail.name}
            errorMessage={imgState === "error" ? imgNote : null}
            imageUrl={imgUrl}
            loading={imgState === "loading"}
            mode={imgMode}
            note={imgNote}
          />
        ) : null}

        <div className="detail-editor-stack">
          {tooltip ? <ItemTooltip details={tooltip} /> : null}

          <section className="editor-card">
            <header className="editor-card-heading">
              <div>
                <strong>節點屬性</strong>
                <span>目前資料來源不會被修改</span>
              </div>
              <span className={`type-badge tag-${detail.type_tag}`}>
                {typeLabel(detail.type_tag)}
              </span>
            </header>
            <div className="node-editor-form">
              <label>
                <span>名稱</span>
                <input readOnly value={detail.name} />
              </label>
              <label>
                <span>型別</span>
                <input readOnly value={`${typeLabel(detail.type_tag)} · ${detail.type_tag}`} />
              </label>
              {detail.value.type !== "SubTree" ? (
                <label className="is-wide">
                  <span>值</span>
                  {useTextarea ? (
                    <textarea readOnly spellCheck={false} value={valueText} />
                  ) : (
                    <input className="value-input" readOnly spellCheck={false} value={valueText} />
                  )}
                </label>
              ) : null}
            </div>
          </section>

          {detail.text_asset != null ? (
            <section className="editor-card text-editor-card">
              <header className="editor-card-heading">
                <div>
                  <strong>TextAsset</strong>
                  <span>{detail.text_asset.length.toLocaleString()} 字元</span>
                </div>
              </header>
              <textarea
                className="text-asset-editor"
                readOnly
                spellCheck={false}
                value={
                  detail.text_asset.length > 20000
                    ? `${detail.text_asset.slice(0, 20000)}\n…（內容過長，預覽已截斷）`
                    : detail.text_asset
                }
              />
            </section>
          ) : null}

        </div>
      </div>
    </article>
  );
}
