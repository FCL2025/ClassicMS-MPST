import { useEffect, useLayoutEffect, useRef, useState } from "react";
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

interface SkillStringInfo {
  name: string | null;
  desc: string | null;
  levelDesc: string | null;
}

type SkillStringTable = Record<
  string,
  { Name?: string; Desc?: string; LevelDesc?: Record<string, string> }
>;

// String/TW/Skill 是大型 TextAsset JSON（技能 id → {Name, Desc, ...}），解析一次後整個 session 共用。
let skillStringsPromise: Promise<SkillStringTable> | null = null;

function loadSkillStrings(): Promise<SkillStringTable> {
  if (!skillStringsPromise) {
    skillStringsPromise = getNode("String/TW/Skill").then((node) => {
      try {
        return JSON.parse(node.text_asset ?? "{}") as SkillStringTable;
      } catch {
        return {};
      }
    });
  }
  return skillStringsPromise;
}

// 僅在路徑剛好是技能代碼節點（Skill/<職業>/skill/<技能id>）時回傳技能 id。
function skillIdFromPath(path: string): string | null {
  const segments = path.split("/");
  if (
    segments.length === 4 &&
    segments[0] === "Skill" &&
    segments[2] === "skill" &&
    /^\d+$/.test(segments[3])
  ) {
    return segments[3];
  }
  return null;
}

// LevelDesc 是 {等級: 說明} 的物件，攤平成「Lv.1 …」逐行文字，等級由小到大排序。
function formatLevelDesc(
  levelDesc: Record<string, string> | undefined,
): string | null {
  if (!levelDesc) return null;
  const lines = Object.entries(levelDesc)
    .sort(([a], [b]) => Number(a) - Number(b))
    .map(([level, text]) => `Lv.${level} ${text}`);
  return lines.length > 0 ? lines.join("\n") : null;
}

// 唯讀 textarea，高度依內容自動撐開，不需要捲動或手動拖曳。
function AutoTextarea({ value }: { value: string }) {
  const ref = useRef<HTMLTextAreaElement>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [value]);
  return <textarea ref={ref} readOnly spellCheck={false} value={value} />;
}

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
  const [skillInfo, setSkillInfo] = useState<SkillStringInfo | null>(null);

  // 技能名稱/描述：路徑剛好是技能代碼節點時，查 String/TW/Skill 的名稱表。
  useEffect(() => {
    const skillId = path ? skillIdFromPath(path) : null;
    if (!skillId) {
      setSkillInfo(null);
      return;
    }
    let active = true;
    loadSkillStrings()
      .then((table) => {
        if (!active) return;
        // Skill img 的 id 可能補零（如 10000012），字串表的鍵沒有 → 兩種都試
        const stripped = skillId.replace(/^0+/, "");
        const entry = table[skillId] ?? (stripped ? table[stripped] : undefined);
        setSkillInfo(
          entry
            ? {
                name: entry.Name ?? null,
                desc: entry.Desc ?? null,
                levelDesc: formatLevelDesc(entry.LevelDesc),
              }
            : null,
        );
      })
      .catch(() => {
        if (active) setSkillInfo(null);
      });
    return () => {
      active = false;
    };
  }, [path]);

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

          {skillInfo ? (
            <section className="editor-card skill-info-card">
              <header className="editor-card-heading">
                <div>
                  <strong>技能資訊</strong>
                  <span>來源：String/TW/Skill</span>
                </div>
              </header>
              <div className="node-editor-form">
                <label className="is-wide">
                  <span>技能名稱</span>
                  <input readOnly value={skillInfo.name ?? "（查無名稱）"} />
                </label>
                <label className="is-wide">
                  <span>技能描述</span>
                  <AutoTextarea value={skillInfo.desc ?? "（查無描述）"} />
                </label>
                {skillInfo.levelDesc ? (
                  <label className="is-wide">
                    <span>每級數值</span>
                    <AutoTextarea value={skillInfo.levelDesc} />
                  </label>
                ) : null}
              </div>
            </section>
          ) : null}

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
